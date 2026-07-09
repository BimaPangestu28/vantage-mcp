# vantage-mcp Gated act tools (Spec C) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add four act tools (`clipboard_write`, `type_text`, `click`, `focus_window`) behind a default-off policy gate so that, unless the operator opts in via `--allow-act`/`VANTAGE_ALLOW_ACT`, no side-effecting tool is even mounted.

**Architecture:** New `InputController` core trait + `MouseButton` type. The handler splits its tools into a read router (always mounted) and an act router (merged only when the gate is on), served via a `tool_router` field. `main` resolves the gate once at startup. Backends: arboard (`clipboard_write`), AT-SPI `grab_focus`/AX (`focus_window`), enigo (`type_text`/`click`, feature-gated).

**Tech Stack:** Rust 1.95; rmcp 2.1.0 named routers (`#[tool_router(router=…)]` + `merge`); arboard; the Spec A AT-SPI stack; enigo 0.6.

## Global Constraints

- **Default OFF, structurally isolated:** act tools live in a separate rmcp router merged into the handler ONLY when the gate is enabled. When off they must be absent from `tools/list` and uncallable. The gate is resolved once at startup from `--allow-act` OR `VANTAGE_ALLOW_ACT` (truthy: `1`/`true`/`yes`, case-insensitive) — never from per-call agent input.
- **Additive core change:** add `MouseButton` type + `InputController` trait. Do NOT change existing types/traits/`CaptureError`.
- **Every backend + mock updates together:** `backends()` becomes a 5-tuple; both platform crates provide an `InputController`; handler tests add a `NoInput` double and pass it (+ `allow_act`) to every `Vantage::new`.
- **Audit + stdout:** on enable, `main` logs a stderr `warn!`; each act call logs an `info!`. stdout stays JSON-RPC only. No `println!`.
- **No production `unwrap()`/`panic!`; `spawn_blocking` for blocking backend calls; errors via `to_mcp_error`.** Commit after each task's tests pass. Conventional commits.

---

### Task 1: Core `InputController` + gate plumbing (no act tools yet)

Establishes the trait, the 5-tuple `backends()`, stub input backends, the
read/act router split with a `tool_router` field, and the `main` gate. After this
task the server builds and behaves exactly as today (act router is empty), with
the gate wired but no act tools yet.

**Files:**
- Modify: `crates/core/src/types.rs` (`MouseButton`)
- Modify: `crates/core/src/traits.rs` (`InputController`)
- Create: `crates/platform/linux/src/input.rs` (stub `LinuxInputController`)
- Create: `crates/platform/macos/src/input.rs` (stub `MacInputController`)
- Modify: `crates/platform/linux/src/lib.rs`, `crates/platform/macos/src/lib.rs` (export + 5-tuple `backends()`)
- Modify: `crates/mcp-server/src/handler.rs` (router split + field + `input` field + `new` signature + `NoInput`)
- Modify: `crates/mcp-server/src/main.rs` (gate resolution)
- Test: inline gate-parse unit test in `main.rs`; handler tests updated

**Interfaces:**
- Produces: `vantage_core::MouseButton { Left, Right, Middle }`; `vantage_core::InputController` with `write_clipboard(&str)`, `type_text(&str)`, `click(i32,i32,MouseButton)`, `focus_window(&WindowInfo)`, all `-> Result<(), CaptureError>`. `backends()` 5-tuple `(…, Arc<dyn InputController>)`. `Vantage::new(windows, capturer, ocr, clipboard, input, allow_act: bool)`. `main::act_enabled(args, env) -> bool`.

- [ ] **Step 1: Add `MouseButton` to `crates/core/src/types.rs`**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}
```

- [ ] **Step 2: Add `InputController` to `crates/core/src/traits.rs`**

Add `MouseButton` to the `use crate::types::{…}` import, then:

```rust
pub trait InputController: Send + Sync {
    fn write_clipboard(&self, text: &str) -> Result<(), CaptureError>;
    fn type_text(&self, text: &str) -> Result<(), CaptureError>;
    fn click(&self, x: i32, y: i32, button: MouseButton) -> Result<(), CaptureError>;
    fn focus_window(&self, target: &WindowInfo) -> Result<(), CaptureError>;
}
```

Re-export is automatic (`pub use traits::*` / `types::*` in `lib.rs`). Confirm
`lib.rs` re-exports the new trait; if it lists traits explicitly, add
`InputController`.

- [ ] **Step 3: Stub `crates/platform/linux/src/input.rs`**

```rust
use vantage_core::{CaptureError, InputController, MouseButton, WindowInfo};

