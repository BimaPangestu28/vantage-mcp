# vantage-mcp Phase 0 + Phase 1 (macOS) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a working macOS MCP server (stdio) that lets an agent enumerate windows, read a window's accessibility text, OCR a screen region to text, and read the clipboard — text-first, with strictly-stderr logging.

**Architecture:** A Cargo workspace of three crates. `core` defines platform-agnostic capability traits, value types, and a structured error enum. `platform-macos` implements those traits over CGWindowList, AXUIElement, xcap, Vision, and arboard. `mcp-server` is the binary: it wires rmcp tool handlers (which depend only on `core` traits, via `dyn` injection) to a concrete backend chosen in `main`, and serves over stdio.

**Tech Stack:** Rust 1.95, `rmcp` 2.1.0 (server + transport-io + macros), `tokio`, `serde`/`serde_json`, `schemars` 1, `tracing`/`tracing-subscriber`, `anyhow`; macOS backends: `core-graphics` 0.25, `objc2`/`objc2-app-kit` 0.3, `accessibility` 0.2, `xcap` 0.9, `objc2-vision` 0.3 (OCR, with `ocrs` fallback), `arboard` 3, `image` 0.25, `base64` 0.22.

## Global Constraints

- **Platform:** macOS 12.3+ only this phase. Linux/Wayland and act tools are out of scope.
- **Transport:** stdio. rmcp features exactly: `server`, `transport-io`, `macros`.
- **rmcp version:** pin `rmcp = "2.1.0"`. The 2.x tool API is `#[tool_router]` + `#[tool]` + `Parameters<T>` + `Json<T>` — NOT the old 0.3.x API the PRD text references.
- **Stdout is sacred:** nothing writes to stdout except the rmcp JSON-RPC stream. All logging → stderr via `tracing_subscriber` with `.with_writer(std::io::stderr)`. No `println!`/`dbg!` anywhere in shipping code.
- **Text-first:** every capture tool defaults to `output=text`. Image output is opt-in and downscaled so its largest side ≤ `max_dimension` (server default cap **1024** enforced even when omitted).
- **Depth cap:** `read_window_text` default depth **20**, hard cap **50**; set `truncated=true` when the cap or a node budget is hit.
- **Errors are actionable:** permission-denied states map to distinct MCP errors with a fix message and NEVER collapse into a generic/internal error.
- **Handlers are platform-agnostic:** tool handlers depend only on `core` traits (`dyn Trait`), never on `platform-macos`.
- **Commit** after every task's tests pass. Conventional commit messages (`feat:`, `test:`, `chore:`).

---

### Task 1: Cargo workspace skeleton

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/core/Cargo.toml`, `crates/core/src/lib.rs`
- Create: `crates/platform/macos/Cargo.toml`, `crates/platform/macos/src/lib.rs`
- Create: `crates/mcp-server/Cargo.toml`, `crates/mcp-server/src/main.rs`
- Create: `rust-toolchain.toml`

**Interfaces:**
- Consumes: nothing.
- Produces: crate names `vantage-core`, `vantage-platform-macos`, `vantage-mcp-server` (binary `vantage-mcp`); the workspace all three later tasks build in.

- [ ] **Step 1: Write the workspace root `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = ["crates/core", "crates/platform/macos", "crates/mcp-server"]

[workspace.package]
edition = "2021"
rust-version = "1.95"
license = "MIT"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "1"
thiserror = "2"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
image = "0.25"
base64 = "0.22"
```

- [ ] **Step 2: Write `rust-toolchain.toml`**

```toml
[toolchain]
channel = "1.95"
```

- [ ] **Step 3: Write `crates/core/Cargo.toml`**

```toml
[package]
name = "vantage-core"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
serde = { workspace = true }
schemars = { workspace = true }
thiserror = { workspace = true }
```

- [ ] **Step 4: Write `crates/core/src/lib.rs` (placeholder that compiles)**

```rust
//! Platform-agnostic capability traits, value types, and errors for vantage-mcp.
```

- [ ] **Step 5: Write `crates/platform/macos/Cargo.toml`**

```toml
[package]
name = "vantage-platform-macos"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
vantage-core = { path = "../../core" }
```

- [ ] **Step 6: Write `crates/platform/macos/src/lib.rs` (placeholder)**

```rust
//! macOS backend implementations of vantage-core capability traits.
```

- [ ] **Step 7: Write `crates/mcp-server/Cargo.toml`**

```toml
[package]
name = "vantage-mcp-server"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "vantage-mcp"
path = "src/main.rs"

[dependencies]
vantage-core = { path = "../core" }
vantage-platform-macos = { path = "../platform/macos" }
rmcp = { version = "2.1.0", features = ["server", "transport-io", "macros"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "io-std", "signal"] }
serde = { workspace = true }
serde_json = { workspace = true }
schemars = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
anyhow = { workspace = true }
image = { workspace = true }
base64 = { workspace = true }
```

- [ ] **Step 8: Write `crates/mcp-server/src/main.rs` (placeholder)**

```rust
fn main() {
    eprintln!("vantage-mcp skeleton");
}
```

- [ ] **Step 9: Build the whole workspace**

Run: `cargo build --workspace`
Expected: PASS — all three crates compile. (First build downloads rmcp + deps; may take a few minutes.)

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml Cargo.lock rust-toolchain.toml crates
git commit -m "chore: scaffold vantage-mcp cargo workspace (core, platform-macos, mcp-server)"
```

---

### Task 2: Core types, error enum, capability traits

**Files:**
- Create: `crates/core/src/types.rs`
- Create: `crates/core/src/error.rs`
- Create: `crates/core/src/traits.rs`
- Modify: `crates/core/src/lib.rs`
- Test: inline `#[cfg(test)]` in `crates/core/src/error.rs`

**Interfaces:**
- Consumes: nothing.
- Produces (relied on by every later task — exact names):
  - Types: `WindowId = u32`, `Bounds { x: i32, y: i32, width: u32, height: u32 }`, `WindowInfo { window_id, app, title, bounds, focused }`, `WindowFilter { app_filter: Option<String>, on_screen_only: bool }`, `WindowText { text: String, truncated: bool }`, `RgbaImage { width: u32, height: u32, pixels: Vec<u8> }` (RGBA8, row-major), `ClipboardPrefer { Text, Image }`, `ClipboardKind { Text, Image, Empty }`, `ClipboardContent { kind, text: Option<String>, image: Option<RgbaImage> }`.
  - Error: `CaptureError` enum with variants `ScreenRecordingPermissionDenied`, `AccessibilityPermissionDenied`, `WindowNotFound(WindowId)`, `InvalidBounds(Bounds)`, `Unsupported(String)`, `Internal(String)`; method `CaptureError::kind(&self) -> ErrorKind` where `ErrorKind { ScreenRecordingPermission, AccessibilityPermission, NotFound, InvalidInput, Unsupported, Internal }`.
  - Traits: `WindowInspector { list_windows(&self, WindowFilter) -> Result<Vec<WindowInfo>, CaptureError>; read_window_text(&self, WindowId, depth: u32) -> Result<WindowText, CaptureError> }`, `ScreenCapturer { capture_region(&self, Bounds) -> Result<RgbaImage, CaptureError> }`, `TextRecognizer { recognize(&self, &RgbaImage) -> Result<String, CaptureError> }`, `ClipboardAccess { read(&self, ClipboardPrefer) -> Result<ClipboardContent, CaptureError> }`. All `: Send + Sync`.

