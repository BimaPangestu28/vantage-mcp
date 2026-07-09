use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ErrorData, Json, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use vantage_core::{
    Bounds, ClipboardAccess, ClipboardKind, ClipboardPrefer, DisplayInfo, InputController,
    ScreenCapturer, TextRecognizer, WindowFilter, WindowInfo, WindowInspector, WindowText,
};

use crate::error_map::to_mcp_error;
use crate::image_out::{downscale, rgba_to_base64_png, DEFAULT_MAX_DIMENSION};

/// Default accessibility-tree walk depth for `read_window_text` when the
/// caller omits `depth`.
pub const DEFAULT_DEPTH: u32 = 20;
/// Hard cap on accessibility-tree walk depth for `read_window_text`,
/// regardless of what the caller requests.
pub const MAX_DEPTH: u32 = 50;

/// What a capture tool should return: OCR text, a downscaled PNG, or both.
/// Shared by `capture_region` and `capture_window`.
#[derive(PartialEq, Clone, Copy)]
enum CaptureMode {
    Text,
    Image,
    Both,
}

/// Parse the `output` parameter accepted by the capture tools.
fn parse_mode(output: Option<&str>) -> Result<CaptureMode, ErrorData> {
    match output {
        None | Some("text") => Ok(CaptureMode::Text),
        Some("image") => Ok(CaptureMode::Image),
        Some("both") => Ok(CaptureMode::Both),
        Some(other) => Err(ErrorData::invalid_params(
            format!("output must be \"text\", \"image\", or \"both\", got {other:?}"),
            None,
        )),
    }
}

/// Clamp/normalize the `max_dimension` parameter (0/absent → default cap).
fn clamp_max_dim(max_dimension: Option<u32>) -> u32 {
    match max_dimension {
        None | Some(0) => DEFAULT_MAX_DIMENSION,
        Some(n) => n.min(DEFAULT_MAX_DIMENSION),
    }
}