pub struct LinuxInputController;

impl LinuxInputController {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxInputController {
    fn default() -> Self {
        Self::new()
    }
}

impl InputController for LinuxInputController {
    fn write_clipboard(&self, _text: &str) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported("linux clipboard_write not yet implemented".into()))
    }
    fn type_text(&self, _text: &str) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported("linux type_text not yet implemented".into()))
    }
    fn click(&self, _x: i32, _y: i32, _button: MouseButton) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported("linux click not yet implemented".into()))
    }
    fn focus_window(&self, _target: &WindowInfo) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported("linux focus_window not yet implemented".into()))
    }
}
```

- [ ] **Step 4: Stub `crates/platform/macos/src/input.rs`**

Identical body with `MacInputController` and "macos …" messages.

- [ ] **Step 5: Export + extend `backends()` in both platform `lib.rs`**

Linux `lib.rs`: add (Linux-gated) `mod input; pub use input::LinuxInputController;`
and change `backends()` to the 5-tuple:

```rust
#[cfg(target_os = "linux")]
use vantage_core::{ClipboardAccess, InputController, ScreenCapturer, TextRecognizer, WindowInspector};
...
#[allow(clippy::type_complexity)]
pub fn backends() -> (
    Arc<dyn WindowInspector>,
    Arc<dyn ScreenCapturer>,
    Arc<dyn TextRecognizer>,
    Arc<dyn ClipboardAccess>,
    Arc<dyn InputController>,
) {
    (
        Arc::new(LinuxWindowInspector::new()),
        Arc::new(LinuxScreenCapturer::new()),
        Arc::new(LinuxTextRecognizer::new()),
        Arc::new(LinuxClipboard::new()),
        Arc::new(LinuxInputController::new()),
    )
}
```

macOS `lib.rs`: the mirror with `MacInputController`. (`input` module is NOT
feature-gated; the stub has no heavy deps. enigo is added in Task 4 under the
`input` feature.)

- [ ] **Step 6: Handler — split routers, add `input` + `tool_router` fields, update `new`**

In `crates/mcp-server/src/handler.rs`:

Add imports: `use rmcp::handler::server::router::tool::ToolRouter;` and
`InputController` to the `vantage_core` use list.

Change the existing `#[tool_router]` on the read-tools impl block to name the
router:

```rust
#[tool_router(router = read_tool_router)]
impl Vantage { /* existing read tools unchanged */ }
```

Add an empty act-tools impl block (act tools land in Tasks 2–4):

```rust
#[tool_router(router = act_tool_router)]
impl Vantage {
    // act tools added in later tasks
}
```

Update the struct + constructor:

```rust
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
        Self { windows, capturer, ocr, clipboard, input, tool_router }
    }
}
```

Note: `new` must live in a plain `impl Vantage` block (NOT one tagged
`#[tool_router]`, or the macro will try to treat it as a tool). Move `new` out of
the read-tools `#[tool_router]` block into its own `impl Vantage { … }`.

Point the handler at the field:

```rust
#[tool_handler(router = self.tool_router)]
impl ServerHandler for Vantage { /* get_info unchanged */ }
```

- [ ] **Step 7: Add the `NoInput` test double and fix every `Vantage::new` call site**

In `#[cfg(test)] mod tests`, add:

```rust
pub(crate) struct NoInput;
impl vantage_core::InputController for NoInput {
    fn write_clipboard(&self, _t: &str) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported("mock".into()))
    }
    fn type_text(&self, _t: &str) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported("mock".into()))
    }
    fn click(&self, _x: i32, _y: i32, _b: vantage_core::MouseButton) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported("mock".into()))
    }
    fn focus_window(&self, _t: &WindowInfo) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported("mock".into()))
    }
}
```

Update the `vantage_with_windows` helper and EVERY inline `Vantage::new(...)` in
the tests to pass `Arc::new(NoInput)` and `false` as the last two args. (There are
several — grep `Vantage::new(` and fix each.)

- [ ] **Step 8: Gate resolution in `crates/mcp-server/src/main.rs`**