- [ ] **Step 1: Write the failing test in `crates/core/src/error.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    ScreenRecordingPermission,
    AccessibilityPermission,
    NotFound,
    InvalidInput,
    Unsupported,
    Internal,
}

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("Screen Recording permission not granted to this process")]
    ScreenRecordingPermissionDenied,
    #[error("Accessibility permission not granted to this process")]
    AccessibilityPermissionDenied,
    #[error("window {0} not found")]
    WindowNotFound(crate::types::WindowId),
    #[error("region {0:?} is outside all display bounds")]
    InvalidBounds(crate::types::Bounds),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl CaptureError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            CaptureError::ScreenRecordingPermissionDenied => ErrorKind::ScreenRecordingPermission,
            CaptureError::AccessibilityPermissionDenied => ErrorKind::AccessibilityPermission,
            CaptureError::WindowNotFound(_) => ErrorKind::NotFound,
            CaptureError::InvalidBounds(_) => ErrorKind::InvalidInput,
            CaptureError::Unsupported(_) => ErrorKind::Unsupported,
            CaptureError::Internal(_) => ErrorKind::Internal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Bounds;

    #[test]
    fn every_variant_has_a_distinct_kind() {
        let bounds = Bounds { x: 0, y: 0, width: 1, height: 1 };
        let cases = [
            (CaptureError::ScreenRecordingPermissionDenied, ErrorKind::ScreenRecordingPermission),
            (CaptureError::AccessibilityPermissionDenied, ErrorKind::AccessibilityPermission),
            (CaptureError::WindowNotFound(7), ErrorKind::NotFound),
            (CaptureError::InvalidBounds(bounds), ErrorKind::InvalidInput),
            (CaptureError::Unsupported("x".into()), ErrorKind::Unsupported),
            (CaptureError::Internal("x".into()), ErrorKind::Internal),
        ];
        for (err, expected) in cases {
            assert_eq!(err.kind(), expected);
        }
    }
}
```

- [ ] **Step 2: Write `crates/core/src/types.rs`**

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub type WindowId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WindowInfo {
    pub window_id: WindowId,
    pub app: String,
    pub title: String,
    pub bounds: Bounds,
    pub focused: bool,
}

