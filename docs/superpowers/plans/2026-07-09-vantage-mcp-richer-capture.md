# vantage-mcp Richer capture surface (Spec B) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two read tools — `list_displays` (enumerate monitors) and `capture_window` (capture one window by id, text-first) — across macOS and Linux (Wayland → actionable `Unsupported` for `capture_window`).

**Architecture:** Extend `vantage_core` additively: a `DisplayInfo` value type plus two new methods on the `ScreenCapturer` trait (`list_displays`, `capture_window`). Both platform backends and all test doubles implement them. The handler resolves a `window_id` to its `WindowInfo` via the existing `WindowInspector`, then reuses the existing `capture_region` post-capture pipeline (mode → OCR/downscale/PNG) for `capture_window`.

**Tech Stack:** Rust 1.95; `rmcp` 2.1.0; xcap 0.9 (`Monitor`/`Window`); the existing `image_out` OCR/PNG path.

## Global Constraints

- **Additive core change only:** add `DisplayInfo` and two `ScreenCapturer` methods. Do NOT change existing types, `CaptureError`/`ErrorKind`, or the other three traits.
- **Every `ScreenCapturer` implementor updates together:** `MacScreenCapturer`, `LinuxScreenCapturer`, Linux `capture_stub`, and the `handler.rs` test doubles (`NoScreen`, `FakeScreen`, `LargeFakeScreen`) — or the workspace won't compile.
- **Text-first preserved:** `capture_window` defaults to `output:"text"`; images downscale to `max_dimension` (default+cap 1024). Reuse the `capture_region` mode logic, do not re-derive it.
- **rmcp quirk:** a tool's `outputSchema` root must be a JSON object — wrap `Vec<DisplayInfo>` in `ListDisplaysResult { displays }`.
- **Blocking calls on `spawn_blocking`; stdout stays JSON-RPC only.**
- **Wayland `capture_window`:** detect `XDG_SESSION_TYPE == "wayland"` (or `WAYLAND_DISPLAY` set) and return `CaptureError::Unsupported` BEFORE calling xcap — never capture a wrong region.
- **No production `unwrap()`/`panic!`** in backends. Commit after each task's tests pass. Conventional commits.

---

### Task 1: Core — `DisplayInfo` + extend `ScreenCapturer`; update all implementors