```rust
/// Act tools are enabled only if the `--allow-act` flag is present OR
/// `VANTAGE_ALLOW_ACT` is truthy. Resolved once at startup.
fn act_enabled(args: impl Iterator<Item = String>, env: Option<String>) -> bool {
    let flag = args.any(|a| a == "--allow-act");
    let env = env
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    flag || env
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gate_off_by_default_on_by_flag_or_env() {
        assert!(!act_enabled(std::iter::empty(), None));
        assert!(act_enabled(["--allow-act".to_string()].into_iter(), None));
        assert!(act_enabled(std::iter::empty(), Some("1".into())));
        assert!(act_enabled(std::iter::empty(), Some("TRUE".into())));
        assert!(!act_enabled(std::iter::empty(), Some("0".into())));
    }
}
```

Wire it into `main`:

```rust
let allow_act = act_enabled(std::env::args(), std::env::var("VANTAGE_ALLOW_ACT").ok());
if allow_act {
    tracing::warn!("act tools ENABLED (clipboard_write/type_text/click/focus_window are mounted)");
}
let (windows, capturer, ocr, clipboard, input) = backend::backends();
let service = Vantage::new(windows, capturer, ocr, clipboard, input, allow_act)
    .serve(stdio())
    .await?;
```

- [ ] **Step 8b: Confirm `#[tool_handler(router = self.tool_router)]` compiles**

If rmcp 2.1.0's `tool_handler` rejects a field expression (only accepts
`Self::fn()`), fall back: keep `#[tool_handler(router = self.tool_router)]` — the
macro substitutes the expression directly into `self`-methods, so a field works.
If it does not, implement `ServerHandler::list_tools`/`call_tool` by hand,
delegating to `self.tool_router` (the router exposes `list_all` + `call`). Prefer
the attribute; only hand-roll if the build fails here.

- [ ] **Step 9: Build + test (full features; libs present)**

Run: `cargo build --workspace && cargo test --workspace`
Expected: PASS — the gate-parse unit test passes; all existing handler/boot tests
pass; `tools/list` still returns the six read tools (act router empty/unmounted).

- [ ] **Step 10: Commit**

```bash
git add crates
git commit -m "feat: InputController trait + default-off act-tool gate (router split, no act tools yet)"
```

---

### Task 2: `clipboard_write` act tool + arboard backend

**Files:**
- Modify: `crates/platform/linux/src/input.rs`, `crates/platform/macos/src/input.rs` (real `write_clipboard`)
- Modify: `crates/platform/linux/Cargo.toml`, `crates/platform/macos/Cargo.toml` (arboard already a dep on both — confirm)
- Modify: `crates/mcp-server/src/handler.rs` (act tool + `AckOutput` + gate test)
- Test: inline handler tests; live test `crates/platform/linux/tests/input_live.rs`

**Interfaces:**
- Produces: MCP act tool `clipboard_write { text: String }` → `Json<AckOutput { ok: bool }>`; `InputController::write_clipboard` implemented via arboard.

- [ ] **Step 1: Implement `write_clipboard` in `crates/platform/linux/src/input.rs`**

Replace the stub body:

```rust
    fn write_clipboard(&self, text: &str) -> Result<(), CaptureError> {
        let mut board = arboard::Clipboard::new()
            .map_err(|e| CaptureError::Internal(format!("clipboard open: {e}")))?;
        board
            .set_text(text.to_owned())
            .map_err(|e| CaptureError::Internal(format!("clipboard set_text: {e}")))
    }
```

(arboard is already a Linux dep from Spec A.) Do the mirror in the macOS
`input.rs` (arboard is already a macOS dep).

- [ ] **Step 2: Add the `AckOutput` type and the gated tool in `handler.rs`**

Top-level:

```rust
#[derive(Debug, Serialize, JsonSchema)]
pub struct AckOutput {
    pub ok: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClipboardWriteParams {
    /// Text to place on the system clipboard.
    pub text: String,
}
```

Inside the `#[tool_router(router = act_tool_router)] impl Vantage` block:

```rust
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
```

- [ ] **Step 3: Add the gate visibility test (append inside `mod tests`)**

```rust
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
    use rmcp::handler::server::router::tool::ToolRouter;
    let names = |v: &Vantage| -> Vec<String> {
        // The composed router lists exactly the mounted tools.
        Vantage::read_tool_router()
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect::<Vec<_>>()
    };
    let _ = names; // read router always present
    // Gate off: act tool not in the merged router.
    let off = vantage_gated(false);
    assert!(!off.tool_router.has_route("clipboard_write"));
    // Gate on: act tool present.
    let on = vantage_gated(true);
    assert!(on.tool_router.has_route("clipboard_write"));
}
```