#[derive(Debug, Clone)]
pub struct WindowFilter {
    pub app_filter: Option<String>,
    pub on_screen_only: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WindowText {
    pub text: String,
    pub truncated: bool,
}

/// RGBA8, row-major, `pixels.len() == width * height * 4`.
#[derive(Debug, Clone)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardPrefer {
    Text,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ClipboardKind {
    Text,
    Image,
    Empty,
}

#[derive(Debug, Clone)]
pub struct ClipboardContent {
    pub kind: ClipboardKind,
    pub text: Option<String>,
    pub image: Option<RgbaImage>,
}
```

- [ ] **Step 3: Write `crates/core/src/traits.rs`**

```rust
use crate::error::CaptureError;
use crate::types::{
    Bounds, ClipboardContent, ClipboardPrefer, RgbaImage, WindowFilter, WindowId, WindowInfo,
    WindowText,
};

pub trait WindowInspector: Send + Sync {
    fn list_windows(&self, filter: WindowFilter) -> Result<Vec<WindowInfo>, CaptureError>;
    fn read_window_text(&self, window_id: WindowId, depth: u32) -> Result<WindowText, CaptureError>;
}

pub trait ScreenCapturer: Send + Sync {
    fn capture_region(&self, bounds: Bounds) -> Result<RgbaImage, CaptureError>;
}

pub trait TextRecognizer: Send + Sync {
    fn recognize(&self, image: &RgbaImage) -> Result<String, CaptureError>;
}

pub trait ClipboardAccess: Send + Sync {
    fn read(&self, prefer: ClipboardPrefer) -> Result<ClipboardContent, CaptureError>;
}
```

- [ ] **Step 4: Rewrite `crates/core/src/lib.rs`**

```rust
//! Platform-agnostic capability traits, value types, and errors for vantage-mcp.
pub mod error;
pub mod traits;
pub mod types;

pub use error::{CaptureError, ErrorKind};
pub use traits::{ClipboardAccess, ScreenCapturer, TextRecognizer, WindowInspector};
pub use types::*;
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p vantage-core`
Expected: PASS — `every_variant_has_a_distinct_kind`.

- [ ] **Step 6: Commit**

```bash
git add crates/core
git commit -m "feat(core): capability traits, value types, and structured error enum"
```

---

### Task 3: mcp-server foundation — stdio boot, stderr logging, stdout guard, error mapping

**Files:**
- Create: `crates/mcp-server/src/error_map.rs`
- Create: `crates/mcp-server/src/logging.rs`
- Rewrite: `crates/mcp-server/src/main.rs`
- Create: `crates/mcp-server/src/handler.rs`
- Test: inline `#[cfg(test)]` in `crates/mcp-server/src/error_map.rs`; integration test `crates/mcp-server/tests/boot.rs`

**Interfaces:**
- Consumes: `vantage_core::{CaptureError, ErrorKind}`.
- Produces: `error_map::to_mcp_error(CaptureError) -> rmcp::ErrorData`; `logging::init()` (stderr subscriber); `handler::Vantage` struct holding `Arc<dyn ...>` backends with a `Vantage::new(...)` constructor and an empty `#[tool_router]`/`#[tool_handler]` so the server serves an (initially empty) tool set.

- [ ] **Step 1: Write the failing unit test in `crates/mcp-server/src/error_map.rs`**

```rust
use rmcp::ErrorData;
use vantage_core::CaptureError;

/// Map a domain error to an MCP error. Permission-denied variants use
/// `invalid_request` with an actionable fix message and never collapse into
/// `internal_error`.
pub fn to_mcp_error(err: CaptureError) -> ErrorData {
    match err {
        CaptureError::ScreenRecordingPermissionDenied => ErrorData::invalid_request(
            "Screen Recording permission not granted to this process. Grant it in System \
             Settings > Privacy & Security > Screen Recording, then restart the agent.",
            None,
        ),
        CaptureError::AccessibilityPermissionDenied => ErrorData::invalid_request(
            "Accessibility permission not granted to this process. Grant it in System \
             Settings > Privacy & Security > Accessibility, then restart the agent.",
            None,
        ),
        CaptureError::WindowNotFound(id) => {
            ErrorData::invalid_params(format!("window {id} not found"), None)
        }
        CaptureError::InvalidBounds(b) => ErrorData::invalid_params(
            format!("region {b:?} is outside all display bounds"),
            None,
        ),
        CaptureError::Unsupported(msg) => {
            ErrorData::invalid_request(format!("unsupported on this platform: {msg}"), None)
        }
        CaptureError::Internal(msg) => ErrorData::internal_error(msg, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::ErrorCode;

    #[test]
    fn permission_denied_is_not_internal_and_is_actionable() {
        let mapped = to_mcp_error(CaptureError::ScreenRecordingPermissionDenied);
        assert_eq!(mapped.code, ErrorCode::INVALID_REQUEST);
        assert_ne!(mapped.code, ErrorCode::INTERNAL_ERROR);
        assert!(mapped.message.contains("Screen Recording"));
        assert!(mapped.message.to_lowercase().contains("grant"));
    }

    #[test]
    fn not_found_maps_to_invalid_params() {
        let mapped = to_mcp_error(CaptureError::WindowNotFound(42));
        assert_eq!(mapped.code, ErrorCode::INVALID_PARAMS);
        assert!(mapped.message.contains("42"));
    }
}
```

Note: confirm the `ErrorCode` constant path against rmcp 2.1.0 (`rmcp::model::ErrorCode::INVALID_REQUEST` / `INVALID_PARAMS` / `INTERNAL_ERROR`). If the constants differ, assert on the numeric `.code` values instead (-32600 invalid_request, -32602 invalid_params, -32603 internal_error).

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p vantage-mcp-server to_mcp_error 2>&1 | tail -20`
Expected: FAIL — module `error_map` not yet wired into the crate (unresolved import) or assertion not yet reachable.

- [ ] **Step 3: Write `crates/mcp-server/src/logging.rs`**

```rust
use tracing_subscriber::{fmt, EnvFilter};

/// Initialize tracing to **stderr only**. stdout is reserved for the JSON-RPC
/// stream; nothing else may write to it.
pub fn init() {
    let filter = EnvFilter::try_from_env("VANTAGE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .with_ansi(false)
        .init();
}
```

- [ ] **Step 4: Write `crates/mcp-server/src/handler.rs`**

```rust
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::{tool_handler, tool_router, ServerHandler};
use rmcp::model::{ServerCapabilities, ServerInfo};

use vantage_core::{ClipboardAccess, ScreenCapturer, TextRecognizer, WindowInspector};

/// The MCP server handler. Holds injected, platform-agnostic backends.
/// Tool methods are added in later tasks; this establishes the wiring.
#[derive(Clone)]
pub struct Vantage {
    pub(crate) windows: Arc<dyn WindowInspector>,
    pub(crate) capturer: Arc<dyn ScreenCapturer>,
    pub(crate) ocr: Arc<dyn TextRecognizer>,
    pub(crate) clipboard: Arc<dyn ClipboardAccess>,
    tool_router: ToolRouter<Self>,
}

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
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_handler]
impl ServerHandler for Vantage {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Desktop capture for macOS. Prefer read_window_text over screenshots; \
                 capture_region defaults to OCR text (no image) to keep token cost low."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
```

Note: confirm `ToolRouter` import path (`rmcp::handler::server::router::tool::ToolRouter`) and that `#[tool_router]` generates the associated `Self::tool_router()` used above. If the generated router does not need a struct field in 2.1.0, drop the field and the `tool_router:` initializer (the `#[tool_handler]` macro doc says it calls `Self::tool_router()` automatically). Adjust to whichever the crate compiles against — the behavior (an empty-but-valid tool router) is the deliverable.

- [ ] **Step 5: Rewrite `crates/mcp-server/src/main.rs`**

```rust
mod error_map;
mod handler;
mod logging;

use std::sync::Arc;

use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};

use handler::Vantage;
use vantage_platform_macos as backend;

#[tokio::main]
async fn main() -> Result<()> {
    logging::init();
    tracing::info!("vantage-mcp starting (stdio); logging on stderr only");

    // Concrete macOS backends are constructed here and injected as trait objects.
    let windows = Arc::new(backend::MacWindowInspector::new());
    let capturer = Arc::new(backend::MacScreenCapturer::new());
    let ocr = Arc::new(backend::MacTextRecognizer::new());
    let clipboard = Arc::new(backend::MacClipboard::new());

    let service = Vantage::new(windows, capturer, ocr, clipboard)
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
```

Note: `backend::Mac*` constructors land in Tasks 8–11. Until then, either (a) temporarily construct in-crate test doubles, or (b) implement Tasks 8–11 before first running `main`. The unit/integration tests below do not require the macOS backends.

- [ ] **Step 6: Write the boot integration test `crates/mcp-server/tests/boot.rs`**

```rust
//! Boots the built binary over stdio, performs the MCP `initialize` handshake
//! by writing one JSON-RPC line to stdin, and asserts:
//!   1. a JSON-RPC response comes back on stdout, and
//!   2. stdout contains ONLY JSON-RPC (no stray log/print lines).
//! Logging must be on stderr.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn initialize_handshake_and_clean_stdout() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_vantage-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn vantage-mcp");

    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "boot-test", "version": "0.0.0" }
        }
    });

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "{init}").expect("write initialize");
        stdin.flush().expect("flush");
    }

    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read response line");

    let parsed: serde_json::Value =
        serde_json::from_str(line.trim()).expect("stdout line 1 must be valid JSON-RPC");
    assert_eq!(parsed["jsonrpc"], "2.0", "first stdout line must be JSON-RPC");
    assert_eq!(parsed["id"], 1);
    assert!(parsed.get("result").is_some(), "initialize must return a result");

    child.kill().ok();
    child.wait().ok();
}
```

- [ ] **Step 7: Run the tests**

Run: `cargo test -p vantage-mcp-server`
Expected: unit tests in `error_map` PASS. The `boot` integration test PASS **once the macOS backends (Tasks 8–11) exist** so `main` compiles and runs. If executing Task 3 before Tasks 8–11, temporarily stub the four `Arc::new(backend::Mac*::new())` lines with local no-op doubles to make `main` build, and mark the boot test `#[ignore]` with a `// un-ignore after Task 12` note. Re-enable in Task 12.

- [ ] **Step 8: Commit**

```bash
git add crates/mcp-server
git commit -m "feat(server): stdio boot, stderr-only logging, error mapping, empty tool router"
```

---

### Task 4: `list_windows` tool handler

**Files:**
- Modify: `crates/mcp-server/src/handler.rs` (add tool method + param/output types)
- Test: inline `#[cfg(test)]` in `crates/mcp-server/src/handler.rs` (with mock backends)

**Interfaces:**
- Consumes: `vantage_core::{WindowInspector, WindowFilter, WindowInfo}`; `error_map::to_mcp_error`.
- Produces: MCP tool `list_windows` with params `ListWindowsParams { app_filter: Option<String>, on_screen_only: Option<bool> }` (default `on_screen_only=true`) returning `Json<Vec<WindowInfo>>`. Establishes the `#[cfg(test)] mod tests` mock-backend harness reused by Tasks 5–7.

- [ ] **Step 1: Write the failing test (append to `handler.rs`)**

```rust
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

        assert_eq!(out.0.len(), 1);
        assert_eq!(out.0[0].app, "Notes");
        assert_eq!(*mock.last_filter_on_screen_only.lock().unwrap(), Some(true));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p vantage-mcp-server list_windows_defaults 2>&1 | tail -20`
Expected: FAIL — `list_windows` method and `ListWindowsParams` do not exist.

- [ ] **Step 3: Add the tool to the `#[tool_router] impl Vantage` block**

Add these imports near the top of `handler.rs`:

```rust
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, ErrorData, Json};
use schemars::JsonSchema;
use serde::Deserialize;
use vantage_core::{WindowFilter, WindowInfo};

use crate::error_map::to_mcp_error;
```

Add the param type (top-level in the file):

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListWindowsParams {
    /// Only return windows whose owning application name equals this.
    #[serde(default)]
    pub app_filter: Option<String>,
    /// Restrict to on-screen windows. Defaults to true.
    #[serde(default)]
    pub on_screen_only: Option<bool>,
}
```

Add the method inside `#[tool_router] impl Vantage { ... }`:

```rust
/// List on-screen windows: window_id, owning app, title, bounds, and focus.
/// Primary entry point for an agent to orient before reading a window.
#[tool(description = "List on-screen windows (id, app, title, bounds, focused).")]
pub async fn list_windows(
    &self,
    Parameters(params): Parameters<ListWindowsParams>,
) -> Result<Json<Vec<WindowInfo>>, ErrorData> {
    let filter = WindowFilter {
        app_filter: params.app_filter,
        on_screen_only: params.on_screen_only.unwrap_or(true),
    };
    let windows = self.windows.clone();
    let result = tokio::task::spawn_blocking(move || windows.list_windows(filter))
        .await
        .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?;
    result.map(Json).map_err(to_mcp_error)
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p vantage-mcp-server list_windows_defaults`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mcp-server/src/handler.rs
git commit -m "feat(server): list_windows tool + mock-backend test harness"
```

---

### Task 5: `read_window_text` tool handler (depth default + cap + truncation)

**Files:**
- Modify: `crates/mcp-server/src/handler.rs`
- Test: inline `#[cfg(test)]` in `handler.rs`

**Interfaces:**
- Consumes: `WindowInspector::read_window_text`, mock harness from Task 4.
- Produces: MCP tool `read_window_text` with params `ReadWindowTextParams { window_id: u32, depth: Option<u32> }`, returning `Json<WindowText>`. Applies default depth **20**, hard cap **50** before calling the backend. Constants `DEFAULT_DEPTH: u32 = 20`, `MAX_DEPTH: u32 = 50`.

- [ ] **Step 1: Write the failing test (append inside `mod tests`)**

```rust
#[tokio::test]
async fn read_window_text_applies_default_and_caps_depth() {
    use std::sync::Mutex;

    struct DepthSpy { seen: Mutex<Vec<u32>> }
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
    vantage.read_window_text(Parameters(ReadWindowTextParams { window_id: 1, depth: None }))
        .await.unwrap();
    // caps when too large
    vantage.read_window_text(Parameters(ReadWindowTextParams { window_id: 1, depth: Some(999) }))
        .await.unwrap();

    assert_eq!(*spy.seen.lock().unwrap(), vec![DEFAULT_DEPTH, MAX_DEPTH]);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p vantage-mcp-server read_window_text_applies 2>&1 | tail -20`
Expected: FAIL — method/params/constants missing.

- [ ] **Step 3: Add constants, param type, and the tool method**

Top-level in `handler.rs`:

```rust
pub const DEFAULT_DEPTH: u32 = 20;
pub const MAX_DEPTH: u32 = 50;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadWindowTextParams {
    /// Target window id (from list_windows).
    pub window_id: u32,
    /// Accessibility-tree depth to walk. Defaults to 20, capped at 50.
    #[serde(default)]
    pub depth: Option<u32>,
}
```

Also add `use vantage_core::WindowText;` to the imports. Inside `#[tool_router] impl Vantage`:

```rust
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
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p vantage-mcp-server read_window_text_applies`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mcp-server/src/handler.rs
git commit -m "feat(server): read_window_text tool with depth default and cap"
```

---

### Task 6: `read_clipboard` tool handler

**Files:**
- Modify: `crates/mcp-server/src/handler.rs`
- Test: inline `#[cfg(test)]` in `handler.rs`

**Interfaces:**
- Consumes: `ClipboardAccess::read`, `ClipboardPrefer`, `ClipboardKind`, `ClipboardContent`.
- Produces: MCP tool `read_clipboard` with params `ReadClipboardParams { prefer: Option<String> }` (`"text"` default, `"image"` accepted; any other value → `invalid_params`), returning `Json<ClipboardOutput>` where `ClipboardOutput { kind: ClipboardKind, text: Option<String>, image: Option<String> /* base64 PNG */ }`.

- [ ] **Step 1: Write the failing test (append inside `mod tests`)**

```rust
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
    let vantage = Vantage::new(
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
    let vantage = Vantage::new(
        Arc::new(MockWindows::default()),
        Arc::new(NoScreen),
        Arc::new(NoOcr),
        Arc::new(NoClip),
    );
    let err = vantage
        .read_clipboard(Parameters(ReadClipboardParams { prefer: Some("video".into()) }))
        .await
        .unwrap_err();
    assert!(err.message.to_lowercase().contains("prefer"));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p vantage-mcp-server read_clipboard 2>&1 | tail -20`
Expected: FAIL — method/params missing.

- [ ] **Step 3: Add the param + output types and the tool method**

Top-level in `handler.rs` (add `use vantage_core::{ClipboardKind, ClipboardPrefer};` and `use crate::image_out::rgba_to_base64_png;` — the latter is created in Task 7; if executing Task 6 first, inline a `todo!()`-free minimal PNG encoder or reorder so Task 7 precedes image encoding. Text-only clipboard needs no encoder.):

```rust
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
```

Add `use serde::Serialize;` to imports. Inside `#[tool_router] impl Vantage`:

```rust
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
    Ok(Json(ClipboardOutput { kind: content.kind, text: content.text, image }))
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p vantage-mcp-server read_clipboard`
Expected: PASS (both). (If `image_out` is not yet present, execute Task 7 first; the two tests here exercise only the text/reject paths and do not hit the encoder.)

- [ ] **Step 5: Commit**

```bash
git add crates/mcp-server/src/handler.rs
git commit -m "feat(server): read_clipboard tool (text default, base64 PNG image)"
```

---

### Task 7: `capture_region` tool + image encoding/downscaling util

**Files:**
- Create: `crates/mcp-server/src/image_out.rs`
- Modify: `crates/mcp-server/src/handler.rs`, `crates/mcp-server/src/main.rs` (add `mod image_out;`)
- Test: inline `#[cfg(test)]` in `image_out.rs` and in `handler.rs`

**Interfaces:**
- Consumes: `ScreenCapturer::capture_region`, `TextRecognizer::recognize`, `RgbaImage`, `Bounds`.
- Produces:
  - `image_out::downscale(&RgbaImage, max_dim: u32) -> RgbaImage` (no-op when already within bound; preserves aspect ratio; largest side becomes ≤ max_dim).
  - `image_out::rgba_to_base64_png(&RgbaImage) -> Result<String, CaptureError>`.
  - MCP tool `capture_region` with params `CaptureRegionParams { bounds: Bounds, output: Option<String>, max_dimension: Option<u32> }`; `output` ∈ {`"text"` (default), `"image"`, `"both"`}; returns `Json<CaptureOutput { text: Option<String>, image: Option<String> }>`. Default cap `DEFAULT_MAX_DIMENSION: u32 = 1024`.

- [ ] **Step 1: Write the failing tests in `crates/mcp-server/src/image_out.rs`**

```rust
use base64::Engine;
use image::{ImageBuffer, Rgba};
use vantage_core::{CaptureError, RgbaImage};

pub const DEFAULT_MAX_DIMENSION: u32 = 1024;

/// Downscale so the largest side is <= `max_dim`, preserving aspect ratio.
/// Returns the input unchanged when it already fits.
pub fn downscale(input: &RgbaImage, max_dim: u32) -> RgbaImage {
    let longest = input.width.max(input.height);
    if max_dim == 0 || longest <= max_dim {
        return input.clone();
    }
    let scale = max_dim as f32 / longest as f32;
    let new_w = (input.width as f32 * scale).round().max(1.0) as u32;
    let new_h = (input.height as f32 * scale).round().max(1.0) as u32;
    let buf: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(input.width, input.height, input.pixels.clone())
            .expect("valid rgba buffer");
    let resized = image::imageops::resize(&buf, new_w, new_h, image::imageops::FilterType::Triangle);
    RgbaImage { width: new_w, height: new_h, pixels: resized.into_raw() }
}

/// Encode an RGBA image as a base64 PNG string.
pub fn rgba_to_base64_png(input: &RgbaImage) -> Result<String, CaptureError> {
    let buf: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(input.width, input.height, input.pixels.clone())
            .ok_or_else(|| CaptureError::Internal("invalid rgba buffer".into()))?;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(buf.as_raw(), input.width, input.height, image::ExtendedColorType::Rgba8)
        .map_err(|e| CaptureError::Internal(format!("png encode: {e}")))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(png))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32) -> RgbaImage {
        RgbaImage { width: w, height: h, pixels: vec![255u8; (w * h * 4) as usize] }
    }

    #[test]
    fn downscale_is_noop_when_within_bound() {
        let img = solid(800, 600);
        let out = downscale(&img, 1024);
        assert_eq!((out.width, out.height), (800, 600));
    }

    #[test]
    fn downscale_caps_longest_side_and_keeps_aspect() {
        let img = solid(2000, 1000);
        let out = downscale(&img, 1000);
        assert_eq!(out.width, 1000);
        assert_eq!(out.height, 500);
        assert_eq!(out.pixels.len() as u32, out.width * out.height * 4);
    }

    #[test]
    fn png_roundtrips_dimensions() {
        let b64 = rgba_to_base64_png(&solid(4, 4)).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (4, 4));
    }
}
```

Add `use image::ImageEncoder;` at the top of `image_out.rs` (needed for `write_image`). Confirm `ExtendedColorType::Rgba8` path against `image` 0.25.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p vantage-mcp-server -- image_out 2>&1 | tail -20`
Expected: FAIL — `mod image_out` not declared yet.

- [ ] **Step 3: Declare the module and add the tool**

In `main.rs` add `mod image_out;` alongside the other `mod` lines.

In `handler.rs`, add imports:

```rust
use vantage_core::Bounds;
use crate::image_out::{downscale, rgba_to_base64_png, DEFAULT_MAX_DIMENSION};
```

Add param/output types top-level:

```rust
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

#[derive(Debug, Serialize, JsonSchema)]
pub struct CaptureOutput {
    pub text: Option<String>,
    /// base64-encoded PNG, present only when output includes an image.
    pub image: Option<String>,
}
```

Inside `#[tool_router] impl Vantage`:

```rust
/// Capture a screen region. Defaults to output=text: runs OCR and returns
/// text only, no pixels (keeps token cost low). Use output=image/both when
/// visual layout matters; images are downscaled to max_dimension.
#[tool(description = "Capture a screen region; defaults to OCR text (no image).")]
pub async fn capture_region(
    &self,
    Parameters(params): Parameters<CaptureRegionParams>,
) -> Result<Json<CaptureOutput>, ErrorData> {
    #[derive(PartialEq)]
    enum Mode { Text, Image, Both }
    let mode = match params.output.as_deref() {
        None | Some("text") => Mode::Text,
        Some("image") => Mode::Image,
        Some("both") => Mode::Both,
        Some(other) => {
            return Err(ErrorData::invalid_params(
                format!("output must be \"text\", \"image\", or \"both\", got {other:?}"),
                None,
            ))
        }
    };
    let max_dim = params.max_dimension.unwrap_or(DEFAULT_MAX_DIMENSION).min(DEFAULT_MAX_DIMENSION);
    let bounds = params.bounds;
    let capturer = self.capturer.clone();
    let ocr = self.ocr.clone();

    let (text, image) = tokio::task::spawn_blocking(move || {
        let frame = capturer.capture_region(bounds)?;
        let text = if mode != Mode::Image {
            Some(ocr.recognize(&frame)?)
        } else {
            None
        };
        let image = if mode != Mode::Text {
            Some(rgba_to_base64_png(&downscale(&frame, max_dim))?)
        } else {
            None
        };
        Ok::<_, vantage_core::CaptureError>((text, image))
    })
    .await
    .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(to_mcp_error)?;

    Ok(Json(CaptureOutput { text, image }))
}
```

- [ ] **Step 4: Write the handler test (append inside `mod tests` in `handler.rs`)**

```rust
#[tokio::test]
async fn capture_region_text_mode_runs_ocr_and_returns_no_image() {
    struct FakeScreen;
    impl ScreenCapturer for FakeScreen {
        fn capture_region(&self, _b: Bounds) -> Result<RgbaImage, CaptureError> {
            Ok(RgbaImage { width: 2, height: 2, pixels: vec![0u8; 16] })
        }
    }
    struct FakeOcr;
    impl TextRecognizer for FakeOcr {
        fn recognize(&self, _i: &RgbaImage) -> Result<String, CaptureError> {
            Ok("hello".into())
        }
    }
    let vantage = Vantage::new(
        Arc::new(MockWindows::default()),
        Arc::new(FakeScreen),
        Arc::new(FakeOcr),
        Arc::new(NoClip),
    );
    let out = vantage
        .capture_region(Parameters(CaptureRegionParams {
            bounds: Bounds { x: 0, y: 0, width: 2, height: 2 },
            output: None,
            max_dimension: None,
        }))
        .await
        .unwrap();
    assert_eq!(out.0.text.as_deref(), Some("hello"));
    assert!(out.0.image.is_none(), "text mode must not return pixels");
}
```

- [ ] **Step 5: Run all server tests to verify they pass**

Run: `cargo test -p vantage-mcp-server`
Expected: PASS — `image_out` unit tests + `capture_region_text_mode...` + all prior handler tests.

- [ ] **Step 6: Commit**

```bash
git add crates/mcp-server/src
git commit -m "feat(server): capture_region tool with text-first output, downscaling, PNG encode"
```

---

### Task 8: macOS `WindowInspector` (CGWindowList + AXUIElement)

**Files:**
- Create: `crates/platform/macos/src/windows.rs`
- Modify: `crates/platform/macos/src/lib.rs`, `crates/platform/macos/Cargo.toml`
- Test: `crates/platform/macos/tests/windows_live.rs` (marked `#[ignore]`)

**Interfaces:**
- Consumes: `vantage_core::{WindowInspector, WindowFilter, WindowInfo, WindowText, WindowId, Bounds, CaptureError}`.
- Produces: `MacWindowInspector` with `MacWindowInspector::new() -> Self`, implementing `WindowInspector`. Used by `main.rs` (Task 3).

**Risk note:** This is a spike task. `list_windows` (CGWindowList) is stable Quartz; `read_window_text` (AX tree walk) is the harder half. Distinguish permission-denied (`AXError` `kAXErrorAPIDisabled`/`kAXErrorNotAuthorized` → `AccessibilityPermissionDenied`) from a genuinely missing window (`WindowNotFound`). Keep the AX walk bounded by `depth` and a node budget; set `truncated=true` when either bound stops the walk.

- [ ] **Step 1: Add macOS dependencies to `crates/platform/macos/Cargo.toml`**

```toml
[target.'cfg(target_os = "macos")'.dependencies]
core-graphics = "0.25"
core-foundation = "0.10"
objc2 = "0.6"
objc2-app-kit = { version = "0.3", features = ["NSWorkspace", "NSRunningApplication"] }
accessibility = "0.2"
accessibility-sys = "0.1"
```

Note: confirm the `objc2`/`objc2-app-kit` minor versions resolve together against `objc2-vision` added in Task 10; align all `objc2-*` crates to the same `objc2` major (0.6.x). Adjust versions if `cargo build` reports a mismatch.

- [ ] **Step 2: Write the `#[ignore]` integration test `crates/platform/macos/tests/windows_live.rs`**

```rust
//! Live tests: require a logged-in macOS session with at least one on-screen
//! window, plus Screen Recording (for titles) and Accessibility permissions.
//! Run manually: `cargo test -p vantage-platform-macos --test windows_live -- --ignored`

use vantage_core::{WindowFilter, WindowInspector};
use vantage_platform_macos::MacWindowInspector;

#[test]
#[ignore = "requires live macOS session + permissions"]
fn lists_at_least_one_window() {
    let inspector = MacWindowInspector::new();
    let windows = inspector
        .list_windows(WindowFilter { app_filter: None, on_screen_only: true })
        .expect("list_windows");
    assert!(!windows.is_empty(), "expected at least one on-screen window");
    assert!(windows.iter().any(|w| !w.app.is_empty()));
}

#[test]
#[ignore = "requires live macOS session + Accessibility permission"]
fn reads_some_text_from_first_window() {
    let inspector = MacWindowInspector::new();
    let windows = inspector
        .list_windows(WindowFilter { app_filter: None, on_screen_only: true })
        .unwrap();
    let target = windows.first().expect("a window");
    let text = inspector.read_window_text(target.window_id, 20).expect("read_window_text");
    // Content varies; assert the call path works and returns the struct.
    let _ = text.truncated;
}
```

- [ ] **Step 3: Implement `crates/platform/macos/src/windows.rs`**

Implement `list_windows` via `CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)`, reading dictionary keys `kCGWindowNumber` (→ `window_id`), `kCGWindowOwnerName` (→ `app`), `kCGWindowName` (→ `title`), `kCGWindowBounds` (→ `Bounds` via `CGRectMakeWithDictionaryRepresentation`), `kCGWindowOwnerPID`, and `kCGWindowLayer` (filter out non-zero layers / menubar/dock chrome). Derive `focused` by comparing the owner pid to `NSWorkspace::sharedWorkspace().frontmostApplication().processIdentifier()`. Apply `app_filter` by exact `app` match.

Implement `read_window_text` via the `accessibility` crate: `AXUIElement::application(pid)` for the window's owner pid, locate the matching `AXWindow` child (match by title/position), then depth-first walk collecting `AXValue` / `AXTitle` / `AXDescription` string attributes up to `depth` and a node budget (e.g. 2000 nodes). On `AXError` indicating the API is disabled/unauthorized, return `CaptureError::AccessibilityPermissionDenied`. If the window/pid can't be resolved, return `CaptureError::WindowNotFound(window_id)`.

```rust
// Skeleton — fill CG/AX FFI against the pinned crate versions.
use vantage_core::{
    Bounds, CaptureError, WindowFilter, WindowId, WindowInfo, WindowInspector, WindowText,
};

pub struct MacWindowInspector;

impl MacWindowInspector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacWindowInspector {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowInspector for MacWindowInspector {
    fn list_windows(&self, filter: WindowFilter) -> Result<Vec<WindowInfo>, CaptureError> {
        // 1. CGWindowListCopyWindowInfo(OnScreenOnly, kCGNullWindowID)
        // 2. For each dict: read Number/OwnerName/Name/Bounds/OwnerPID/Layer.
        // 3. Skip Layer != 0 (chrome). Build WindowInfo.
        // 4. focused = OwnerPID == NSWorkspace frontmost pid.
        // 5. Apply app_filter (exact match). on_screen_only is already implied.
        let _ = filter;
        Err(CaptureError::Internal("windows::list_windows unimplemented".into()))
    }

    fn read_window_text(&self, window_id: WindowId, depth: u32) -> Result<WindowText, CaptureError> {
        // 1. Resolve pid for window_id via CGWindowList.
        // 2. AXUIElement::application(pid); find AXWindow child for this window.
        // 3. DFS collect AXValue/AXTitle/AXDescription up to `depth` + node budget.
        // 4. Map AX API-disabled/not-authorized -> AccessibilityPermissionDenied.
        let _ = (window_id, depth);
        Err(CaptureError::Internal("windows::read_window_text unimplemented".into()))
    }
}

fn _bounds_stub() -> Bounds {
    Bounds { x: 0, y: 0, width: 0, height: 0 }
}
```

Replace each stub body with the real FFI. The `_bounds_stub`/`_` bindings exist only to keep the skeleton compiling; delete them as you implement.

- [ ] **Step 4: Export from `crates/platform/macos/src/lib.rs`**

```rust
//! macOS backend implementations of vantage-core capability traits.
mod windows;
pub use windows::MacWindowInspector;
```

- [ ] **Step 5: Build, then run the live tests manually**

Run: `cargo build -p vantage-platform-macos`
Expected: PASS (compiles).

Run (manual, requires permissions granted to the terminal): `cargo test -p vantage-platform-macos --test windows_live -- --ignored`
Expected: PASS — `lists_at_least_one_window` returns a non-empty list; `reads_some_text_from_first_window` completes without a permission error once Accessibility is granted.

- [ ] **Step 6: Commit**

```bash
git add crates/platform/macos
git commit -m "feat(macos): WindowInspector via CGWindowList + AXUIElement tree walk"
```

---

### Task 9: macOS `ScreenCapturer` (xcap capture + region crop)

**Files:**
- Create: `crates/platform/macos/src/capture.rs`
- Modify: `crates/platform/macos/src/lib.rs`, `Cargo.toml`
- Test: `crates/platform/macos/tests/capture_live.rs` (`#[ignore]`)

**Interfaces:**
- Consumes: `vantage_core::{ScreenCapturer, Bounds, RgbaImage, CaptureError}`.
- Produces: `MacScreenCapturer` with `new() -> Self`, implementing `ScreenCapturer`. Global-coordinate `bounds` → find the display containing the region, capture it, crop to the region, return RGBA8. Region fully outside all displays → `InvalidBounds`. Capture failure that indicates missing screen-recording access → `ScreenRecordingPermissionDenied`.

- [ ] **Step 1: Add dependency to `Cargo.toml`**

```toml
# under [target.'cfg(target_os = "macos")'.dependencies]
xcap = "0.9"
image = { workspace = true }
```

- [ ] **Step 2: Write `#[ignore]` test `crates/platform/macos/tests/capture_live.rs`**

```rust
use vantage_core::{Bounds, ScreenCapturer};
use vantage_platform_macos::MacScreenCapturer;

#[test]
#[ignore = "requires Screen Recording permission + a display"]
fn captures_a_small_region() {
    let capturer = MacScreenCapturer::new();
    let img = capturer
        .capture_region(Bounds { x: 0, y: 0, width: 64, height: 64 })
        .expect("capture");
    assert_eq!(img.width, 64);
    assert_eq!(img.height, 64);
    assert_eq!(img.pixels.len(), 64 * 64 * 4);
}
```

- [ ] **Step 3: Implement `crates/platform/macos/src/capture.rs`**

```rust
use vantage_core::{Bounds, CaptureError, RgbaImage, ScreenCapturer};
use xcap::Monitor;

pub struct MacScreenCapturer;

impl MacScreenCapturer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacScreenCapturer {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenCapturer for MacScreenCapturer {
    fn capture_region(&self, bounds: Bounds) -> Result<RgbaImage, CaptureError> {
        if bounds.width == 0 || bounds.height == 0 {
            return Err(CaptureError::InvalidBounds(bounds));
        }
        let monitors = Monitor::all().map_err(|e| classify_capture_error(&e))?;
        // Pick the monitor whose frame contains the region's top-left corner.
        let monitor = monitors
            .into_iter()
            .find(|m| {
                let (mx, my) = (m.x().unwrap_or(0), m.y().unwrap_or(0));
                let (mw, mh) = (m.width().unwrap_or(0) as i32, m.height().unwrap_or(0) as i32);
                bounds.x >= mx && bounds.y >= my && bounds.x < mx + mw && bounds.y < my + mh
            })
            .ok_or(CaptureError::InvalidBounds(bounds))?;

        let mx = monitor.x().unwrap_or(0);
        let my = monitor.y().unwrap_or(0);
        let shot = monitor.capture_image().map_err(|e| classify_capture_error(&e))?; // image::RgbaImage

        // Crop (region is in global coords; translate to monitor-local).
        let local_x = (bounds.x - mx).max(0) as u32;
        let local_y = (bounds.y - my).max(0) as u32;
        let crop_w = bounds.width.min(shot.width().saturating_sub(local_x));
        let crop_h = bounds.height.min(shot.height().saturating_sub(local_y));
        if crop_w == 0 || crop_h == 0 {
            return Err(CaptureError::InvalidBounds(bounds));
        }
        let cropped = image::imageops::crop_imm(&shot, local_x, local_y, crop_w, crop_h).to_image();
        Ok(RgbaImage { width: crop_w, height: crop_h, pixels: cropped.into_raw() })
    }
}

/// xcap surfaces permission failures as capture errors on macOS; map the
/// screen-recording-denied case distinctly, everything else to Internal.
fn classify_capture_error(err: &xcap::XCapError) -> CaptureError {
    let msg = err.to_string().to_lowercase();
    if msg.contains("permission") || msg.contains("denied") || msg.contains("not authorized") {
        CaptureError::ScreenRecordingPermissionDenied
    } else {
        CaptureError::Internal(format!("capture: {err}"))
    }
}
```

Note: confirm `xcap` 0.9 API — `Monitor::all()`, `Monitor::x()/y()/width()/height()` (may return `XCapResult<T>` or `T`; adjust the `.unwrap_or(0)` accordingly), and `capture_image() -> XCapResult<image::RgbaImage>`. If `capture_image` returns a different buffer type, convert to RGBA8 before wrapping. If xcap does not surface a distinguishable permission error, keep the `Internal` fallback and rely on empty/black captures being caught in end-to-end verification.

- [ ] **Step 4: Export from `lib.rs`**

```rust
mod capture;
pub use capture::MacScreenCapturer;
```

- [ ] **Step 5: Build and run the live test manually**

Run: `cargo build -p vantage-platform-macos`
Expected: PASS.

Run: `cargo test -p vantage-platform-macos --test capture_live -- --ignored`
Expected: PASS — returns a 64×64 RGBA buffer once Screen Recording is granted.

- [ ] **Step 6: Commit**

```bash
git add crates/platform/macos
git commit -m "feat(macos): ScreenCapturer via xcap with region crop and error classification"
```

---

### Task 10: macOS `TextRecognizer` (Vision OCR, ocrs fallback)

**Files:**
- Create: `crates/platform/macos/src/ocr.rs`
- Modify: `crates/platform/macos/src/lib.rs`, `Cargo.toml`
- Test: `crates/platform/macos/tests/ocr_live.rs` (`#[ignore]`)

**Interfaces:**
- Consumes: `vantage_core::{TextRecognizer, RgbaImage, CaptureError}`.
- Produces: `MacTextRecognizer` with `new() -> Self`, implementing `TextRecognizer::recognize(&RgbaImage) -> Result<String, CaptureError>`.

**Risk note:** Highest-FFI-risk task. Primary path: `objc2-vision` `VNRecognizeTextRequest` (accurate level) over a `CGImage` built from the RGBA buffer, via `VNImageRequestHandler`, joining recognized-text observations with newlines. If binding Vision proves unreliable, switch to the pure-Rust `ocrs` fallback (below) — the swap is confined to this file because OCR sits behind `TextRecognizer`. Decide primary-vs-fallback here and record it in the commit message.

- [ ] **Step 1: Add dependency (Vision primary) to `Cargo.toml`**

```toml
# under [target.'cfg(target_os = "macos")'.dependencies]
objc2-vision = "0.3"
objc2-foundation = "0.3"
objc2-core-graphics = "0.3"
```

Fallback deps (add instead if choosing ocrs): `ocrs = "0.10"`, `rten = "0.19"` (confirm current versions; ocrs needs detection+recognition model files loaded at runtime).

- [ ] **Step 2: Write `#[ignore]` test `crates/platform/macos/tests/ocr_live.rs`**

```rust
use image::{Rgba, RgbaImage as ImgRgba};
use vantage_core::{RgbaImage, TextRecognizer};
use vantage_platform_macos::MacTextRecognizer;

/// Render the word "HELLO" onto a white image and assert OCR recovers it.
#[test]
#[ignore = "requires macOS Vision framework at runtime"]
fn recognizes_rendered_text() {
    // Build a simple high-contrast image with text using the `image` crate's
    // default font rendering via `imageproc`, OR load a fixture PNG committed
    // under tests/fixtures/hello.png. Using a fixture is more reliable:
    let bytes = include_bytes!("fixtures/hello.png");
    let decoded = image::load_from_memory(bytes).unwrap().to_rgba8();
    let (w, h) = (decoded.width(), decoded.height());
    let img = RgbaImage { width: w, height: h, pixels: decoded.into_raw() };

    let ocr = MacTextRecognizer::new();
    let text = ocr.recognize(&img).expect("ocr");
    assert!(
        text.to_uppercase().contains("HELLO"),
        "expected HELLO in OCR output, got: {text:?}"
    );
    let _ = (Rgba([0u8; 4]), ImgRgba::new(1, 1)); // keep imports used if fixture path changes
}
```

Add a small committed fixture `crates/platform/macos/tests/fixtures/hello.png` containing the high-contrast word "HELLO".

- [ ] **Step 3: Implement `crates/platform/macos/src/ocr.rs` (Vision primary)**

```rust
use vantage_core::{CaptureError, RgbaImage, TextRecognizer};

pub struct MacTextRecognizer;

impl MacTextRecognizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacTextRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRecognizer for MacTextRecognizer {
    fn recognize(&self, image: &RgbaImage) -> Result<String, CaptureError> {
        // 1. Build a CGImage from RGBA8 (CGColorSpace sRGB, 8 bpc, 32 bpp,
        //    premultipliedLast bitmap info) via objc2-core-graphics.
        // 2. Create VNRecognizeTextRequest; set recognition_level = accurate,
        //    uses_language_correction = true.
        // 3. VNImageRequestHandler::from_cgimage(...).perform([request]).
        // 4. Collect request.results() -> [VNRecognizedTextObservation];
        //    for each, top_candidates(1).first().string(); join with "\n".
        // 5. Map framework failures to CaptureError::Internal.
        let _ = image;
        Err(CaptureError::Internal("ocr::recognize unimplemented".into()))
    }
}
```

Replace the body with the real Vision FFI. Keep observations ordered top-to-bottom (Vision returns normalized coordinates; sort by descending `boundingBox.origin.y` if ordering matters for readability).

- [ ] **Step 4: Export from `lib.rs`**

```rust
mod ocr;
pub use ocr::MacTextRecognizer;
```

- [ ] **Step 5: Build and run the live test manually**

Run: `cargo build -p vantage-platform-macos`
Expected: PASS.

Run: `cargo test -p vantage-platform-macos --test ocr_live -- --ignored`
Expected: PASS — OCR output contains "HELLO". If Vision binding is blocked, switch to the ocrs fallback and re-run until this passes.

- [ ] **Step 6: Commit**

```bash
git add crates/platform/macos
git commit -m "feat(macos): TextRecognizer via Vision VNRecognizeTextRequest"
```

---

### Task 11: macOS `ClipboardAccess` (arboard)

**Files:**
- Create: `crates/platform/macos/src/clipboard.rs`
- Modify: `crates/platform/macos/src/lib.rs`, `Cargo.toml`
- Test: `crates/platform/macos/tests/clipboard_live.rs` (`#[ignore]`)

**Interfaces:**
- Consumes: `vantage_core::{ClipboardAccess, ClipboardPrefer, ClipboardKind, ClipboardContent, RgbaImage, CaptureError}`.
- Produces: `MacClipboard` with `new() -> Self`, implementing `ClipboardAccess::read`. `prefer=Text` returns text when present (else tries image, else `Empty`); `prefer=Image` returns image when present (else text, else `Empty`).

- [ ] **Step 1: Add dependency to `Cargo.toml`**

```toml
# under [target.'cfg(target_os = "macos")'.dependencies]
arboard = "3"
```

- [ ] **Step 2: Write `#[ignore]` test `crates/platform/macos/tests/clipboard_live.rs`**

```rust
use vantage_core::{ClipboardAccess, ClipboardKind, ClipboardPrefer};
use vantage_platform_macos::MacClipboard;

#[test]
#[ignore = "mutates the real system clipboard"]
fn reads_back_written_text() {
    let mut board = arboard::Clipboard::new().unwrap();
    board.set_text("vantage-clip-test").unwrap();

    let clip = MacClipboard::new();
    let content = clip.read(ClipboardPrefer::Text).expect("read");
    assert_eq!(content.kind, ClipboardKind::Text);
    assert_eq!(content.text.as_deref(), Some("vantage-clip-test"));
}
```

- [ ] **Step 3: Implement `crates/platform/macos/src/clipboard.rs`**

```rust
use vantage_core::{
    CaptureError, ClipboardAccess, ClipboardContent, ClipboardKind, ClipboardPrefer, RgbaImage,
};

pub struct MacClipboard;

impl MacClipboard {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardAccess for MacClipboard {
    fn read(&self, prefer: ClipboardPrefer) -> Result<ClipboardContent, CaptureError> {
        let mut board = arboard::Clipboard::new()
            .map_err(|e| CaptureError::Internal(format!("clipboard open: {e}")))?;

        let get_text = |b: &mut arboard::Clipboard| b.get_text().ok();
        let get_image = |b: &mut arboard::Clipboard| {
            b.get_image().ok().map(|img| RgbaImage {
                width: img.width as u32,
                height: img.height as u32,
                pixels: img.bytes.into_owned(),
            })
        };

        let (text, image) = match prefer {
            ClipboardPrefer::Text => {
                let t = get_text(&mut board);
                let i = if t.is_none() { get_image(&mut board) } else { None };
                (t, i)
            }
            ClipboardPrefer::Image => {
                let i = get_image(&mut board);
                let t = if i.is_none() { get_text(&mut board) } else { None };
                (t, i)
            }
        };

        let kind = if text.is_some() {
            ClipboardKind::Text
        } else if image.is_some() {
            ClipboardKind::Image
        } else {
            ClipboardKind::Empty
        };
        Ok(ClipboardContent { kind, text, image })
    }
}
```

Note: confirm arboard 3 `ImageData` fields — `width: usize`, `height: usize`, `bytes: Cow<[u8]>` (RGBA8). Adjust the `.into_owned()`/casts if the field names differ.

- [ ] **Step 4: Export from `lib.rs`**

```rust
mod clipboard;
pub use clipboard::MacClipboard;
```

- [ ] **Step 5: Build and run the live test manually**

Run: `cargo build -p vantage-platform-macos`
Expected: PASS.

Run: `cargo test -p vantage-platform-macos --test clipboard_live -- --ignored`
Expected: PASS — reads back the written text.

- [ ] **Step 6: Commit**

```bash
git add crates/platform/macos
git commit -m "feat(macos): ClipboardAccess via arboard (text + image)"
```

---

### Task 12: Wire backends into `main`, un-ignore boot test, docs + end-to-end verification

**Files:**
- Modify: `crates/mcp-server/src/main.rs` (already references `backend::Mac*`; confirm names)
- Modify: `crates/mcp-server/tests/boot.rs` (remove any temporary `#[ignore]`)
- Create: `README.md`
- Create: `docs/agent-registration.md`

**Interfaces:**
- Consumes: `MacWindowInspector`, `MacScreenCapturer`, `MacTextRecognizer`, `MacClipboard` (Tasks 8–11).
- Produces: a runnable `vantage-mcp` binary registerable with a local MCP agent.

- [ ] **Step 1: Confirm `main.rs` constructs the real backends**

The `main.rs` from Task 3 already builds `MacWindowInspector::new()` etc. Ensure the `use vantage_platform_macos as backend;` names match the exports from Tasks 8–11 (`backend::MacWindowInspector`, `MacScreenCapturer`, `MacTextRecognizer`, `MacClipboard`). Remove any temporary test doubles introduced in Task 3.

- [ ] **Step 2: Un-ignore and run the boot integration test**

Remove the temporary `#[ignore]` (if added) from `crates/mcp-server/tests/boot.rs`.

Run: `cargo test -p vantage-mcp-server --test boot`
Expected: PASS — initialize handshake returns a JSON-RPC result on stdout; first stdout line parses as JSON-RPC (proves stdout is clean).

- [ ] **Step 3: Full workspace test run**

Run: `cargo test --workspace`
Expected: PASS — all non-ignored unit + integration tests. (`#[ignore]` live macOS tests are excluded by default.)

- [ ] **Step 4: Manual smoke run of the binary**

Run: `cargo build --release` then start the binary and send an `initialize` + `tools/list` manually, e.g.:

```bash
printf '%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"cli","version":"0"}}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | ./target/release/vantage-mcp
```

Expected: two JSON-RPC responses on stdout; `tools/list` lists exactly `list_windows`, `read_window_text`, `capture_region`, `read_clipboard`. Logs appear on stderr, never mixed into stdout.

- [ ] **Step 5: Write `README.md`**

Cover: overview, prerequisites (macOS 12.3+, Rust 1.95, grant Screen Recording + Accessibility to the launching agent/terminal), build (`cargo build --release`), the four tools and their params/defaults (text-first), the stdout-is-sacred / stderr-logging note, and a "Roadmap" section pointing to later phases (Linux/X11, image output surface, gated act tools) per the PRD. Follow the README template in the global CLAUDE.md.

- [ ] **Step 6: Write `docs/agent-registration.md`**

Document registering the stdio binary with a local MCP agent (Claude Code / Claude Desktop): the JSON config block pointing `command` at the built binary path, and the macOS permission-granting steps (System Settings → Privacy & Security → Screen Recording and Accessibility → add the terminal/agent app), plus how permission-denied surfaces as an actionable MCP error.

- [ ] **Step 7: End-to-end verification against a real agent**

Register the binary with a local MCP agent and, from the agent, call `list_windows` then `read_window_text` on a returned window id. Confirm a single-round-trip text result and that a revoked permission yields the actionable error (not a hang). This is the phase's definition of done (spec §8).

- [ ] **Step 8: Commit**

```bash
git add crates/mcp-server README.md docs/agent-registration.md
git commit -m "feat: wire macOS backends into main, docs, and end-to-end verification"
```

---

## Self-Review

**Spec coverage** (spec §-by-§ → task):
- §1 objective / success criteria → text-first defaults (Tasks 4–7), actionable errors (Task 3 + variant mapping), clean stdout (Task 3 boot test), single-round-trip read (Task 12 e2e). ✅
- §2 workspace layout → Task 1. ✅
- §3 capability traits → Task 2 (`InputController` intentionally omitted this phase — it is a Phase-3 act capability; spec §3 lists it as "defined but Unsupported" for contract completeness. Not needed for any Phase-1 read tool, so omitted to avoid dead code; noted here as a deliberate deviation). ✅ (deviation recorded)
- §4 tool surface → `list_windows` (4), `read_window_text` (5), `read_clipboard` (6), `capture_region` (7). ✅
- §5 platform impl → WindowInspector (8), ScreenCapturer (9), TextRecognizer + Vision/ocrs (10), ClipboardAccess (11). ✅
- §6 error model → Task 2 enum + Task 3 mapping test (permission ≠ internal). ✅
- §7 non-functional: stdout-sacred + stderr logging (Task 3), text-first + downscaling (Task 7), graceful degradation via `Unsupported`/actionable errors (Tasks 3, 8–11). ✅
- §8 testing: core unit (2), handler mock (4–7), image_out unit (7), platform `#[ignore]` integration (8–11), e2e (12). ✅
- §9 transport/permissions → stdio (Task 1/3), dev permission posture documented (Task 12). ✅
- §10 out-of-scope → nothing in the plan implements act tools / Linux / capture_window / list_displays. ✅

**Deviation from spec §3:** the spec keeps an `InputController` trait as an `Unsupported` stub for contract stability. This plan omits it because no Phase-1 tool consumes it and an unused trait is dead code. When Phase 3 is specced it will be introduced then. If strict spec fidelity is preferred, add a one-file `crates/core/src/traits.rs` `InputController` trait with methods returning `CaptureError::Unsupported` — but that is not required for a working, tested Phase-1 deliverable.

**Placeholder scan:** No "TBD/TODO" in shipping code steps. The macOS FFI tasks (8, 10) ship skeletons with explicit `unimplemented`-returning bodies that each task's own steps replace with real FFI and verify via `#[ignore]` live tests — these are spike tasks by nature, flagged with risk notes and fallbacks, not hidden placeholders.

**Type consistency:** trait signatures in Task 2 (`list_windows(WindowFilter)`, `read_window_text(WindowId, u32)`, `capture_region(Bounds)`, `recognize(&RgbaImage)`, `read(ClipboardPrefer)`) match every consumer (Tasks 4–11) and every mock. `RgbaImage { width, height, pixels }`, `Bounds { x, y, width, height }`, `ClipboardKind` variants, and constant names (`DEFAULT_DEPTH`, `MAX_DEPTH`, `DEFAULT_MAX_DIMENSION`) are used identically across tasks. `Json`/`Parameters`/`ErrorData`/`to_mcp_error` names are consistent across all handlers.
