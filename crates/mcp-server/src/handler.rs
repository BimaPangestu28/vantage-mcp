use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ErrorData, Json, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use vantage_core::{
    ClipboardAccess, ScreenCapturer, TextRecognizer, WindowFilter, WindowInfo, WindowInspector,
    WindowText,
};

use crate::error_map::to_mcp_error;

/// Default accessibility-tree walk depth for `read_window_text` when the
/// caller omits `depth`.
pub const DEFAULT_DEPTH: u32 = 20;
/// Hard cap on accessibility-tree walk depth for `read_window_text`,
/// regardless of what the caller requests.
pub const MAX_DEPTH: u32 = 50;

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

/// The MCP server handler. Holds injected, platform-agnostic backends.
/// `windows` is read by `list_windows`; the remaining backends are wired in
/// now but not read until their tool methods land in later tasks (Tasks 5-7).
#[derive(Clone)]
pub struct Vantage {
    pub(crate) windows: Arc<dyn WindowInspector>,
    #[allow(dead_code)]
    pub(crate) capturer: Arc<dyn ScreenCapturer>,
    #[allow(dead_code)]
    pub(crate) ocr: Arc<dyn TextRecognizer>,
    #[allow(dead_code)]
    pub(crate) clipboard: Arc<dyn ClipboardAccess>,
}

// `#[tool_router]` generates `Self::tool_router()`, an associated function
// returning a `ToolRouter<Self>` built from every method in this impl block
// tagged `#[tool]`. There are none yet, so it returns an empty-but-valid
// router. `#[tool_handler]` below calls `Self::tool_router()` automatically,
// so no field is needed on the struct to hold it.
#[tool_router]
impl Vantage {
    pub fn new(
        windows: Arc<dyn WindowInspector>,
        capturer: Arc<dyn ScreenCapturer>,
        ocr: Arc<dyn TextRecognizer>,
        clipboard: Arc<dyn ClipboardAccess>,
    ) -> Self {
        Self {
            windows,
            capturer,
            ocr,
            clipboard,
        }
    }

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
}

#[tool_handler]
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
    use std::sync::Arc;
    use vantage_core::{
        Bounds, CaptureError, ClipboardContent, ClipboardPrefer, RgbaImage, WindowFilter,
        WindowId, WindowInfo, WindowText,
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
            Ok(WindowText { text: String::new(), truncated: false })
        }
    }

    pub(crate) struct NoScreen;
    impl ScreenCapturer for NoScreen {
        fn capture_region(&self, _b: Bounds) -> Result<RgbaImage, CaptureError> {
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

    pub(crate) fn vantage_with_windows(windows: Arc<MockWindows>) -> Vantage {
        Vantage::new(windows, Arc::new(NoScreen), Arc::new(NoOcr), Arc::new(NoClip))
    }

    fn win(id: WindowId, app: &str, title: &str) -> WindowInfo {
        WindowInfo {
            window_id: id,
            app: app.into(),
            title: title.into(),
            bounds: Bounds { x: 0, y: 0, width: 100, height: 100 },
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
            fn read_window_text(&self, _id: WindowId, depth: u32) -> Result<WindowText, CaptureError> {
                self.seen.lock().unwrap().push(depth);
                Ok(WindowText { text: "hi".into(), truncated: false })
            }
        }
        let spy = Arc::new(DepthSpy { seen: Mutex::new(vec![]) });
        let vantage = Vantage::new(spy.clone(), Arc::new(NoScreen), Arc::new(NoOcr), Arc::new(NoClip));

        // default when omitted
        vantage
            .read_window_text(Parameters(ReadWindowTextParams { window_id: 1, depth: None }))
            .await
            .unwrap();
        // caps when too large
        vantage
            .read_window_text(Parameters(ReadWindowTextParams { window_id: 1, depth: Some(999) }))
            .await
            .unwrap();

        assert_eq!(*spy.seen.lock().unwrap(), vec![DEFAULT_DEPTH, MAX_DEPTH]);
    }
}