Note: confirm `ToolRouter` exposes `has_route(&str) -> bool` (rmcp 2.1.0 has
`has_route`). If the accessor differs, assert via `list_all()` containing the
name. `tool_router` is a private field but the test is in the same module tree
(`#[cfg(test)] mod tests` inside `handler.rs`) so it can read it.

- [ ] **Step 4: Add a mock forwarding test**

```rust
#[tokio::test]
async fn clipboard_write_forwards_to_input() {
    use std::sync::Mutex;
    struct RecInput(Mutex<Option<String>>);
    impl vantage_core::InputController for RecInput {
        fn write_clipboard(&self, t: &str) -> Result<(), CaptureError> {
            *self.0.lock().unwrap() = Some(t.to_owned());
            Ok(())
        }
        fn type_text(&self, _t: &str) -> Result<(), CaptureError> { Ok(()) }
        fn click(&self, _x: i32, _y: i32, _b: vantage_core::MouseButton) -> Result<(), CaptureError> { Ok(()) }
        fn focus_window(&self, _t: &WindowInfo) -> Result<(), CaptureError> { Ok(()) }
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
        .clipboard_write(Parameters(ClipboardWriteParams { text: "hello".into() }))
        .await
        .unwrap();
    assert!(out.0.ok);
    assert_eq!(rec.0.lock().unwrap().as_deref(), Some("hello"));
}
```

- [ ] **Step 5: Live test `crates/platform/linux/tests/input_live.rs`**

```rust
//! Live act-tool tests. Mutates real input state (clipboard/focus).
//! Run: `cargo test -p vantage-platform-linux --test input_live -- --ignored`
#![cfg(target_os = "linux")]

use vantage_core::{ClipboardAccess, ClipboardPrefer, InputController};
use vantage_platform_linux::{LinuxClipboard, LinuxInputController};

#[test]
#[ignore = "mutates the real system clipboard"]
fn clipboard_write_then_read_roundtrips() {
    let input = LinuxInputController::new();
    input.write_clipboard("vantage-act-test").expect("write");
    let content = LinuxClipboard::new().read(ClipboardPrefer::Text).expect("read");
    assert_eq!(content.text.as_deref(), Some("vantage-act-test"));
}
```

- [ ] **Step 6: Build, test, live-check**

Run: `cargo test -p vantage-mcp-server && cargo test -p vantage-platform-linux --test input_live -- --ignored`
Expected: PASS — gate tests, forwarding test, and the live clipboard round-trip.

- [ ] **Step 7: Commit**

```bash
git add crates
git commit -m "feat(act): clipboard_write tool + arboard backend (gated)"
```

---

### Task 3: `focus_window` act tool + AT-SPI `grab_focus` (Linux) / AX (macOS)

**Files:**
- Modify: `crates/platform/linux/src/input.rs` (real `focus_window` via AT-SPI)
- Modify: `crates/platform/linux/src/windows.rs` (expose a `grab_focus_by_id` helper reusing `enumerate_frames`)
- Modify: `crates/platform/macos/src/input.rs` (AX raise)
- Modify: `crates/mcp-server/src/handler.rs` (act tool)
- Test: handler mock test; extend `input_live.rs`

**Interfaces:**
- Consumes: Spec A's `enumerate_frames`, the `Component::grab_focus` proxy.
- Produces: MCP act tool `focus_window { window_id: u32 }` → `Json<AckOutput>`; `InputController::focus_window` implemented.

- [ ] **Step 1: Add a focus helper to `crates/platform/linux/src/windows.rs`**

Expose a `pub(crate)` async helper that resolves a `window_id` to its frame and
calls `grab_focus` on its Component interface:

```rust
use atspi::proxy::proxy_ext::ProxyExt; // already imported

/// Focus the frame with the given synthesized window_id via AT-SPI.
pub(crate) async fn grab_focus_by_id(
    conn: &zbus::Connection,
    window_id: WindowId,
) -> Result<(), CaptureError> {
    let frame = enumerate_frames(conn)
        .await?
        .into_iter()
        .find(|f| f.info.window_id == window_id)
        .ok_or(CaptureError::WindowNotFound(window_id))?;
    let proxy = build_accessible(conn, &frame.bus, &frame.path).await?;
    let component = proxy
        .proxies()
        .await
        .map_err(map_internal)?
        .component()
        .await
        .map_err(map_internal)?;
    component.grab_focus().await.map_err(map_internal)?;
    Ok(())
}
```