/// Turn a captured frame into text-first `CaptureOutput` per `mode`, running OCR
/// and/or downscaling + PNG-encoding as needed. Shared by both capture tools.
fn frame_to_output(
    frame: vantage_core::RgbaImage,
    mode: CaptureMode,
    max_dim: u32,
    ocr: &std::sync::Arc<dyn TextRecognizer>,
) -> Result<CaptureOutput, vantage_core::CaptureError> {
    let text = if mode != CaptureMode::Image {
        Some(ocr.recognize(&frame)?)
    } else {
        None
    };
    let image = if mode != CaptureMode::Text {
        Some(rgba_to_base64_png(&downscale(&frame, max_dim)?)?)
    } else {
        None
    };
    Ok(CaptureOutput { text, image })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListWindowsParams {
    /// Only return windows whose owning application name equals this.
    #[serde(default)]
    pub app_filter: Option<String>,
    /// Restrict to on-screen windows. Defaults to true.
    #[serde(default)]
    pub on_screen_only: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadWindowTextParams {
    /// Target window id (from list_windows).
    pub window_id: u32,
    /// Accessibility-tree depth to walk. Defaults to 20, capped at 50.
    #[serde(default)]
    pub depth: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CaptureRegionParams {
    pub bounds: Bounds,
    /// "text" (default, OCR only, no pixels), "image", or "both".
    #[serde(default)]
    pub output: Option<String>,
    /// Cap the largest image side. Defaults to 1024; always enforced.
    #[serde(default)]
    pub max_dimension: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CaptureWindowParams {
    /// Target window id (from list_windows).
    pub window_id: u32,
    /// "text" (default, OCR only, no pixels), "image", or "both".
    #[serde(default)]
    pub output: Option<String>,
    /// Cap the largest image side. Defaults to 1024; always enforced.
    #[serde(default)]
    pub max_dimension: Option<u32>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CaptureOutput {
    pub text: Option<String>,
    /// base64-encoded PNG, present only when output includes an image.
    pub image: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadClipboardParams {
    /// "text" (default) or "image".
    #[serde(default)]
    pub prefer: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ClipboardOutput {
    pub kind: ClipboardKind,
    pub text: Option<String>,
    /// base64-encoded PNG when an image is present.
    pub image: Option<String>,
}

/// Object wrapper around the window list.
///
/// The MCP spec requires a tool's `outputSchema` root to be an `object`, and
/// rmcp 2.1.0 enforces this at schema-generation time — returning a bare
/// `Vec<_>` (JSON array root) panics the server when `tools/list` runs. The
/// windows are therefore nested under a single `windows` field.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ListWindowsResult {
    /// The matching windows, in the backend's native ordering.
    pub windows: Vec<WindowInfo>,
}

/// Object wrapper around the display list (rmcp requires an object outputSchema
/// root — see `ListWindowsResult`).
#[derive(Debug, Serialize, JsonSchema)]
pub struct ListDisplaysResult {
    pub displays: Vec<DisplayInfo>,
}

/// Shared success acknowledgement returned by the act tools.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AckOutput {
    pub ok: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClipboardWriteParams {
    /// Text to place on the system clipboard.
    pub text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FocusWindowParams {
    /// Target window id (from list_windows).
    pub window_id: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TypeTextParams {
    /// Text to type as synthetic keystrokes.
    pub text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClickParams {
    pub x: i32,
    pub y: i32,
    /// "left" (default), "right", or "middle".
    #[serde(default)]
    pub button: vantage_core::MouseButton,
}

/// The MCP server handler. Holds injected, platform-agnostic backends and the
/// composed tool router. Read tools (`windows`/`capturer`/`ocr`/`clipboard`) are
/// always mounted; the act tools (`input`) are mounted only when the act gate is
/// enabled, so a side-effecting tool is never even present unless the operator
/// opted in.
#[derive(Clone)]
pub struct Vantage {
    pub(crate) windows: Arc<dyn WindowInspector>,
    pub(crate) capturer: Arc<dyn ScreenCapturer>,
    pub(crate) ocr: Arc<dyn TextRecognizer>,
    pub(crate) clipboard: Arc<dyn ClipboardAccess>,
    pub(crate) input: Arc<dyn InputController>,
    tool_router: ToolRouter<Self>,
}

impl Vantage {
    /// Construct the handler. `allow_act` gates the act tools: when false, the
    /// act router is never merged and those tools are absent from `tools/list`
    /// and uncallable.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        windows: Arc<dyn WindowInspector>,
        capturer: Arc<dyn ScreenCapturer>,
        ocr: Arc<dyn TextRecognizer>,
        clipboard: Arc<dyn ClipboardAccess>,
        input: Arc<dyn InputController>,
        allow_act: bool,
    ) -> Self {
        let mut tool_router = Self::read_tool_router();
        if allow_act {
            tool_router.merge(Self::act_tool_router());
        }
        Self {
            windows,
            capturer,
            ocr,
            clipboard,
            input,
            tool_router,
        }
    }
}

/// The read (non-mutating) tools — always mounted.
#[tool_router(router = read_tool_router)]
impl Vantage {
    /// List on-screen windows: window_id, owning app, title, bounds, and focus.
    /// Primary entry point for an agent to orient before reading a window.
    #[tool(description = "List on-screen windows (id, app, title, bounds, focused).")]
    pub async fn list_windows(
        &self,
        Parameters(params): Parameters<ListWindowsParams>,
    ) -> Result<Json<ListWindowsResult>, ErrorData> {
        let filter = WindowFilter {
            app_filter: params.app_filter,
            on_screen_only: params.on_screen_only.unwrap_or(true),
        };
        let windows = self.windows.clone();
        let result = tokio::task::spawn_blocking(move || windows.list_windows(filter))
            .await
            .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?;
        result
            .map(|windows| Json(ListWindowsResult { windows }))
            .map_err(to_mcp_error)
    }

    /// Read a window's content as text via the accessibility tree.
    /// Cheapest way to get window content; prefer this over screenshot + OCR.
    #[tool(description = "Read a window's accessibility text (cheapest window-content read).")]
    pub async fn read_window_text(
        &self,
        Parameters(params): Parameters<ReadWindowTextParams>,
    ) -> Result<Json<WindowText>, ErrorData> {
        let depth = params.depth.unwrap_or(DEFAULT_DEPTH).min(MAX_DEPTH);
        let window_id = params.window_id;
        let windows = self.windows.clone();
        let result =
            tokio::task::spawn_blocking(move || windows.read_window_text(window_id, depth))
                .await
                .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?;
        result.map(Json).map_err(to_mcp_error)
    }

    /// Capture a screen region. Defaults to output=text: runs OCR and returns
    /// text only, no pixels (keeps token cost low). Use output=image/both when
    /// visual layout matters; images are downscaled to max_dimension.
    #[tool(description = "Capture a screen region; defaults to OCR text (no image).")]
    pub async fn capture_region(
        &self,
        Parameters(params): Parameters<CaptureRegionParams>,
    ) -> Result<Json<CaptureOutput>, ErrorData> {
        let mode = parse_mode(params.output.as_deref())?;
        let max_dim = clamp_max_dim(params.max_dimension);
        let bounds = params.bounds;
        let capturer = self.capturer.clone();
        let ocr = self.ocr.clone();

        let out = tokio::task::spawn_blocking(move || {
            let frame = capturer.capture_region(bounds)?;
            frame_to_output(frame, mode, max_dim, &ocr)
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?
        .map_err(to_mcp_error)?;

        Ok(Json(out))
    }

    /// Capture a single window by id (from `list_windows`). Text-first like
    /// `capture_region`. Not available on Wayland (returns an actionable error).
    #[tool(description = "Capture one window by id; defaults to OCR text (no image).")]
    pub async fn capture_window(
        &self,
        Parameters(params): Parameters<CaptureWindowParams>,
    ) -> Result<Json<CaptureOutput>, ErrorData> {
        let mode = parse_mode(params.output.as_deref())?;
        let max_dim = clamp_max_dim(params.max_dimension);
        let window_id = params.window_id;
        let windows = self.windows.clone();
        let capturer = self.capturer.clone();
        let ocr = self.ocr.clone();

        let out = tokio::task::spawn_blocking(move || {
            let target = windows
                .list_windows(WindowFilter {
                    app_filter: None,
                    on_screen_only: false,
                })?
                .into_iter()
                .find(|w| w.window_id == window_id)
                .ok_or(vantage_core::CaptureError::WindowNotFound(window_id))?;
            let frame = capturer.capture_window(&target)?;
            frame_to_output(frame, mode, max_dim, &ocr)
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?
        .map_err(to_mcp_error)?;

        Ok(Json(out))
    }

    /// Read the system clipboard. Defaults to preferring text.
    #[tool(description = "Read the clipboard (text by default; image as base64 PNG).")]
    pub async fn read_clipboard(
        &self,
        Parameters(params): Parameters<ReadClipboardParams>,
    ) -> Result<Json<ClipboardOutput>, ErrorData> {
        let prefer = match params.prefer.as_deref() {
            None | Some("text") => ClipboardPrefer::Text,
            Some("image") => ClipboardPrefer::Image,
            Some(other) => {
                return Err(ErrorData::invalid_params(
                    format!("prefer must be \"text\" or \"image\", got {other:?}"),
                    None,
                ))
            }
        };
        let clipboard = self.clipboard.clone();
        let content = tokio::task::spawn_blocking(move || clipboard.read(prefer))
            .await
            .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?
            .map_err(to_mcp_error)?;

        let image = match content.image {
            Some(img) => Some(rgba_to_base64_png(&img).map_err(to_mcp_error)?),
            None => None,
        };
        Ok(Json(ClipboardOutput {
            kind: content.kind,
            text: content.text,
            image,
        }))
    }

    /// List connected displays: id, name, bounds, scale factor, and which is primary.
    #[tool(description = "List connected displays (id, name, bounds, scale, primary).")]
    pub async fn list_displays(&self) -> Result<Json<ListDisplaysResult>, ErrorData> {
        let capturer = self.capturer.clone();
        let displays = tokio::task::spawn_blocking(move || capturer.list_displays())
            .await
            .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?
            .map_err(to_mcp_error)?;
        Ok(Json(ListDisplaysResult { displays }))
    }
}

/// The act (mutating) tools — mounted only when the act gate is enabled.
#[tool_router(router = act_tool_router)]
impl Vantage {
    /// Write text to the system clipboard. (Act tool — requires the act gate.)
    #[tool(description = "Write text to the system clipboard.")]
    pub async fn clipboard_write(
        &self,
        Parameters(params): Parameters<ClipboardWriteParams>,
    ) -> Result<Json<AckOutput>, ErrorData> {
        tracing::info!("act: clipboard_write ({} chars)", params.text.len());
        let input = self.input.clone();
        tokio::task::spawn_blocking(move || input.write_clipboard(&params.text))
            .await
            .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?
            .map_err(to_mcp_error)?;
        Ok(Json(AckOutput { ok: true }))
    }

    /// Bring a window (from list_windows) to the foreground. (Act tool.)
    #[tool(description = "Focus/raise a window by id.")]
    pub async fn focus_window(
        &self,
        Parameters(params): Parameters<FocusWindowParams>,
    ) -> Result<Json<AckOutput>, ErrorData> {
        tracing::info!("act: focus_window {}", params.window_id);
        let window_id = params.window_id;
        let windows = self.windows.clone();
        let input = self.input.clone();
        tokio::task::spawn_blocking(move || {
            let target = windows
                .list_windows(WindowFilter {
                    app_filter: None,
                    on_screen_only: false,
                })?
                .into_iter()
                .find(|w| w.window_id == window_id)
                .ok_or(vantage_core::CaptureError::WindowNotFound(window_id))?;
            input.focus_window(&target)
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?
        .map_err(to_mcp_error)?;
        Ok(Json(AckOutput { ok: true }))
    }

    /// Type text as synthetic keystrokes into the focused window. (Act tool.)
    #[tool(description = "Type text as synthetic keystrokes.")]
    pub async fn type_text(
        &self,
        Parameters(params): Parameters<TypeTextParams>,
    ) -> Result<Json<AckOutput>, ErrorData> {
        tracing::info!("act: type_text ({} chars)", params.text.len());
        let input = self.input.clone();
        tokio::task::spawn_blocking(move || input.type_text(&params.text))
            .await
            .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?
            .map_err(to_mcp_error)?;
        Ok(Json(AckOutput { ok: true }))
    }

    /// Click the mouse at absolute screen coordinates. (Act tool.)
    #[tool(description = "Click the mouse at (x, y).")]
    pub async fn click(
        &self,
        Parameters(params): Parameters<ClickParams>,
    ) -> Result<Json<AckOutput>, ErrorData> {
        tracing::info!("act: click ({},{}) {:?}", params.x, params.y, params.button);
        let input = self.input.clone();
        let (x, y, button) = (params.x, params.y, params.button);
        tokio::task::spawn_blocking(move || input.click(x, y, button))
            .await
            .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?
            .map_err(to_mcp_error)?;
        Ok(Json(AckOutput { ok: true }))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Vantage {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder().enable_tools().build();
        ServerInfo::new(capabilities)
            .with_server_info(Implementation::new(
                "vantage-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Desktop capture for macOS. Prefer read_window_text over screenshots; \
                 capture_region defaults to OCR text (no image) to keep token cost low.",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use std::sync::Arc;
    use vantage_core::{
        Bounds, CaptureError, ClipboardContent, ClipboardPrefer, RgbaImage, WindowFilter, WindowId,
        WindowInfo, WindowText,
    };

    #[derive(Default)]
    pub(crate) struct MockWindows {
        pub windows: Vec<WindowInfo>,
        pub last_filter_on_screen_only: std::sync::Mutex<Option<bool>>,
    }
    impl WindowInspector for MockWindows {
        fn list_windows(&self, filter: WindowFilter) -> Result<Vec<WindowInfo>, CaptureError> {
            *self.last_filter_on_screen_only.lock().unwrap() = Some(filter.on_screen_only);
            let mut out = self.windows.clone();
            if let Some(app) = filter.app_filter {
                out.retain(|w| w.app == app);
            }
            Ok(out)
        }
        fn read_window_text(&self, _id: WindowId, _depth: u32) -> Result<WindowText, CaptureError> {
            Ok(WindowText {
                text: String::new(),
                truncated: false,
            })
        }
    }

    pub(crate) struct NoScreen;
    impl ScreenCapturer for NoScreen {
        fn capture_region(&self, _b: Bounds) -> Result<RgbaImage, CaptureError> {
            Err(CaptureError::Unsupported("mock".into()))
        }
        fn list_displays(&self) -> Result<Vec<vantage_core::DisplayInfo>, CaptureError> {
            Err(CaptureError::Unsupported("mock".into()))
        }
        fn capture_window(&self, _t: &WindowInfo) -> Result<RgbaImage, CaptureError> {
            Err(CaptureError::Unsupported("mock".into()))
        }
    }
    pub(crate) struct NoOcr;
    impl TextRecognizer for NoOcr {
        fn recognize(&self, _i: &RgbaImage) -> Result<String, CaptureError> {
            Err(CaptureError::Unsupported("mock".into()))
        }
    }
    pub(crate) struct NoClip;
    impl ClipboardAccess for NoClip {
        fn read(&self, _p: ClipboardPrefer) -> Result<ClipboardContent, CaptureError> {
            Err(CaptureError::Unsupported("mock".into()))
        }
    }

    pub(crate) struct NoInput;
    impl InputController for NoInput {
        fn write_clipboard(&self, _t: &str) -> Result<(), CaptureError> {
            Err(CaptureError::Unsupported("mock".into()))
        }
        fn type_text(&self, _t: &str) -> Result<(), CaptureError> {
            Err(CaptureError::Unsupported("mock".into()))
        }
        fn click(
            &self,
            _x: i32,
            _y: i32,
            _b: vantage_core::MouseButton,
        ) -> Result<(), CaptureError> {
            Err(CaptureError::Unsupported("mock".into()))
        }
        fn focus_window(&self, _t: &WindowInfo) -> Result<(), CaptureError> {
            Err(CaptureError::Unsupported("mock".into()))
        }
    }

    /// Build a `Vantage` for tests with a no-op input backend and the act gate
    /// off (the default). Tests that need the act gate on use `Vantage::new`
    /// directly with `allow_act = true`.
    fn mk_vantage(
        windows: Arc<dyn WindowInspector>,
        capturer: Arc<dyn ScreenCapturer>,
        ocr: Arc<dyn TextRecognizer>,
        clipboard: Arc<dyn ClipboardAccess>,
    ) -> Vantage {
        Vantage::new(windows, capturer, ocr, clipboard, Arc::new(NoInput), false)
    }

    pub(crate) fn vantage_with_windows(windows: Arc<MockWindows>) -> Vantage {
        mk_vantage(
            windows,
            Arc::new(NoScreen),
            Arc::new(NoOcr),
            Arc::new(NoClip),
        )
    }

    fn win(id: WindowId, app: &str, title: &str) -> WindowInfo {
        WindowInfo {
            window_id: id,
            app: app.into(),
            title: title.into(),
            bounds: Bounds {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
            focused: false,
        }
    }

    #[tokio::test]
    async fn list_windows_defaults_on_screen_only_true_and_filters_by_app() {
        let mock = Arc::new(MockWindows {
            windows: vec![win(1, "Safari", "A"), win(2, "Notes", "B")],
            ..Default::default()
        });
        let vantage = vantage_with_windows(mock.clone());

        let out = vantage
            .list_windows(Parameters(ListWindowsParams {
                app_filter: Some("Notes".into()),
                on_screen_only: None,
            }))
            .await
            .expect("ok");

        assert_eq!(out.0.windows.len(), 1);
        assert_eq!(out.0.windows[0].app, "Notes");
        assert_eq!(*mock.last_filter_on_screen_only.lock().unwrap(), Some(true));
    }

    #[tokio::test]
    async fn read_window_text_applies_default_and_caps_depth() {
        use std::sync::Mutex;

        struct DepthSpy {
            seen: Mutex<Vec<u32>>,
        }
        impl WindowInspector for DepthSpy {
            fn list_windows(&self, _f: WindowFilter) -> Result<Vec<WindowInfo>, CaptureError> {
                Ok(vec![])
            }
            fn read_window_text(
                &self,
                _id: WindowId,
                depth: u32,
            ) -> Result<WindowText, CaptureError> {
                self.seen.lock().unwrap().push(depth);
                Ok(WindowText {
                    text: "hi".into(),
                    truncated: false,
                })
            }
        }
        let spy = Arc::new(DepthSpy {
            seen: Mutex::new(vec![]),
        });
        let vantage = mk_vantage(
            spy.clone(),
            Arc::new(NoScreen),
            Arc::new(NoOcr),
            Arc::new(NoClip),
        );

        // default when omitted
        vantage
            .read_window_text(Parameters(ReadWindowTextParams {
                window_id: 1,
                depth: None,
            }))
            .await
            .unwrap();
        // caps when too large
        vantage
            .read_window_text(Parameters(ReadWindowTextParams {
                window_id: 1,
                depth: Some(999),
            }))
            .await
            .unwrap();

        assert_eq!(*spy.seen.lock().unwrap(), vec![DEFAULT_DEPTH, MAX_DEPTH]);
    }

    #[tokio::test]
    async fn capture_region_text_mode_runs_ocr_and_returns_no_image() {
        struct FakeScreen;
        impl ScreenCapturer for FakeScreen {
            fn capture_region(&self, _b: Bounds) -> Result<RgbaImage, CaptureError> {
                Ok(RgbaImage {
                    width: 2,
                    height: 2,
                    pixels: vec![0u8; 16],
                })
            }
            fn list_displays(&self) -> Result<Vec<vantage_core::DisplayInfo>, CaptureError> {
                Err(CaptureError::Unsupported("mock".into()))
            }
            fn capture_window(&self, _t: &WindowInfo) -> Result<RgbaImage, CaptureError> {
                Err(CaptureError::Unsupported("mock".into()))
            }
        }
        struct FakeOcr;
        impl TextRecognizer for FakeOcr {
            fn recognize(&self, _i: &RgbaImage) -> Result<String, CaptureError> {
                Ok("hello".into())
            }
        }
        let vantage = mk_vantage(
            Arc::new(MockWindows::default()),
            Arc::new(FakeScreen),
            Arc::new(FakeOcr),
            Arc::new(NoClip),
        );
        let out = vantage
            .capture_region(Parameters(CaptureRegionParams {
                bounds: Bounds {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 2,
                },
                output: None,
                max_dimension: None,
            }))
            .await
            .unwrap();
        assert_eq!(out.0.text.as_deref(), Some("hello"));
        assert!(out.0.image.is_none(), "text mode must not return pixels");
    }

    #[tokio::test]
    async fn capture_region_treats_max_dimension_zero_as_default_cap() {
        struct LargeFakeScreen;
        impl ScreenCapturer for LargeFakeScreen {
            fn capture_region(&self, _b: Bounds) -> Result<RgbaImage, CaptureError> {
                let width = 2000u32;
                let height = 1500u32;
                Ok(RgbaImage {
                    width,
                    height,
                    pixels: vec![0u8; (width * height * 4) as usize],
                })
            }
            fn list_displays(&self) -> Result<Vec<vantage_core::DisplayInfo>, CaptureError> {
                Err(CaptureError::Unsupported("mock".into()))
            }
            fn capture_window(&self, _t: &WindowInfo) -> Result<RgbaImage, CaptureError> {
                Err(CaptureError::Unsupported("mock".into()))
            }
        }
        let vantage = mk_vantage(
            Arc::new(MockWindows::default()),
            Arc::new(LargeFakeScreen),
            Arc::new(NoOcr),
            Arc::new(NoClip),
        );
        let out = vantage
            .capture_region(Parameters(CaptureRegionParams {
                bounds: Bounds {
                    x: 0,
                    y: 0,
                    width: 2000,
                    height: 1500,
                },
                output: Some("image".into()),
                max_dimension: Some(0),
            }))
            .await
            .unwrap();

        assert!(
            out.0.text.is_none(),
            "pure image mode must not run OCR or return text"
        );
        let b64 = out.0.image.expect("image output must be present");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        let longest_side = decoded.width().max(decoded.height());
        assert!(
            longest_side <= DEFAULT_MAX_DIMENSION,
            "max_dimension: 0 must be treated as the default cap ({DEFAULT_MAX_DIMENSION}), \
             got longest side {longest_side}"
        );
    }

    #[tokio::test]
    async fn read_clipboard_returns_text_and_defaults_to_text_prefer() {
        struct ClipText;
        impl ClipboardAccess for ClipText {
            fn read(&self, prefer: ClipboardPrefer) -> Result<ClipboardContent, CaptureError> {
                assert_eq!(prefer, ClipboardPrefer::Text);
                Ok(ClipboardContent {
                    kind: vantage_core::ClipboardKind::Text,
                    text: Some("copied".into()),
                    image: None,
                })
            }
        }
        let vantage = mk_vantage(
            Arc::new(MockWindows::default()),
            Arc::new(NoScreen),
            Arc::new(NoOcr),
            Arc::new(ClipText),
        );
        let out = vantage
            .read_clipboard(Parameters(ReadClipboardParams { prefer: None }))
            .await
            .unwrap();
        assert_eq!(out.0.text.as_deref(), Some("copied"));
        assert!(out.0.image.is_none());
    }

    #[tokio::test]
    async fn read_clipboard_rejects_bad_prefer() {
        let vantage = mk_vantage(
            Arc::new(MockWindows::default()),
            Arc::new(NoScreen),
            Arc::new(NoOcr),
            Arc::new(NoClip),
        );
        // `rmcp::Json<T>` does not implement `Debug`, so `Result::unwrap_err`
        // (which requires the `Ok` type to be `Debug`) is not usable here;
        // extract the error via `match` instead.
        let result = vantage
            .read_clipboard(Parameters(ReadClipboardParams {
                prefer: Some("video".into()),
            }))
            .await;
        let err = match result {
            Ok(_) => panic!("expected an error for an invalid prefer value"),
            Err(e) => e,
        };
        assert!(err.message.to_lowercase().contains("prefer"));
    }

    #[tokio::test]
    async fn list_displays_returns_displays_from_backend() {
        struct TwoDisplays;
        impl ScreenCapturer for TwoDisplays {
            fn capture_region(&self, _b: Bounds) -> Result<RgbaImage, CaptureError> {
                Err(CaptureError::Unsupported("mock".into()))
            }
            fn list_displays(&self) -> Result<Vec<vantage_core::DisplayInfo>, CaptureError> {
                Ok(vec![vantage_core::DisplayInfo {
                    display_id: 7,
                    name: "HDMI-1".into(),
                    bounds: Bounds {
                        x: 0,
                        y: 0,
                        width: 800,
                        height: 600,
                    },
                    scale_factor: 1.0,
                    is_primary: true,
                }])
            }
            fn capture_window(&self, _t: &WindowInfo) -> Result<RgbaImage, CaptureError> {
                Err(CaptureError::Unsupported("mock".into()))
            }
        }
        let vantage = mk_vantage(
            Arc::new(MockWindows::default()),
            Arc::new(TwoDisplays),
            Arc::new(NoOcr),
            Arc::new(NoClip),
        );
        let out = vantage.list_displays().await.expect("ok");
        assert_eq!(out.0.displays.len(), 1);
        assert_eq!(out.0.displays[0].name, "HDMI-1");
        assert!(out.0.displays[0].is_primary);
    }

    #[tokio::test]
    async fn capture_window_resolves_id_then_captures_text() {
        struct WinScreen;
        impl ScreenCapturer for WinScreen {
            fn capture_region(&self, _b: Bounds) -> Result<RgbaImage, CaptureError> {
                Err(CaptureError::Unsupported("mock".into()))
            }
            fn list_displays(&self) -> Result<Vec<vantage_core::DisplayInfo>, CaptureError> {
                Ok(vec![])
            }
            fn capture_window(&self, target: &WindowInfo) -> Result<RgbaImage, CaptureError> {
                assert_eq!(target.window_id, 1);
                assert_eq!(target.app, "Safari");
                Ok(RgbaImage {
                    width: 2,
                    height: 2,
                    pixels: vec![0u8; 16],
                })
            }
        }
        struct FakeOcr2;
        impl TextRecognizer for FakeOcr2 {
            fn recognize(&self, _i: &RgbaImage) -> Result<String, CaptureError> {
                Ok("win-text".into())
            }
        }
        let mock = Arc::new(MockWindows {
            windows: vec![win(1, "Safari", "A")],
            ..Default::default()
        });
        let vantage = mk_vantage(
            mock,
            Arc::new(WinScreen),
            Arc::new(FakeOcr2),
            Arc::new(NoClip),
        );
        let out = vantage
            .capture_window(Parameters(CaptureWindowParams {
                window_id: 1,
                output: None,
                max_dimension: None,
            }))
            .await
            .unwrap();
        assert_eq!(out.0.text.as_deref(), Some("win-text"));
        assert!(out.0.image.is_none(), "text mode must not return pixels");
    }

    #[tokio::test]
    async fn capture_window_unknown_id_errors() {
        let vantage = mk_vantage(
            Arc::new(MockWindows::default()),
            Arc::new(NoScreen),
            Arc::new(NoOcr),
            Arc::new(NoClip),
        );
        let result = vantage
            .capture_window(Parameters(CaptureWindowParams {
                window_id: 999,
                output: None,
                max_dimension: None,
            }))
            .await;
        let err = match result {
            Ok(_) => panic!("expected an error for an unknown window id"),
            Err(e) => e,
        };
        assert!(err.message.contains("999"));
    }

    fn vantage_gated(allow_act: bool) -> Vantage {
        Vantage::new(
            Arc::new(MockWindows::default()),
            Arc::new(NoScreen),
            Arc::new(NoOcr),
            Arc::new(NoClip),
            Arc::new(NoInput),
            allow_act,
        )
    }

    #[test]
    fn act_tools_absent_when_gate_off_present_when_on() {
        let off = vantage_gated(false);
        assert!(
            !off.tool_router.has_route("clipboard_write"),
            "act tool must be unmounted when the gate is off"
        );
        // Read tools are always mounted.
        assert!(off.tool_router.has_route("list_windows"));

        let on = vantage_gated(true);
        assert!(
            on.tool_router.has_route("clipboard_write"),
            "act tool must be mounted when the gate is on"
        );
    }

    #[tokio::test]
    async fn clipboard_write_forwards_to_input() {
        use std::sync::Mutex;
        struct RecInput(Mutex<Option<String>>);
        impl InputController for RecInput {
            fn write_clipboard(&self, t: &str) -> Result<(), CaptureError> {
                *self.0.lock().unwrap() = Some(t.to_owned());
                Ok(())
            }
            fn type_text(&self, _t: &str) -> Result<(), CaptureError> {
                Ok(())
            }
            fn click(
                &self,
                _x: i32,
                _y: i32,
                _b: vantage_core::MouseButton,
            ) -> Result<(), CaptureError> {
                Ok(())
            }
            fn focus_window(&self, _t: &WindowInfo) -> Result<(), CaptureError> {
                Ok(())
            }
        }
        let rec = Arc::new(RecInput(Mutex::new(None)));
        let vantage = Vantage::new(
            Arc::new(MockWindows::default()),
            Arc::new(NoScreen),
            Arc::new(NoOcr),
            Arc::new(NoClip),
            rec.clone(),
            true,
        );
        let out = vantage
            .clipboard_write(Parameters(ClipboardWriteParams {
                text: "hello".into(),
            }))
            .await
            .unwrap();
        assert!(out.0.ok);
        assert_eq!(rec.0.lock().unwrap().as_deref(), Some("hello"));
    }

    #[tokio::test]
    async fn focus_window_unknown_id_errors() {
        let vantage = vantage_gated(true);
        let result = vantage
            .focus_window(Parameters(FocusWindowParams { window_id: 424242 }))
            .await;
        let err = match result {
            Ok(_) => panic!("expected an error for an unknown window id"),
            Err(e) => e,
        };
        assert!(err.message.contains("424242"));
    }

    #[tokio::test]
    async fn type_text_and_click_forward_to_input() {
        use std::sync::Mutex;
        #[derive(Default)]
        struct RecInput {
            typed: Mutex<Option<String>>,
            clicked: Mutex<Option<(i32, i32, vantage_core::MouseButton)>>,
        }
        impl InputController for RecInput {
            fn write_clipboard(&self, _t: &str) -> Result<(), CaptureError> {
                Ok(())
            }
            fn type_text(&self, t: &str) -> Result<(), CaptureError> {
                *self.typed.lock().unwrap() = Some(t.to_owned());
                Ok(())
            }
            fn click(
                &self,
                x: i32,
                y: i32,
                b: vantage_core::MouseButton,
            ) -> Result<(), CaptureError> {
                *self.clicked.lock().unwrap() = Some((x, y, b));
                Ok(())
            }
            fn focus_window(&self, _t: &WindowInfo) -> Result<(), CaptureError> {
                Ok(())
            }
        }
        let rec = Arc::new(RecInput::default());
        let vantage = Vantage::new(
            Arc::new(MockWindows::default()),
            Arc::new(NoScreen),
            Arc::new(NoOcr),
            Arc::new(NoClip),
            rec.clone(),
            true,
        );
        vantage
            .type_text(Parameters(TypeTextParams {
                text: "hi there".into(),
            }))
            .await
            .unwrap();
        vantage
            .click(Parameters(ClickParams {
                x: 12,
                y: 34,
                button: vantage_core::MouseButton::Right,
            }))
            .await
            .unwrap();
        assert_eq!(rec.typed.lock().unwrap().as_deref(), Some("hi there"));
        assert_eq!(
            *rec.clicked.lock().unwrap(),
            Some((12, 34, vantage_core::MouseButton::Right))
        );
    }
}