Additive core change plus the mechanical fan-out so the workspace compiles.
This is one task because the trait change and every implementor must land
together (a reviewer can't accept a half-updated trait).

**Files:**
- Modify: `crates/core/src/types.rs` (add `DisplayInfo`)
- Modify: `crates/core/src/traits.rs` (extend `ScreenCapturer`)
- Modify: `crates/platform/macos/src/capture.rs` (impl 2 methods)
- Modify: `crates/platform/linux/src/capture.rs` (impl 2 methods)
- Modify: `crates/platform/linux/src/capture_stub.rs` (impl 2 methods)
- Modify: `crates/mcp-server/src/handler.rs` (update test doubles `NoScreen`/`FakeScreen`/`LargeFakeScreen`)
- Test: inline unit test in `crates/core/src/types.rs`

**Interfaces:**
- Produces: `vantage_core::DisplayInfo { display_id: u32, name: String, bounds: Bounds, scale_factor: f32, is_primary: bool }`; `ScreenCapturer::list_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError>`; `ScreenCapturer::capture_window(&self, target: &WindowInfo) -> Result<RgbaImage, CaptureError>`.

- [ ] **Step 1: Add `DisplayInfo` to `crates/core/src/types.rs`**

Append:

```rust
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DisplayInfo {
    pub display_id: u32,
    pub name: String,
    pub bounds: Bounds,
    pub scale_factor: f32,
    pub is_primary: bool,
}
```

- [ ] **Step 2: Add a shape unit test in `crates/core/src/types.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_info_serializes_expected_fields() {
        let d = DisplayInfo {
            display_id: 1,
            name: "eDP-1".into(),
            bounds: Bounds { x: 0, y: 0, width: 1920, height: 1080 },
            scale_factor: 2.0,
            is_primary: true,
        };
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["display_id"], 1);
        assert_eq!(v["name"], "eDP-1");
        assert_eq!(v["bounds"]["width"], 1920);
        assert_eq!(v["is_primary"], true);
    }
}
```

`serde_json` is already a dependency of `vantage-core`? It is not — `core` uses
`serde` + `schemars` only. Add `serde_json` under `[dev-dependencies]` in
`crates/core/Cargo.toml`:

```toml
[dev-dependencies]
serde_json = { workspace = true }
```

- [ ] **Step 3: Extend the `ScreenCapturer` trait in `crates/core/src/traits.rs`**

Add `DisplayInfo` and `WindowInfo` to the `use crate::types::{...}` import, then:

```rust
pub trait ScreenCapturer: Send + Sync {
    fn capture_region(&self, bounds: Bounds) -> Result<RgbaImage, CaptureError>;
    /// Enumerate connected displays (monitors).
    fn list_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError>;
    /// Capture a single window, identified by an already-resolved `WindowInfo`
    /// (from `WindowInspector::list_windows`). Not supported on Wayland.
    fn capture_window(&self, target: &WindowInfo) -> Result<RgbaImage, CaptureError>;
}
```

- [ ] **Step 4: Implement both methods in `crates/platform/macos/src/capture.rs`**

Add `use vantage_core::{DisplayInfo, WindowInfo};` to the imports, and inside
`impl ScreenCapturer for MacScreenCapturer`:

```rust
    fn list_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        list_displays_via_xcap()
    }

    fn capture_window(&self, target: &WindowInfo) -> Result<RgbaImage, CaptureError> {
        let windows = xcap::Window::all().map_err(|e| classify_capture_error(&e))?;
        let win = windows
            .into_iter()
            .find(|w| w.id().map(|id| id == target.window_id).unwrap_or(false))
            .ok_or(CaptureError::WindowNotFound(target.window_id))?;
        let shot = win.capture_image().map_err(|e| classify_capture_error(&e))?;
        Ok(RgbaImage { width: shot.width(), height: shot.height(), pixels: shot.into_raw() })
    }
```

And a shared free function (also used by Linux — but each crate has its own copy
since they don't depend on each other; define it in this file):

```rust
/// Enumerate monitors via xcap into `DisplayInfo`.
fn list_displays_via_xcap() -> Result<Vec<DisplayInfo>, CaptureError> {
    let monitors = xcap::Monitor::all().map_err(|e| classify_capture_error(&e))?;
    Ok(monitors
        .into_iter()
        .map(|m| DisplayInfo {
            display_id: m.id().unwrap_or(0),
            name: m.name().unwrap_or_default(),
            bounds: Bounds {
                x: m.x().unwrap_or(0),
                y: m.y().unwrap_or(0),
                width: m.width().unwrap_or(0),
                height: m.height().unwrap_or(0),
            },
            scale_factor: m.scale_factor().unwrap_or(1.0),
            is_primary: m.is_primary().unwrap_or(false),
        })
        .collect())
}
```

*macOS risk note:* if `xcap::Window::id()` on macOS is not the CGWindowID that
`window_id` holds, the `id()==window_id` match will miss. Fallback (apply if the
macOS live test in Task 4 fails): also accept a match on
`app_name()==target.app && title()==target.title`. Left as the primary path per
the spec; revisit only if the live run disproves it.

- [ ] **Step 5: Implement both methods in `crates/platform/linux/src/capture.rs`**

Add `use vantage_core::{DisplayInfo, WindowInfo};`, and inside
`impl ScreenCapturer for LinuxScreenCapturer`:

```rust
    fn list_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        let monitors = Monitor::all().map_err(|e| classify_capture_error(&e))?;
        Ok(monitors
            .into_iter()
            .map(|m| DisplayInfo {
                display_id: m.id().unwrap_or(0),
                name: m.name().unwrap_or_default(),
                bounds: Bounds {
                    x: m.x().unwrap_or(0),
                    y: m.y().unwrap_or(0),
                    width: m.width().unwrap_or(0),
                    height: m.height().unwrap_or(0),
                },
                scale_factor: m.scale_factor().unwrap_or(1.0),
                is_primary: m.is_primary().unwrap_or(false),
            })
            .collect())
    }

    fn capture_window(&self, target: &WindowInfo) -> Result<RgbaImage, CaptureError> {
        // Wayland compositors do not permit capturing arbitrary application
        // windows; refuse before touching xcap so we never grab a wrong region.
        let is_wayland = std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
            || std::env::var("WAYLAND_DISPLAY").is_ok();
        if is_wayland {
            return Err(CaptureError::Unsupported(
                "per-window capture is not available on Wayland; use capture_region with a \
                 display/region, or run under X11"
                    .into(),
            ));
        }
        let windows = xcap::Window::all().map_err(|e| classify_capture_error(&e))?;
        let matches: Vec<xcap::Window> = windows
            .into_iter()
            .filter(|w| {
                w.app_name().map(|a| a == target.app).unwrap_or(false)
                    && w.title().map(|t| t == target.title).unwrap_or(false)
            })
            .collect();
        let win = matches
            .iter()
            .find(|w| {
                w.x().unwrap_or(i32::MIN) == target.bounds.x
                    && w.y().unwrap_or(i32::MIN) == target.bounds.y
            })
            .or_else(|| matches.first())
            .ok_or(CaptureError::WindowNotFound(target.window_id))?;
        let shot = win.capture_image().map_err(|e| classify_capture_error(&e))?;
        Ok(RgbaImage { width: shot.width(), height: shot.height(), pixels: shot.into_raw() })
    }
```

- [ ] **Step 6: Implement both methods in `crates/platform/linux/src/capture_stub.rs`**

```rust
    fn list_displays(&self) -> Result<Vec<vantage_core::DisplayInfo>, CaptureError> {
        Err(CaptureError::Unsupported(
            "display enumeration was disabled at build time (the `capture` feature / xcap \
             system libraries were unavailable)"
                .into(),
        ))
    }

    fn capture_window(
        &self,
        _target: &vantage_core::WindowInfo,
    ) -> Result<RgbaImage, CaptureError> {
        Err(CaptureError::Unsupported(
            "screen capture was disabled at build time (the `capture` feature / xcap \
             system libraries were unavailable)"
                .into(),
        ))
    }
```

- [ ] **Step 7: Update the handler test doubles in `crates/mcp-server/src/handler.rs`**

Every `impl ScreenCapturer for …` in `#[cfg(test)] mod tests` needs the two new
methods. For `NoScreen`, `FakeScreen`, `LargeFakeScreen`, add:

```rust
        fn list_displays(&self) -> Result<Vec<vantage_core::DisplayInfo>, CaptureError> {
            Err(CaptureError::Unsupported("mock".into()))
        }
        fn capture_window(
            &self,
            _t: &vantage_core::WindowInfo,
        ) -> Result<RgbaImage, CaptureError> {
            Err(CaptureError::Unsupported("mock".into()))
        }
```

(For `FakeScreen`/`LargeFakeScreen`, returning `Unsupported` is fine — Task 3's
tests use purpose-built doubles for the success paths.) Import `DisplayInfo`/`WindowInfo`
via the existing `vantage_core::{…}` test import or fully-qualify as above.

- [ ] **Step 8: Build + test (lib-free is fine on this box)**

Run: `cargo build --workspace --no-default-features && cargo test --workspace --no-default-features`
Expected: PASS — core unit test for `DisplayInfo`, all existing tests still green,
workspace compiles with the extended trait.

- [ ] **Step 9: Commit**

```bash
git add crates/core crates/platform crates/mcp-server/src/handler.rs
git commit -m "feat(core): DisplayInfo + ScreenCapturer::{list_displays,capture_window}"
```

---

### Task 2: `list_displays` MCP tool + live test

**Files:**
- Modify: `crates/mcp-server/src/handler.rs` (tool + result type + mock test)
- Test: inline `#[cfg(test)]` in `handler.rs`; live test `crates/platform/linux/tests/displays_live.rs` (`#[ignore]`)

**Interfaces:**
- Consumes: `ScreenCapturer::list_displays`, `DisplayInfo`, `to_mcp_error`.
- Produces: MCP tool `list_displays` (no args) → `Json<ListDisplaysResult { displays: Vec<DisplayInfo> }>`.

- [ ] **Step 1: Add the failing handler test (append inside `mod tests`)**

```rust
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
                bounds: Bounds { x: 0, y: 0, width: 800, height: 600 },
                scale_factor: 1.0,
                is_primary: true,
            }])
        }
        fn capture_window(&self, _t: &WindowInfo) -> Result<RgbaImage, CaptureError> {
            Err(CaptureError::Unsupported("mock".into()))
        }
    }
    let vantage = Vantage::new(
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
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p vantage-mcp-server --no-default-features list_displays_returns 2>&1 | tail -20`
Expected: FAIL — `list_displays` method and `ListDisplaysResult` do not exist.

- [ ] **Step 3: Add the result type + tool method in `handler.rs`**

Add `DisplayInfo` to the `use vantage_core::{…}` import at the top. Add the
object-root wrapper near `ListWindowsResult`:

```rust
/// Object wrapper around the display list (rmcp requires an object outputSchema
/// root — see `ListWindowsResult`).
#[derive(Debug, Serialize, JsonSchema)]
pub struct ListDisplaysResult {
    pub displays: Vec<DisplayInfo>,
}
```

Add the tool inside `#[tool_router] impl Vantage`:

```rust
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
```

- [ ] **Step 4: Run the test to see it pass**

Run: `cargo test -p vantage-mcp-server --no-default-features list_displays_returns`
Expected: PASS.

- [ ] **Step 5: Write the live test `crates/platform/linux/tests/displays_live.rs`**

```rust
//! Live display-enumeration test. Needs a desktop session with >= 1 monitor.
//! Run: `cargo test -p vantage-platform-linux --test displays_live -- --ignored`
#![cfg(all(target_os = "linux", feature = "capture"))]

use vantage_core::ScreenCapturer;
use vantage_platform_linux::LinuxScreenCapturer;

#[test]
#[ignore = "requires a desktop session with a display"]
fn lists_at_least_one_display() {
    let cap = LinuxScreenCapturer::new();
    let displays = cap.list_displays().expect("list_displays");
    assert!(!displays.is_empty(), "expected at least one display");
    assert!(displays.iter().any(|d| d.bounds.width > 0 && d.bounds.height > 0));
}
```

- [ ] **Step 6: Run the live test (needs the `capture` feature + libs)**

Run: `cargo test -p vantage-platform-linux --test displays_live -- --ignored`
Expected: PASS — at least one display with non-zero bounds.

- [ ] **Step 7: Commit**

```bash
git add crates/mcp-server/src/handler.rs crates/platform/linux/tests/displays_live.rs
git commit -m "feat(server): list_displays tool + live test"
```

---

### Task 3: `capture_window` MCP tool (shared post-capture pipeline) + mock tests

**Files:**
- Modify: `crates/mcp-server/src/handler.rs` (refactor `capture_region` to share a helper; add `capture_window`)
- Test: inline `#[cfg(test)]` in `handler.rs`

**Interfaces:**
- Consumes: `WindowInspector::list_windows`, `ScreenCapturer::capture_window`, the `image_out` path.
- Produces: MCP tool `capture_window { window_id: u32, output?: String, max_dimension?: u32 }` → `Json<CaptureOutput>`; a private helper `fn frame_to_output(frame, mode, max_dim, ocr) -> Result<CaptureOutput, CaptureError>` shared with `capture_region`.

- [ ] **Step 1: Add the failing tests (append inside `mod tests`)**

```rust
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
            Ok(RgbaImage { width: 2, height: 2, pixels: vec![0u8; 16] })
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
    let vantage = Vantage::new(mock, Arc::new(WinScreen), Arc::new(FakeOcr2), Arc::new(NoClip));
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
    let vantage = Vantage::new(
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
        Ok(_) => panic!("expected error for unknown window id"),
        Err(e) => e,
    };
    assert!(err.message.contains("999"));
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p vantage-mcp-server --no-default-features capture_window 2>&1 | tail -20`
Expected: FAIL — `capture_window` / `CaptureWindowParams` do not exist.

- [ ] **Step 3: Refactor the shared post-capture pipeline out of `capture_region`**

In `handler.rs`, factor the mode-parse + OCR + downscale/PNG logic into a
private module-level helper so both tools share it. Add near the top:

```rust
#[derive(PartialEq, Clone, Copy)]
enum CaptureMode { Text, Image, Both }

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

/// Turn a captured frame into text-first `CaptureOutput` per `mode`, running OCR
/// and/or downscaling+PNG-encoding as needed. Shared by capture_region/window.
fn frame_to_output(
    frame: RgbaImage,
    mode: CaptureMode,
    max_dim: u32,
    ocr: &Arc<dyn TextRecognizer>,
) -> Result<CaptureOutput, CaptureError> {
    let text = if mode != CaptureMode::Image { Some(ocr.recognize(&frame)?) } else { None };
    let image = if mode != CaptureMode::Text {
        Some(rgba_to_base64_png(&downscale(&frame, max_dim)?)?)
    } else {
        None
    };
    Ok(CaptureOutput { text, image })
}

fn clamp_max_dim(max_dimension: Option<u32>) -> u32 {
    match max_dimension {
        None | Some(0) => DEFAULT_MAX_DIMENSION,
        Some(n) => n.min(DEFAULT_MAX_DIMENSION),
    }
}
```

Then rewrite `capture_region`'s body to use them (behaviour identical):

```rust
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
```

- [ ] **Step 4: Add `CaptureWindowParams` and the `capture_window` tool**

Top-level param type:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CaptureWindowParams {
    /// Target window id (from list_windows).
    pub window_id: u32,
    /// "text" (default, OCR only), "image", or "both".
    #[serde(default)]
    pub output: Option<String>,
    /// Cap the largest image side. Defaults to 1024; always enforced.
    #[serde(default)]
    pub max_dimension: Option<u32>,
}
```

Tool method inside `#[tool_router] impl Vantage`:

```rust
/// Capture a single window by id (from list_windows). Text-first like
/// capture_region. Not available on Wayland (returns an actionable error).
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
            .list_windows(WindowFilter { app_filter: None, on_screen_only: false })?
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
```

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p vantage-mcp-server --no-default-features capture_window`
Expected: PASS — both `capture_window_resolves_id_then_captures_text` and
`capture_window_unknown_id_errors`.

- [ ] **Step 6: Confirm the refactor didn't break `capture_region`**

Run: `cargo test -p vantage-mcp-server --no-default-features capture_region`
Expected: PASS — the two existing `capture_region` tests still pass.

- [ ] **Step 7: Commit**

```bash
git add crates/mcp-server/src/handler.rs
git commit -m "feat(server): capture_window tool + shared post-capture pipeline"
```

---

### Task 4: End-to-end verification, live tests, docs

**Files:**
- Create: `crates/platform/linux/tests/capture_window_live.rs` (`#[ignore]`)
- Modify: `README.md`, `docs/agent-registration.md`, `CLAUDE.md`

**Interfaces:**
- Consumes: everything from Tasks 1–3.

- [ ] **Step 1: Write the Wayland-Unsupported live test `crates/platform/linux/tests/capture_window_live.rs`**

```rust
//! Live capture_window test. On Wayland this asserts the actionable Unsupported
//! path; on X11 it should capture the first listed window.
//! Run: `cargo test -p vantage-platform-linux --test capture_window_live -- --ignored`
#![cfg(all(target_os = "linux", feature = "capture"))]

use vantage_core::{Bounds, CaptureError, ScreenCapturer, WindowInfo};
use vantage_platform_linux::LinuxScreenCapturer;

fn dummy_target() -> WindowInfo {
    WindowInfo {
        window_id: 1,
        app: "nonexistent-app".into(),
        title: "nonexistent-title".into(),
        bounds: Bounds { x: 0, y: 0, width: 10, height: 10 },
        focused: false,
    }
}

#[test]
#[ignore = "requires a desktop session"]
fn capture_window_behaviour_matches_session() {
    let cap = LinuxScreenCapturer::new();
    let is_wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|v| v.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
        || std::env::var("WAYLAND_DISPLAY").is_ok();
    let result = cap.capture_window(&dummy_target());
    if is_wayland {
        match result {
            Err(CaptureError::Unsupported(_)) => {}
            other => panic!("expected Unsupported on Wayland, got {other:?}"),
        }
    } else {
        // X11: a nonexistent window resolves to WindowNotFound, not a panic.
        match result {
            Err(CaptureError::WindowNotFound(_)) => {}
            other => panic!("expected WindowNotFound for a bogus window on X11, got {other:?}"),
        }
    }
}
```

- [ ] **Step 2: Run the live test on this box (Wayland → Unsupported path)**

Run: `cargo test -p vantage-platform-linux --test capture_window_live -- --ignored`
Expected: PASS — on the GNOME/Wayland reference box this exercises and confirms
the `Unsupported` branch.

- [ ] **Step 3: Full workspace build + test (full features) + clippy + fmt**

Run:
```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all --check
```
Expected: PASS — new `DisplayInfo` test, `list_displays`/`capture_window` handler
tests, refactored `capture_region` tests, everything green; clippy clean; fmt clean.
(If this box lacks the capture libs, use the `--no-default-features` variants and
note capture-feature tests are deferred; the libs are expected present per Spec A.)

- [ ] **Step 4: End-to-end smoke through the built binary**

Run `list_displays` and `capture_window` via stdio (initialize + initialized +
tools/call). `tools/list` must now show six tools:
`capture_region`, `capture_window`, `list_displays`, `list_windows`,
`read_clipboard`, `read_window_text`. On Wayland, `capture_window` returns the
actionable Unsupported error; `list_displays` returns the real monitors.

```bash
cargo build --release
printf '%s\n%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"cli","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_displays","arguments":{}}}' \
  | ./target/release/vantage-mcp
```
Expected: `tools/list` shows the six tools; `list_displays` returns a non-empty
`displays` array.

- [ ] **Step 5: Update docs**

- `README.md`: add `list_displays` and `capture_window` to the Features list and
  the tool table (params/defaults); note `capture_window` is macOS + Linux/X11,
  Wayland returns an actionable Unsupported error.
- `docs/agent-registration.md`: mention the two new tools in the sanity-check
  section; note the Wayland `capture_window` limitation.
- `CLAUDE.md`: update the tool count/list and the `ScreenCapturer` trait note
  (now three methods); mention `frame_to_output` is the shared capture pipeline.
- Move the "Full image-output surface" item from README Roadmap to done.

- [ ] **Step 6: Commit**

```bash
git add crates/platform/linux/tests/capture_window_live.rs README.md docs/agent-registration.md CLAUDE.md
git commit -m "test+docs: capture_window live test, e2e verification, document list_displays/capture_window"
```

---

## Self-Review

**Spec coverage:**
- §2.1 core extensions (`DisplayInfo` + 2 trait methods, all implementors) → Task 1. ✅
- §2.2 `list_displays` tool → Task 2. ✅
- §2.3 `capture_window` tool (handler resolve → capturer → shared pipeline; per-backend match; Wayland Unsupported) → Task 1 (backends) + Task 3 (tool). ✅
- §2.4 error mapping (no new variants; Unsupported/WindowNotFound) → Tasks 1, 3. ✅
- §3 testing (DisplayInfo unit, list_displays/capture_window mocks, live displays, live capture_window Wayland-Unsupported, existing tests updated) → Tasks 1–4. ✅
- §4 risks (macOS id match fallback, X11 title ambiguity, Wayland short-circuit) → Task 1 notes + Task 3 backend logic. ✅

**Placeholder scan:** No TODO/TBD. macOS `capture_window` id-match has an explicit,
bounded fallback note (Task 1 Step 4) resolved only if the macOS live run
disproves the primary path — not a hidden gap. The `--no-default-features`
fallbacks for this box are explicit, not silent.

**Type consistency:** `DisplayInfo` fields identical across core def (Task 1),
handler `ListDisplaysResult` (Task 2), and backends (Task 1). `ScreenCapturer`'s
three methods have one signature used by every impl and every mock. `capture_window`
takes `&WindowInfo` everywhere; `CaptureWindowParams`/`CaptureOutput`/`parse_mode`/
`frame_to_output`/`clamp_max_dim` names are used consistently across Tasks 2–3.
`WindowFilter { app_filter, on_screen_only }` matches the core type.