(`build_accessible`, `enumerate_frames`, `map_internal` already exist in this
module from Spec A; make them `pub(crate)` if not already.)

- [ ] **Step 2: Implement `focus_window` in `crates/platform/linux/src/input.rs`**

`LinuxInputController` needs a tokio runtime for the async AT-SPI call, like
`LinuxWindowInspector`. Give it one:

```rust
use std::sync::Mutex;
use crate::windows::{connect, grab_focus_by_id};

pub struct LinuxInputController {
    rt: Mutex<tokio::runtime::Runtime>,
}

impl LinuxInputController {
    pub fn new() -> Self {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread runtime for AT-SPI input");
        Self { rt: Mutex::new(rt) }
    }
}
```

`focus_window`:

```rust
    fn focus_window(&self, target: &WindowInfo) -> Result<(), CaptureError> {
        let rt = self.rt.lock().expect("runtime mutex");
        rt.block_on(async {
            let conn = connect().await?;
            grab_focus_by_id(conn.connection(), target.window_id).await
        })
    }
```

Make `connect` `pub(crate)` in `windows.rs` if it is not already.

- [ ] **Step 3: Implement macOS `focus_window` in `crates/platform/macos/src/input.rs`**

Use AX to raise the window and activate its app. Skeleton (fill the AX FFI against
the `accessibility`/`objc2-app-kit` crates already used by the macOS window
backend):

```rust
    fn focus_window(&self, target: &WindowInfo) -> Result<(), CaptureError> {
        // 1. Resolve target.window_id -> owner pid via CGWindowList (as windows.rs does).
        // 2. AXUIElement for the app; find the AXWindow matching the title/position.
        // 3. AXUIElementPerformAction(window, kAXRaiseAction); activate the app via
        //    NSRunningApplication::activateWithOptions.
        // 4. Map AX permission failure -> AccessibilityPermissionDenied.
        let _ = target;
        Err(CaptureError::Internal("macos focus_window unimplemented".into()))
    }
```

Replace with the real AX calls, mirroring `windows.rs` resolution. (macOS-only;
verified by inspection + cross-compile.)

- [ ] **Step 4: Add the `focus_window` act tool in `handler.rs`**

Inside `#[tool_router(router = act_tool_router)] impl Vantage`:

```rust
/// Bring a window (from list_windows) to the foreground. (Act tool.)
#[tool(description = "Focus/raise a window by id.")]
pub async fn focus_window(
    &self,
    Parameters(params): Parameters<ReadWindowTextParams>,
) -> Result<Json<AckOutput>, ErrorData> {
    tracing::info!("act: focus_window {}", params.window_id);
    let window_id = params.window_id;
    let windows = self.windows.clone();
    let input = self.input.clone();
    tokio::task::spawn_blocking(move || {
        let target = windows
            .list_windows(WindowFilter { app_filter: None, on_screen_only: false })?
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
```

Note: `ReadWindowTextParams` has an extra optional `depth` field; define a
dedicated `FocusWindowParams { window_id: u32 }` instead to keep the schema clean:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FocusWindowParams {
    /// Target window id (from list_windows).
    pub window_id: u32,
}
```

and use `Parameters<FocusWindowParams>` above.

- [ ] **Step 5: Handler mock test — unknown id → WindowNotFound**

```rust
#[tokio::test]
async fn focus_window_unknown_id_errors() {
    let vantage = vantage_gated(true);
    let result = vantage
        .focus_window(Parameters(FocusWindowParams { window_id: 424242 }))
        .await;
    let err = match result {
        Ok(_) => panic!("expected error"),
        Err(e) => e,
    };
    assert!(err.message.contains("424242"));
}
```

- [ ] **Step 6: Extend the live test (append to `input_live.rs`)**

```rust
#[test]
#[ignore = "focuses a real window via AT-SPI"]
fn focus_first_window_does_not_error() {
    use vantage_core::{WindowFilter, WindowInspector};
    use vantage_platform_linux::LinuxWindowInspector;
    let ins = LinuxWindowInspector::new();
    let ws = ins
        .list_windows(WindowFilter { app_filter: None, on_screen_only: true })
        .unwrap();
    if let Some(w) = ws.first() {
        LinuxInputController::new().focus_window(w).expect("focus");
    }
}
```

- [ ] **Step 7: Build, test, live-check**

Run: `cargo test -p vantage-mcp-server && cargo test -p vantage-platform-linux --test input_live -- --ignored`
Expected: PASS — including the live AT-SPI focus (no error on the reference box).

- [ ] **Step 8: Commit**

```bash
git add crates
git commit -m "feat(act): focus_window via AT-SPI grab_focus (Linux) / AX (macOS)"
```

---

### Task 4: `type_text` + `click` act tools + enigo backend (feature-gated)

**Files:**
- Modify: `crates/platform/linux/Cargo.toml`, `crates/platform/macos/Cargo.toml` (enigo, feature-gated `input`)
- Modify: `crates/platform/linux/src/input.rs`, `crates/platform/macos/src/input.rs` (real `type_text`/`click`, or stubbed when `input` feature off)
- Modify: `crates/mcp-server/src/handler.rs` (two act tools)
- Test: handler mock tests

**Interfaces:**
- Produces: MCP act tools `type_text { text }` and `click { x, y, button? }` → `Json<AckOutput>`; enigo-backed `type_text`/`click`.

- [ ] **Step 1: Resolve enigo + decide feature-gating**

Run: `cargo add enigo --dry-run -p vantage-platform-linux` (inspect the pulled
system deps). If enigo requires system libraries at build time on Linux (libei/
xdo), add it as an optional dep behind a default-on `input` feature, mirroring
`capture`/`ocr`; otherwise a plain dep is fine. Record the decision in the commit.

Add to both platform `Cargo.toml` under `[target.'cfg(target_os = "…")'.dependencies]`:

```toml
enigo = "0.6"   # optional = true + `input` feature if Step 1 shows heavy system deps
```

If feature-gated, add to `[features]`: `default = [..., "input"]`,
`input = ["dep:enigo"]`, and split `input.rs` `type_text`/`click` bodies behind
`#[cfg(feature = "input")]` with an `Unsupported` stub otherwise (as
`capture_stub.rs` does).

- [ ] **Step 2: Implement `type_text` + `click` via enigo (Linux `input.rs`)**

```rust
use enigo::{Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};

    fn type_text(&self, text: &str) -> Result<(), CaptureError> {
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| classify_input_error(&e.to_string()))?;
        enigo
            .text(text)
            .map_err(|e| classify_input_error(&e.to_string()))
    }

    fn click(&self, x: i32, y: i32, button: MouseButton) -> Result<(), CaptureError> {
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| classify_input_error(&e.to_string()))?;
        enigo
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(|e| classify_input_error(&e.to_string()))?;
        let btn = match button {
            MouseButton::Left => Button::Left,
            MouseButton::Right => Button::Right,
            MouseButton::Middle => Button::Middle,
        };
        enigo
            .button(btn, Direction::Click)
            .map_err(|e| classify_input_error(&e.to_string()))
    }
```

Add a classifier that maps compositor refusal to an actionable error:

```rust
fn classify_input_error(msg: &str) -> CaptureError {
    let m = msg.to_lowercase();
    if m.contains("wayland") || m.contains("libei") || m.contains("permission") || m.contains("portal") {
        CaptureError::Unsupported(format!(
            "synthetic input was refused ({msg}). On Wayland, input injection needs a \
             RemoteDesktop portal grant and compositor support (limited on GNOME); X11 works."
        ))
    } else {
        CaptureError::Internal(format!("input: {msg}"))
    }
}
```

Confirm the enigo 0.6 API (`Enigo::new`, `text`, `move_mouse`, `button`,
`Coordinate::Abs`, `Direction::Click`, `Button`, the `Keyboard`/`Mouse` traits)
and adjust. Mirror in the macOS `input.rs`.

- [ ] **Step 3: Add the `type_text` + `click` tools in `handler.rs`**

```rust
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
```

Inside `#[tool_router(router = act_tool_router)] impl Vantage`:

```rust
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
```

- [ ] **Step 4: Handler mock tests — both forward to `InputController`**

Add a `RecInput`-style test (reuse the pattern from Task 2) asserting `type_text`
records the string and `click` records the coordinates/button, via a mock that
captures them; assert `out.0.ok`.

- [ ] **Step 5: Build + test**

Run: `cargo build --workspace && cargo test -p vantage-mcp-server`
Expected: PASS — both new tools forward correctly; gate still hides them when off.

- [ ] **Step 6: Commit**

```bash
git add crates
git commit -m "feat(act): type_text + click via enigo (gated; Wayland-limited)"
```

---

### Task 5: End-to-end gate verification + docs

**Files:**
- Modify: `README.md`, `docs/agent-registration.md`, `CLAUDE.md`
- Test: none new (e2e via the built binary)

- [ ] **Step 1: Full build + test + clippy + fmt**

Run: `cargo build --workspace && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all --check`
Expected: PASS/clean.

- [ ] **Step 2: E2E — gate OFF hides act tools**

```bash
cargo build --release
printf '%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"c","version":"0"}}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | ./target/release/vantage-mcp
```
Expected: `tools/list` shows exactly the SIX read tools — no `clipboard_write`/
`type_text`/`click`/`focus_window`.

- [ ] **Step 3: E2E — gate ON mounts act tools + clipboard_write works**

```bash
printf '%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"c","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | VANTAGE_ALLOW_ACT=1 ./target/release/vantage-mcp
# then a tools/call clipboard_write and verify via read_clipboard
```
Expected: `tools/list` shows TEN tools; `clipboard_write` returns `{ ok: true }`;
a subsequent `read_clipboard` returns the written text. `--allow-act` behaves the
same as the env var. The startup warning appears on stderr, not stdout.

- [ ] **Step 4: Update docs**

- `README.md`: new "Act tools (gated)" section — the four tools, the default-off
  gate, how to enable (`--allow-act` / `VANTAGE_ALLOW_ACT=1`), the security
  rationale (unmounted when off), and the Wayland caveat for `type_text`/`click`.
  Move the "Gated act tools" roadmap item to done.
- `docs/agent-registration.md`: how to enable act tools in the MCP config
  (`args: ["--allow-act"]` or `env: { VANTAGE_ALLOW_ACT: "1" }`) with a safety
  warning.
- `CLAUDE.md`: note the read/act router split + gate, the `InputController`
  trait, and that `Vantage::new` takes `input` + `allow_act`.

- [ ] **Step 5: Commit**

```bash
git add README.md docs/agent-registration.md CLAUDE.md
git commit -m "docs: document gated act tools (enable, security model, Wayland caveat)"
```

---

## Self-Review

**Spec coverage:**
- §2.1 gate + structural isolation (router split, field, startup resolution, audit log) → Task 1 (+ gate test) + Task 5 e2e. ✅
- §2.2 core extensions (`MouseButton`, `InputController`, 5-tuple `backends()`, `NoInput`) → Task 1. ✅
- §2.3 backends: clipboard_write/arboard → Task 2; focus_window/AT-SPI+AX → Task 3; type_text+click/enigo → Task 4. ✅
- §2.4 four act tools + `AckOutput` + resolve-by-id + spawn_blocking → Tasks 2–4. ✅
- §3 testing (gate on/off, forwarding mocks, unknown-id, gate parse, live clipboard+focus) → Tasks 1–4 + Task 5 e2e. ✅
- §4 risks (prompt-injection→unmounted, enigo Wayland, enigo deps→feature gate, macOS unverifiable) → Task 1 design + Task 4 Step 1 + notes. ✅

**Placeholder scan:** macOS `focus_window` (Task 3 Step 3) ships an
`unimplemented`-returning skeleton with numbered AX steps — a spike like the Spec A
macOS FFI, replaced within the task and validated by inspection/cross-compile
(no Mac here). enigo feature-gating (Task 4 Step 1) is an explicit conditional
resolved against the real dependency graph, not a hidden gap. `Step 8b` gives a
concrete fallback if the rmcp field-router attribute is rejected. No TODO/TBD.

**Type consistency:** `InputController`'s four methods have one signature used by
both backends, `NoInput`, and the `RecInput` mocks. `MouseButton { Left, Right,
Middle }` is identical in core, `ClickParams`, and the enigo mapping. `AckOutput {
ok }` is the shared act result. `act_tool_router`/`read_tool_router` names match
between the `#[tool_router(router=…)]` blocks and `Vantage::new`'s `merge`. The
`Vantage::new(windows, capturer, ocr, clipboard, input, allow_act)` 6-arg
signature is used identically in `main`, `vantage_gated`, and every test.
`FocusWindowParams`/`ClipboardWriteParams`/`TypeTextParams`/`ClickParams` are used
consistently between their definitions and the tool methods.
