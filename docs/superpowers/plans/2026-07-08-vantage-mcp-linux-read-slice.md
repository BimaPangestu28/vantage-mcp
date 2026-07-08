# vantage-mcp Linux read-slice (Spec A) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a Linux backend that makes the existing `vantage-mcp` binary work on Linux (X11 + Wayland) with the same four read tools it has on macOS, and make the whole workspace build on both platforms.

**Architecture:** Add a `vantage-platform-linux` crate implementing the four `vantage-core` capability traits (window inspection + text via AT-SPI2, capture via xcap, OCR via Tesseract, clipboard via arboard). Make `mcp-server` select its backend at compile time via `cfg(target_os)`, and expose a uniform `backends()` constructor from each platform crate so `main.rs` stays platform-agnostic. Build the Linux crate stub-first so the workspace compiles immediately, then replace each stub capability with a real implementation, TDD-style.

**Tech Stack:** Rust 1.95; `vantage-core` traits; Linux backends: `atspi` (+ `zbus`, `tokio` current-thread runtime for the async→sync bridge), `xcap` 0.9 (shared with macOS; `ashpd` portal fallback only if the capture spike needs it), `tesseract`, `arboard` 3, `image` 0.25.

## Global Constraints

- **Core is frozen:** do NOT change `crates/core` (traits, value types, `CaptureError`/`ErrorKind`). The Linux backend implements the exact same traits the macOS backend does.
- **Platform:** Linux (X11 and Wayland, runtime-detected) this spec; must not break the existing macOS build. `cargo build --workspace` must stay green on both.
- **Stdout is sacred:** nothing writes to stdout except the rmcp JSON-RPC stream. All logging → stderr. No `println!`/`dbg!` in shipping code.
- **Text-first:** unchanged — the handler layer is not modified behaviourally.
- **No production `unwrap()`/`panic!`:** backend code returns `CaptureError`; tests may unwrap.
- **Session divergence is confined to capture:** window/text (AT-SPI), clipboard (arboard), OCR (Tesseract) are session-agnostic at our layer.
- **Error variants are fixed:** map Linux failures onto the existing six `CaptureError` variants (§2.8 of the spec). Only the remediation *text* in `error_map.rs` becomes platform-aware.
- **Commit** after every task's tests pass. Conventional commit messages (`feat:`, `test:`, `chore:`, `fix:`).

---

### Task 1: Linux crate scaffold + cross-platform binary wiring (stub backends)

Establishes the workspace change: a `vantage-platform-linux` crate with four
stub backends, a uniform `backends()` on both platform crates, and cfg-based
backend selection in `mcp-server`. After this task the workspace builds on Linux
and the boot test runs here.

**Files:**
- Create: `crates/platform/linux/Cargo.toml`
- Create: `crates/platform/linux/src/lib.rs`
- Create: `crates/platform/linux/src/windows.rs`
- Create: `crates/platform/linux/src/capture.rs`
- Create: `crates/platform/linux/src/ocr.rs`
- Create: `crates/platform/linux/src/clipboard.rs`
- Modify: `Cargo.toml` (workspace members)
- Modify: `crates/platform/macos/src/lib.rs` (add `backends()`)
- Modify: `crates/mcp-server/Cargo.toml` (cfg-gated platform deps)
- Modify: `crates/mcp-server/src/main.rs` (cfg backend selection)

**Interfaces:**
- Consumes: `vantage_core::{WindowInspector, ScreenCapturer, TextRecognizer, ClipboardAccess, ...}`.
- Produces:
  - `vantage_platform_linux::backends() -> (Arc<dyn WindowInspector>, Arc<dyn ScreenCapturer>, Arc<dyn TextRecognizer>, Arc<dyn ClipboardAccess>)`.
  - `vantage_platform_macos::backends()` with the identical signature.
  - Stub structs `LinuxWindowInspector`, `LinuxScreenCapturer`, `LinuxTextRecognizer`, `LinuxClipboard`, each with `new() -> Self` and a trait impl. These are rewritten in Tasks 2–6; **`backends()` does not change after this task** — only these modules' bodies do.

- [ ] **Step 1: Write `crates/platform/linux/Cargo.toml`**

```toml
[package]
name = "vantage-platform-linux"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
vantage-core = { path = "../../core" }

# Real backend deps are added per-capability in Tasks 2–6 under:
# [target.'cfg(target_os = "linux")'.dependencies]
```

- [ ] **Step 2: Write the four stub backend modules**

`crates/platform/linux/src/clipboard.rs`:

```rust
use vantage_core::{
    CaptureError, ClipboardAccess, ClipboardContent, ClipboardPrefer,
};

pub struct LinuxClipboard;

impl LinuxClipboard {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardAccess for LinuxClipboard {
    fn read(&self, _prefer: ClipboardPrefer) -> Result<ClipboardContent, CaptureError> {
        Err(CaptureError::Unsupported("linux clipboard not yet implemented".into()))
    }
}
```

`crates/platform/linux/src/capture.rs`:

```rust
use vantage_core::{Bounds, CaptureError, RgbaImage, ScreenCapturer};

pub struct LinuxScreenCapturer;

impl LinuxScreenCapturer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxScreenCapturer {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenCapturer for LinuxScreenCapturer {
    fn capture_region(&self, _bounds: Bounds) -> Result<RgbaImage, CaptureError> {
        Err(CaptureError::Unsupported("linux capture not yet implemented".into()))
    }
}
```

`crates/platform/linux/src/ocr.rs`:

```rust
use vantage_core::{CaptureError, RgbaImage, TextRecognizer};

pub struct LinuxTextRecognizer;

impl LinuxTextRecognizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxTextRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRecognizer for LinuxTextRecognizer {
    fn recognize(&self, _image: &RgbaImage) -> Result<String, CaptureError> {
        Err(CaptureError::Unsupported("linux ocr not yet implemented".into()))
    }
}
```

`crates/platform/linux/src/windows.rs`:

```rust
use vantage_core::{
    CaptureError, WindowFilter, WindowId, WindowInfo, WindowInspector, WindowText,
};

pub struct LinuxWindowInspector;

impl LinuxWindowInspector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxWindowInspector {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowInspector for LinuxWindowInspector {
    fn list_windows(&self, _filter: WindowFilter) -> Result<Vec<WindowInfo>, CaptureError> {
        Err(CaptureError::Unsupported("linux window inspection not yet implemented".into()))
    }

    fn read_window_text(
        &self,
        _window_id: WindowId,
        _depth: u32,
    ) -> Result<WindowText, CaptureError> {
        Err(CaptureError::Unsupported("linux window text not yet implemented".into()))
    }
}
```

- [ ] **Step 3: Write `crates/platform/linux/src/lib.rs` with `backends()`**

```rust
//! Linux backend implementations of vantage-core capability traits.
//!
//! Everything OS-specific is gated behind `#[cfg(target_os = "linux")]` so this
//! crate compiles to an empty lib on non-Linux hosts (mirrors the macOS crate).
use std::sync::Arc;

use vantage_core::{ClipboardAccess, ScreenCapturer, TextRecognizer, WindowInspector};

#[cfg(target_os = "linux")]
mod capture;
#[cfg(target_os = "linux")]
mod clipboard;
#[cfg(target_os = "linux")]
mod ocr;
#[cfg(target_os = "linux")]
mod windows;

#[cfg(target_os = "linux")]
pub use capture::LinuxScreenCapturer;
#[cfg(target_os = "linux")]
pub use clipboard::LinuxClipboard;
#[cfg(target_os = "linux")]
pub use ocr::LinuxTextRecognizer;
#[cfg(target_os = "linux")]
pub use windows::LinuxWindowInspector;

/// Construct the four Linux backends as trait objects. The single seam
/// `main.rs` uses; identical signature to `vantage_platform_macos::backends()`.
#[cfg(target_os = "linux")]
pub fn backends() -> (
    Arc<dyn WindowInspector>,
    Arc<dyn ScreenCapturer>,
    Arc<dyn TextRecognizer>,
    Arc<dyn ClipboardAccess>,
) {
    (
        Arc::new(LinuxWindowInspector::new()),
        Arc::new(LinuxScreenCapturer::new()),
        Arc::new(LinuxTextRecognizer::new()),
        Arc::new(LinuxClipboard::new()),
    )
}
```

- [ ] **Step 4: Add `backends()` to the macOS crate**

Rewrite `crates/platform/macos/src/lib.rs` to add the uniform constructor,
keeping every existing `#[cfg(target_os = "macos")]` export:

```rust
//! macOS backend implementations of vantage-core capability traits.
use std::sync::Arc;

use vantage_core::{ClipboardAccess, ScreenCapturer, TextRecognizer, WindowInspector};

#[cfg(target_os = "macos")]
mod capture;
#[cfg(target_os = "macos")]
mod clipboard;
#[cfg(target_os = "macos")]
mod ocr;
#[cfg(target_os = "macos")]
mod windows;
#[cfg(target_os = "macos")]
pub use capture::MacScreenCapturer;
#[cfg(target_os = "macos")]
pub use clipboard::MacClipboard;
#[cfg(target_os = "macos")]
pub use ocr::MacTextRecognizer;
#[cfg(target_os = "macos")]
pub use windows::MacWindowInspector;

/// Construct the four macOS backends as trait objects. Identical signature to
/// `vantage_platform_linux::backends()`.
#[cfg(target_os = "macos")]
pub fn backends() -> (
    Arc<dyn WindowInspector>,
    Arc<dyn ScreenCapturer>,
    Arc<dyn TextRecognizer>,
    Arc<dyn ClipboardAccess>,
) {
    (
        Arc::new(MacWindowInspector::new()),
        Arc::new(MacScreenCapturer::new()),
        Arc::new(MacTextRecognizer::new()),
        Arc::new(MacClipboard::new()),
    )
}
```

- [ ] **Step 5: Add the Linux crate to the workspace**

In the root `Cargo.toml`, add the crate to `members`:

```toml
members = ["crates/core", "crates/platform/macos", "crates/platform/linux", "crates/mcp-server"]
```

- [ ] **Step 6: Rewire `crates/mcp-server/Cargo.toml` platform deps**

Replace the two unconditional `vantage-platform-macos` lines under
`[dependencies]` with cfg-gated tables. Remove `vantage-platform-macos` from the
plain `[dependencies]` table; keep everything else. Add:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
vantage-platform-macos = { path = "../platform/macos" }

[target.'cfg(target_os = "linux")'.dependencies]
vantage-platform-linux = { path = "../platform/linux" }
```

- [ ] **Step 7: Rewrite the backend selection in `crates/mcp-server/src/main.rs`**

Replace the `use vantage_platform_macos as backend;` line and the four
`Arc::new(backend::Mac*::new())` lines. The new `main.rs` body:

```rust
mod error_map;
mod handler;
mod image_out;
mod logging;

use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};

use handler::Vantage;

#[cfg(target_os = "macos")]
use vantage_platform_macos as backend;
#[cfg(target_os = "linux")]
use vantage_platform_linux as backend;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("vantage-mcp supports macOS and Linux only");

#[tokio::main]
async fn main() -> Result<()> {
    logging::init();
    tracing::info!("vantage-mcp starting (stdio); logging on stderr only");

    let (windows, capturer, ocr, clipboard) = backend::backends();

    let service = Vantage::new(windows, capturer, ocr, clipboard)
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
```

- [ ] **Step 8: Build the workspace on Linux**

Run: `cargo build --workspace`
Expected: PASS — all four crates compile on Linux; `vantage-mcp` now links the
Linux backend. (Previously this failed with `could not find MacClipboard in backend`.)

- [ ] **Step 9: Run the boot test (now runnable on Linux)**

Run: `cargo test -p vantage-mcp-server`
Expected: PASS — `error_map` + `handler` unit tests and the `boot` integration
test all pass. The server boots, answers `initialize` + `tools/list`, and stdout
carries only JSON-RPC. (The four tools are present but every backend call returns
`Unsupported` for now — that is fine; the boot test does not invoke them.)

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml Cargo.lock crates/platform/linux crates/platform/macos/src/lib.rs crates/mcp-server/Cargo.toml crates/mcp-server/src/main.rs
git commit -m "feat(linux): crate scaffold + cross-platform backend selection (stubs)"
```

---

### Task 2: Linux clipboard (arboard)

Simplest real capability; no session-specific branching (arboard handles X11 and
Wayland internally). Mirrors the macOS `clipboard.rs` almost 1:1.

**Files:**
- Modify: `crates/platform/linux/src/clipboard.rs`
- Modify: `crates/platform/linux/Cargo.toml` (add `arboard`)
- Test: `crates/platform/linux/tests/clipboard_live.rs` (`#[ignore]`)

**Interfaces:**
- Consumes: `vantage_core::{ClipboardAccess, ClipboardPrefer, ClipboardKind, ClipboardContent, RgbaImage, CaptureError}`.
- Produces: `LinuxClipboard::read` returning real clipboard content. `prefer=Text` returns text if present, else image, else `Empty`; `prefer=Image` the converse.

- [ ] **Step 1: Add the dependency to `crates/platform/linux/Cargo.toml`**

```toml
[target.'cfg(target_os = "linux")'.dependencies]
arboard = "3"
```

- [ ] **Step 2: Write the `#[ignore]` live test `crates/platform/linux/tests/clipboard_live.rs`**

```rust
//! Live clipboard test. Mutates the real system clipboard.
//! Run manually: `cargo test -p vantage-platform-linux --test clipboard_live -- --ignored`
#![cfg(target_os = "linux")]

use vantage_core::{ClipboardAccess, ClipboardKind, ClipboardPrefer};
use vantage_platform_linux::LinuxClipboard;

#[test]
#[ignore = "mutates the real system clipboard; needs a desktop session"]
fn reads_back_written_text() {
    let mut board = arboard::Clipboard::new().unwrap();
    board.set_text("vantage-clip-test").unwrap();

    let clip = LinuxClipboard::new();
    let content = clip.read(ClipboardPrefer::Text).expect("read");
    assert_eq!(content.kind, ClipboardKind::Text);
    assert_eq!(content.text.as_deref(), Some("vantage-clip-test"));
}
```

- [ ] **Step 3: Implement `LinuxClipboard::read` in `crates/platform/linux/src/clipboard.rs`**

```rust
use vantage_core::{
    CaptureError, ClipboardAccess, ClipboardContent, ClipboardKind, ClipboardPrefer, RgbaImage,
};

pub struct LinuxClipboard;

impl LinuxClipboard {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardAccess for LinuxClipboard {
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

Note: `arboard::ImageData` fields are `width: usize`, `height: usize`,
`bytes: Cow<[u8]>` (RGBA8). Confirm against arboard 3 and adjust casts if the
field names differ.

- [ ] **Step 4: Build, then run the live test manually**

Run: `cargo build -p vantage-platform-linux`
Expected: PASS.

Run: `cargo test -p vantage-platform-linux --test clipboard_live -- --ignored`
Expected: PASS — reads back the written text. (On a headless box with no
clipboard provider, arboard may fail to open; run in the desktop session.)

- [ ] **Step 5: Commit**

```bash
git add crates/platform/linux
git commit -m "feat(linux): ClipboardAccess via arboard (text + image)"
```

---

### Task 3: Linux screen capture (xcap) + Wayland spike

Implement region capture via xcap, mirroring the macOS `capture.rs`. Spike-verify
on the reference GNOME/Wayland box; if xcap's Wayland path is unreliable, add the
`ashpd` portal fallback (decision recorded in the commit message).

**Files:**
- Modify: `crates/platform/linux/src/capture.rs`
- Modify: `crates/platform/linux/Cargo.toml` (add `xcap`, `image`; `ashpd` only if the spike needs it)
- Test: `crates/platform/linux/tests/capture_live.rs` (`#[ignore]`)

**Interfaces:**
- Consumes: `vantage_core::{ScreenCapturer, Bounds, RgbaImage, CaptureError}`.
- Produces: `LinuxScreenCapturer::capture_region` — global-coordinate `Bounds` → find the monitor containing the region, capture, crop, return RGBA8. Zero-size/outside all monitors → `InvalidBounds`. Capture-permission failure → `ScreenRecordingPermissionDenied`.

- [ ] **Step 1: Add dependencies to `crates/platform/linux/Cargo.toml`**

```toml
# under [target.'cfg(target_os = "linux")'.dependencies]
xcap = "0.9"
image = { workspace = true }
```

- [ ] **Step 2: Write the `#[ignore]` live test `crates/platform/linux/tests/capture_live.rs`**

```rust
//! Live capture test. Needs a desktop session; on Wayland the compositor may
//! prompt for screen-capture permission the first time.
//! Run manually: `cargo test -p vantage-platform-linux --test capture_live -- --ignored`
#![cfg(target_os = "linux")]

use vantage_core::{Bounds, ScreenCapturer};
use vantage_platform_linux::LinuxScreenCapturer;

#[test]
#[ignore = "requires a desktop session + screen-capture permission"]
fn captures_a_small_region() {
    let capturer = LinuxScreenCapturer::new();
    let img = capturer
        .capture_region(Bounds { x: 0, y: 0, width: 64, height: 64 })
        .expect("capture");
    assert_eq!(img.width, 64);
    assert_eq!(img.height, 64);
    assert_eq!(img.pixels.len(), 64 * 64 * 4);
}
```

- [ ] **Step 3: Implement `LinuxScreenCapturer::capture_region` (xcap primary)**

```rust
use vantage_core::{Bounds, CaptureError, RgbaImage, ScreenCapturer};
use xcap::Monitor;

pub struct LinuxScreenCapturer;

impl LinuxScreenCapturer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxScreenCapturer {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenCapturer for LinuxScreenCapturer {
    fn capture_region(&self, bounds: Bounds) -> Result<RgbaImage, CaptureError> {
        if bounds.width == 0 || bounds.height == 0 {
            return Err(CaptureError::InvalidBounds(bounds));
        }
        let monitors = Monitor::all().map_err(|e| classify_capture_error(&e))?;
        let monitor = monitors
            .into_iter()
            .find(|m| {
                let mx = m.x().unwrap_or(0);
                let my = m.y().unwrap_or(0);
                let mw = m.width().unwrap_or(0) as i32;
                let mh = m.height().unwrap_or(0) as i32;
                bounds.x >= mx && bounds.y >= my && bounds.x < mx + mw && bounds.y < my + mh
            })
            .ok_or(CaptureError::InvalidBounds(bounds))?;

        let mx = monitor.x().unwrap_or(0);
        let my = monitor.y().unwrap_or(0);
        let shot = monitor.capture_image().map_err(|e| classify_capture_error(&e))?;

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

/// Map a screen-capture-permission failure distinctly; everything else Internal.
fn classify_capture_error(err: &xcap::XCapError) -> CaptureError {
    let msg = err.to_string().to_lowercase();
    if msg.contains("permission") || msg.contains("denied") || msg.contains("authorized") {
        CaptureError::ScreenRecordingPermissionDenied
    } else {
        CaptureError::Internal(format!("capture: {err}"))
    }
}
```

Note: confirm `xcap` 0.9 API — `Monitor::all()`, `Monitor::x()/y()/width()/height()`
(return `XCapResult<T>`; adjust `.unwrap_or(0)`), `capture_image() -> XCapResult<image::RgbaImage>`.

- [ ] **Step 4: Build and run the capture spike on the reference box**

Run: `cargo build -p vantage-platform-linux`
Expected: PASS.

Run: `cargo test -p vantage-platform-linux --test capture_live -- --ignored`
Expected (goal): PASS — returns a 64×64 RGBA buffer with non-uniform pixels.

**Spike decision:** if the capture returns all-zero/black pixels or fails on
GNOME/Wayland, implement the portal fallback in Step 5. If it passes with real
pixels, skip Step 5 and note "xcap Wayland path sufficient" in the commit.

- [ ] **Step 5 (conditional): Add the `ashpd` portal fallback for Wayland**

Only if Step 4 failed on Wayland. Add `ashpd = "0.9"` and `tokio` (`rt`) to the
Linux deps. Add a runtime branch at the top of `capture_region`:

```rust
// At the start of capture_region, before the xcap path:
let is_wayland = std::env::var("XDG_SESSION_TYPE")
    .map(|v| v.eq_ignore_ascii_case("wayland"))
    .unwrap_or(false)
    || std::env::var("WAYLAND_DISPLAY").is_ok();
if is_wayland {
    return capture_region_portal(bounds);
}
```

```rust
/// Wayland fallback: full-screen shot via the desktop Screenshot portal, then
/// crop to `bounds`. Runs the async portal call on a private current-thread runtime.
fn capture_region_portal(bounds: Bounds) -> Result<RgbaImage, CaptureError> {
    use ashpd::desktop::screenshot::Screenshot;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CaptureError::Internal(format!("runtime: {e}")))?;

    let uri = rt
        .block_on(async {
            Screenshot::request()
                .interactive(false)
                .modal(false)
                .send()
                .await?
                .response()
        })
        .map_err(|e| {
            let msg = e.to_string().to_lowercase();
            if msg.contains("cancel") || msg.contains("denied") || msg.contains("permission") {
                CaptureError::ScreenRecordingPermissionDenied
            } else {
                CaptureError::Internal(format!("portal screenshot: {e}"))
            }
        })?;

    let path = uri
        .uri()
        .to_file_path()
        .map_err(|_| CaptureError::Internal("portal returned a non-file URI".into()))?;
    let full = image::open(&path)
        .map_err(|e| CaptureError::Internal(format!("decode portal shot: {e}")))?
        .to_rgba8();

    let x = bounds.x.max(0) as u32;
    let y = bounds.y.max(0) as u32;
    let crop_w = bounds.width.min(full.width().saturating_sub(x));
    let crop_h = bounds.height.min(full.height().saturating_sub(y));
    if crop_w == 0 || crop_h == 0 {
        return Err(CaptureError::InvalidBounds(bounds));
    }
    let cropped = image::imageops::crop_imm(&full, x, y, crop_w, crop_h).to_image();
    Ok(RgbaImage { width: crop_w, height: crop_h, pixels: cropped.into_raw() })
}
```

Note: confirm the `ashpd` 0.9 `Screenshot` builder API (`request()/interactive()/send()/response()`
and `uri()`). Adjust to the resolved version.

- [ ] **Step 6: Re-run the live test to confirm the chosen path works**

Run: `cargo test -p vantage-platform-linux --test capture_live -- --ignored`
Expected: PASS on the reference box via whichever path was selected.

- [ ] **Step 7: Commit**

```bash
git add crates/platform/linux
git commit -m "feat(linux): ScreenCapturer via xcap (+portal fallback if needed)"
```

---

### Task 4: Linux OCR (Tesseract)

Implement `recognize` via the `tesseract` crate. Requires libtesseract +
leptonica + `eng` traineddata installed on the host.

**Files:**
- Modify: `crates/platform/linux/src/ocr.rs`
- Modify: `crates/platform/linux/Cargo.toml` (add `tesseract`)
- Create: `crates/platform/linux/tests/ocr_live.rs` (`#[ignore]`)
- Create: `crates/platform/linux/tests/fixtures/hello.png` (copy of the macOS fixture)

**Interfaces:**
- Consumes: `vantage_core::{TextRecognizer, RgbaImage, CaptureError}`.
- Produces: `LinuxTextRecognizer::recognize(&RgbaImage) -> Result<String, CaptureError>`.

- [ ] **Step 1: Install the system dependency on the host**

Run: `sudo apt-get install -y libtesseract-dev libleptonica-dev tesseract-ocr-eng`
Expected: installs the OCR library + English traineddata. If `sudo` is
unavailable in this session, ask the user to run it via `!sudo apt-get install …`.

- [ ] **Step 2: Add the dependency to `crates/platform/linux/Cargo.toml`**

```toml
# under [target.'cfg(target_os = "linux")'.dependencies]
tesseract = "0.15"
```

Note: confirm the current `tesseract` crate version resolves against Rust 1.95.

- [ ] **Step 3: Copy the OCR fixture**

Run: `cp crates/platform/macos/tests/fixtures/hello.png crates/platform/linux/tests/fixtures/hello.png`
Expected: a committed high-contrast "HELLO" PNG under the Linux crate's tests.

- [ ] **Step 4: Write the `#[ignore]` live test `crates/platform/linux/tests/ocr_live.rs`**

```rust
//! Live OCR test. Requires libtesseract + `eng` traineddata installed.
//! Run manually: `cargo test -p vantage-platform-linux --test ocr_live -- --ignored`
#![cfg(target_os = "linux")]

use vantage_core::{RgbaImage, TextRecognizer};
use vantage_platform_linux::LinuxTextRecognizer;

#[test]
#[ignore = "requires libtesseract + eng traineddata"]
fn recognizes_rendered_text() {
    let bytes = include_bytes!("fixtures/hello.png");
    let decoded = image::load_from_memory(bytes)
        .expect("fixture hello.png should decode")
        .to_rgba8();
    let (width, height) = (decoded.width(), decoded.height());
    let img = RgbaImage { width, height, pixels: decoded.into_raw() };

    let ocr = LinuxTextRecognizer::new();
    let text = ocr.recognize(&img).expect("ocr");
    assert!(
        text.to_uppercase().contains("HELLO"),
        "expected HELLO in OCR output, got: {text:?}"
    );
}
```

The test needs `image` as a dev-dependency of the linux crate; it is already a
target dependency (Task 3), which is visible to integration tests, so no extra
entry is required.

- [ ] **Step 5: Implement `LinuxTextRecognizer::recognize` in `crates/platform/linux/src/ocr.rs`**

```rust
use vantage_core::{CaptureError, RgbaImage, TextRecognizer};

pub struct LinuxTextRecognizer;

impl LinuxTextRecognizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxTextRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRecognizer for LinuxTextRecognizer {
    fn recognize(&self, image: &RgbaImage) -> Result<String, CaptureError> {
        // `tesseract` 0.15: build an API handle for English, feed the raw RGBA
        // frame (4 bytes/pixel, stride = width*4), then read recognized text.
        let api = tesseract::Tesseract::new(None, Some("eng")).map_err(|e| {
            CaptureError::Internal(format!(
                "Tesseract init failed ({e}). Install libtesseract + the 'eng' \
                 traineddata (e.g. apt install libtesseract-dev tesseract-ocr-eng)."
            ))
        })?;
        let bytes_per_pixel = 4;
        let bytes_per_line = image.width * bytes_per_pixel;
        let text = api
            .set_frame(
                &image.pixels,
                image.width as i32,
                image.height as i32,
                bytes_per_pixel as i32,
                bytes_per_line as i32,
            )
            .and_then(|api| api.get_text())
            .map_err(|e| CaptureError::Internal(format!("tesseract recognize: {e}")))?;
        Ok(text)
    }
}
```

Note: the `tesseract` 0.15 builder methods (`Tesseract::new`, `set_frame`,
`get_text`) consume and return `self` (`Result<Tesseract, _>`); confirm the exact
signatures against the resolved version and adjust the chaining if needed.

- [ ] **Step 6: Build and run the live test**

Run: `cargo build -p vantage-platform-linux`
Expected: PASS.

Run: `cargo test -p vantage-platform-linux --test ocr_live -- --ignored`
Expected: PASS — OCR output contains "HELLO".

- [ ] **Step 7: Commit**

```bash
git add crates/platform/linux
git commit -m "feat(linux): TextRecognizer via Tesseract"
```

---

### Task 5: Linux window enumeration (AT-SPI2) — `list_windows`

The hardest task (spike), analogous to the macOS AX work. Establishes the AT-SPI
connection, the async→sync bridge, frame enumeration, and the `window_id` hash.

**Files:**
- Modify: `crates/platform/linux/src/windows.rs`
- Create: `crates/platform/linux/src/atspi_conn.rs` (connection + runtime bridge helper)
- Modify: `crates/platform/linux/Cargo.toml` (add `atspi`, `zbus`, `tokio`)
- Test: `crates/platform/linux/tests/windows_live.rs` (`#[ignore]`)

**Interfaces:**
- Consumes: `vantage_core::{WindowInspector, WindowFilter, WindowInfo, WindowText, WindowId, Bounds, CaptureError}`.
- Produces: `LinuxWindowInspector::list_windows` returning frames from the live
  AT-SPI tree. Also a private module `atspi_conn` exposing a helper that owns a
  current-thread `tokio::runtime::Runtime` and an AT-SPI connection, plus
  `fn window_id_hash(bus_name: &str, object_path: &str) -> u32` (FNV-1a).

- [ ] **Step 1: Add dependencies to `crates/platform/linux/Cargo.toml`**

```toml
# under [target.'cfg(target_os = "linux")'.dependencies]
atspi = "0.26"
zbus = "5"
tokio = { version = "1", features = ["rt"] }
```

Note: pin `atspi`/`zbus` to versions that resolve together against Rust 1.95;
the versions above are a starting point — adjust to what `cargo update` resolves.

- [ ] **Step 2: Write the deterministic window-id hash + connection helper `crates/platform/linux/src/atspi_conn.rs`**

```rust
//! AT-SPI connection wrapper + the async→sync bridge.
//!
//! The `WindowInspector` trait is synchronous and is called from the handler's
//! `spawn_blocking` pool. AT-SPI (via `atspi`/`zbus`) is async, so this helper
//! owns a private current-thread Tokio runtime and drives async calls with
//! `block_on`. That is safe here because trait methods never run on a runtime
//! worker thread.

/// Stable FNV-1a hash of an AT-SPI accessible's identity → a `u32` window id.
/// Deterministic within a process run, which is all `read_window_text` needs
/// (it re-enumerates and re-hashes in the same process to resolve an id).
pub fn window_id_hash(bus_name: &str, object_path: &str) -> u32 {
    const OFFSET: u32 = 2166136261;
    const PRIME: u32 = 16777619;
    let mut hash = OFFSET;
    for byte in bus_name.bytes().chain(std::iter::once(0)).chain(object_path.bytes()) {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic_and_distinguishes_paths() {
        let a = window_id_hash(":1.42", "/org/a11y/atspi/accessible/1");
        let b = window_id_hash(":1.42", "/org/a11y/atspi/accessible/1");
        let c = window_id_hash(":1.42", "/org/a11y/atspi/accessible/2");
        assert_eq!(a, b, "same identity must hash equal");
        assert_ne!(a, c, "different object paths must differ");
    }
}
```

- [ ] **Step 3: Run the hash unit test to verify it passes**

Run: `cargo test -p vantage-platform-linux window_id_hash 2>&1 | tail -20`
Expected: PASS — `hash_is_deterministic_and_distinguishes_paths`. (This unit test
compiles without a live session, proving the module wiring before the FFI spike.)

- [ ] **Step 4: Write the `#[ignore]` live test `crates/platform/linux/tests/windows_live.rs`**

```rust
//! Live AT-SPI tests: require a desktop session with the accessibility bus
//! enabled and at least one on-screen application window.
//! Run manually: `cargo test -p vantage-platform-linux --test windows_live -- --ignored`
#![cfg(target_os = "linux")]

use vantage_core::{WindowFilter, WindowInspector};
use vantage_platform_linux::LinuxWindowInspector;

#[test]
#[ignore = "requires live desktop session + AT-SPI accessibility bus"]
fn lists_at_least_one_window() {
    let inspector = LinuxWindowInspector::new();
    let windows = inspector
        .list_windows(WindowFilter { app_filter: None, on_screen_only: true })
        .expect("list_windows");
    assert!(!windows.is_empty(), "expected at least one on-screen window");
    assert!(windows.iter().any(|w| !w.app.is_empty()));
}
```

- [ ] **Step 5: Implement `list_windows` in `crates/platform/linux/src/windows.rs`**

Declare the helper module and hold the runtime + connection in the struct. The
DFS text walk lands in Task 6; this task implements enumeration only.

```rust
use std::sync::Mutex;

use vantage_core::{
    Bounds, CaptureError, WindowFilter, WindowId, WindowInfo, WindowInspector, WindowText,
};

use crate::atspi_conn::window_id_hash;

pub struct LinuxWindowInspector {
    // A private current-thread runtime drives the async atspi calls. Wrapped in a
    // Mutex so the &self trait methods can borrow it mutably for block_on.
    rt: Mutex<tokio::runtime::Runtime>,
}

impl LinuxWindowInspector {
    pub fn new() -> Self {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread runtime for AT-SPI");
        Self { rt: Mutex::new(rt) }
    }

    /// Enumerate on-screen application frames from the AT-SPI desktop tree.
    /// Returns (WindowInfo, bus_name, object_path) so `read_window_text` can
    /// re-resolve by the same hash without extra state.
    fn enumerate(&self) -> Result<Vec<(WindowInfo, String, String)>, CaptureError> {
        let rt = self.rt.lock().expect("runtime mutex");
        rt.block_on(async {
            // 1. atspi::connection::AccessibilityConnection::new().await — connect to
            //    the a11y bus. Map connection failure -> AccessibilityPermissionDenied.
            // 2. Get the registry root; iterate application accessibles.
            // 3. For each application, iterate its child frames (Role::Frame / Window).
            // 4. Per frame read: Name (title), parent application Name (app),
            //    Component.GetExtents(CoordType::Screen) -> Bounds,
            //    State set -> focused = Active, on_screen = Showing/Visible.
            // 5. window_id = window_id_hash(bus_name, object_path).
            // Return the (WindowInfo, bus_name, object_path) triples.
            Err::<Vec<(WindowInfo, String, String)>, CaptureError>(CaptureError::Internal(
                "atspi enumerate unimplemented".into(),
            ))
        })
    }
}

impl Default for LinuxWindowInspector {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowInspector for LinuxWindowInspector {
    fn list_windows(&self, filter: WindowFilter) -> Result<Vec<WindowInfo>, CaptureError> {
        let mut out: Vec<WindowInfo> = self
            .enumerate()?
            .into_iter()
            .map(|(info, _bus, _path)| info)
            .collect();
        if filter.on_screen_only {
            // enumerate() already restricts to showing/visible frames; kept for clarity.
        }
        if let Some(app) = filter.app_filter {
            out.retain(|w| w.app == app);
        }
        Ok(out)
    }

    fn read_window_text(
        &self,
        _window_id: WindowId,
        _depth: u32,
    ) -> Result<WindowText, CaptureError> {
        // Implemented in Task 6.
        Err(CaptureError::Unsupported("linux window text not yet implemented".into()))
    }
}

// Keep a compile-time reference to Bounds so the skeleton builds before the FFI
// is filled in; delete once enumerate() constructs real Bounds.
#[allow(dead_code)]
fn _bounds_ref() -> Bounds {
    Bounds { x: 0, y: 0, width: 0, height: 0 }
}
```

Add to `crates/platform/linux/src/lib.rs` (Linux-gated), before the capability modules:

```rust
#[cfg(target_os = "linux")]
mod atspi_conn;
```

Replace the `enumerate()` `block_on` body with the real AT-SPI calls (steps 1–5 in
the comment). Map a11y-bus connection failure to `CaptureError::AccessibilityPermissionDenied`.
Delete `_bounds_ref` once real `Bounds` are built.

*Spike fallback:* if the `atspi` high-level API is awkward, drive the AT-SPI D-Bus
interfaces (`org.a11y.atspi.Accessible`, `.Component`) via `zbus::Connection` proxies
inside the same `block_on`. The seam (`enumerate()` returning the triples) is unchanged.

- [ ] **Step 6: Build, then run the live test on the reference box**

Run: `cargo build -p vantage-platform-linux`
Expected: PASS (skeleton compiles; `enumerate` returns an error until the FFI is filled).

Run: `cargo test -p vantage-platform-linux --test windows_live -- --ignored`
Expected: PASS once the real AT-SPI enumeration is implemented — a non-empty list
with at least one non-empty `app`. (On GNOME/Wayland this reads native windows via
the a11y bus, which X11 EWMH could not.)

- [ ] **Step 7: Commit**

```bash
git add crates/platform/linux
git commit -m "feat(linux): WindowInspector list_windows via AT-SPI2 (+ id hash, runtime bridge)"
```

---

### Task 6: Linux `read_window_text` (AT-SPI subtree walk + resolve-by-id)

Complete the `WindowInspector` by walking the matched frame's accessible subtree,
honouring the depth and node-budget bounds and the `truncated` flag.

**Files:**
- Modify: `crates/platform/linux/src/windows.rs`
- Test: extend `crates/platform/linux/tests/windows_live.rs` (`#[ignore]`)

**Interfaces:**
- Consumes: the `enumerate()` triples + `window_id_hash` from Task 5.
- Produces: `LinuxWindowInspector::read_window_text(window_id, depth) -> Result<WindowText, CaptureError>`. Re-enumerates, hashes each frame, matches `window_id` (first match wins), DFS-collects text up to `depth` and a node budget (2000), sets `truncated` when a bound stops the walk. No match → `WindowNotFound`.

- [ ] **Step 1: Add the live text test (append to `windows_live.rs`)**

```rust
#[test]
#[ignore = "requires live desktop session + AT-SPI accessibility bus"]
fn reads_some_text_from_first_window() {
    let inspector = LinuxWindowInspector::new();
    let windows = inspector
        .list_windows(WindowFilter { app_filter: None, on_screen_only: true })
        .unwrap();
    let target = windows.first().expect("a window");
    let text = inspector
        .read_window_text(target.window_id, 20)
        .expect("read_window_text");
    // Content varies across apps; assert the call path returns the struct.
    let _ = text.truncated;
}
```

- [ ] **Step 2: Implement `read_window_text` in `crates/platform/linux/src/windows.rs`**

Replace the placeholder `read_window_text` body:

```rust
    fn read_window_text(
        &self,
        window_id: WindowId,
        depth: u32,
    ) -> Result<WindowText, CaptureError> {
        // Re-resolve the frame by re-enumerating and matching the same hash.
        let target = self
            .enumerate()?
            .into_iter()
            .find(|(info, _bus, _path)| info.window_id == window_id)
            .ok_or(CaptureError::WindowNotFound(window_id))?;
        let (_info, bus_name, object_path) = target;

        const NODE_BUDGET: usize = 2000;
        let rt = self.rt.lock().expect("runtime mutex");
        rt.block_on(async {
            // 1. Reconnect / reuse the a11y bus; build an Accessible proxy for
            //    (bus_name, object_path).
            // 2. Iterative DFS (stack) over children up to `depth` levels and
            //    NODE_BUDGET nodes total. For each node collect:
            //      - Text interface content if present, else Name/Description.
            // 3. truncated = (hit depth limit) || (hit NODE_BUDGET).
            // 4. Join collected strings with '\n'.
            let _ = (&bus_name, &object_path, depth, NODE_BUDGET);
            Err::<WindowText, CaptureError>(CaptureError::Internal(
                "atspi read_window_text unimplemented".into(),
            ))
        })
    }
```

Fill the `block_on` body with the real DFS against the AT-SPI accessible tree,
using the same connection approach chosen in Task 5. Set `truncated` per step 3.

- [ ] **Step 3: Build and run both window live tests**

Run: `cargo build -p vantage-platform-linux`
Expected: PASS.

Run: `cargo test -p vantage-platform-linux --test windows_live -- --ignored`
Expected: PASS — both `lists_at_least_one_window` and
`reads_some_text_from_first_window` succeed against the live session.

- [ ] **Step 4: Commit**

```bash
git add crates/platform/linux
git commit -m "feat(linux): read_window_text via AT-SPI subtree walk (depth + node budget)"
```

---

### Task 7: Platform-aware error remediation text

Give Linux users correct remediation guidance without changing the `CaptureError`
enum. Only the message strings in `error_map.rs` become platform-aware.

**Files:**
- Modify: `crates/mcp-server/src/error_map.rs`
- Test: inline `#[cfg(test)]` in `error_map.rs`

**Interfaces:**
- Consumes: `vantage_core::CaptureError`.
- Produces: `to_mcp_error` unchanged in signature and error *codes*; the two
  permission variants emit platform-appropriate remediation text via `cfg`.

- [ ] **Step 1: Add a Linux-specific test (append to the `tests` mod in `error_map.rs`)**

```rust
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_permission_text_is_platform_appropriate() {
        let ax = to_mcp_error(CaptureError::AccessibilityPermissionDenied);
        assert_eq!(ax.code, ErrorCode::INVALID_REQUEST);
        // Must not reference macOS System Settings on Linux.
        assert!(!ax.message.contains("System Settings"));
        assert!(ax.message.to_lowercase().contains("accessibility"));

        let sr = to_mcp_error(CaptureError::ScreenRecordingPermissionDenied);
        assert_eq!(sr.code, ErrorCode::INVALID_REQUEST);
        assert!(!sr.message.contains("System Settings"));
    }
```

- [ ] **Step 2: Make the two permission arms platform-aware in `to_mcp_error`**

Replace the two permission-denied arms (keep everything else identical):

```rust
        CaptureError::ScreenRecordingPermissionDenied => {
            #[cfg(target_os = "macos")]
            let msg = "Screen Recording permission not granted to this process. Grant it in System \
                       Settings > Privacy & Security > Screen Recording, then restart the agent.";
            #[cfg(target_os = "linux")]
            let msg = "Screen capture was denied. On Wayland, approve the screen-capture/screenshot \
                       portal prompt (xdg-desktop-portal) for this application; on X11 ensure the \
                       session allows capture, then retry.";
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            let msg = "Screen capture permission not granted to this process.";
            ErrorData::invalid_request(msg, None)
        }
        CaptureError::AccessibilityPermissionDenied => {
            #[cfg(target_os = "macos")]
            let msg = "Accessibility permission not granted to this process. Grant it in System \
                       Settings > Privacy & Security > Accessibility, then restart the agent.";
            #[cfg(target_os = "linux")]
            let msg = "The accessibility (AT-SPI) bus is unavailable. Enable assistive \
                       technologies / the accessibility bus for this session (e.g. set \
                       GTK_MODULES to load the a11y bridge or enable it in your desktop's \
                       accessibility settings), then retry.";
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            let msg = "Accessibility support is not available on this platform.";
            ErrorData::invalid_request(msg, None)
        }
```

- [ ] **Step 3: Run the error_map tests**

Run: `cargo test -p vantage-mcp-server error_map`
Expected: PASS — the existing macOS-agnostic assertions (`permission_denied_is_not_internal_and_is_actionable`
checks for "Screen Recording" + "grant"; note the Linux text still contains
neither of those exact tokens). 

**Compat note:** the existing test `permission_denied_is_not_internal_and_is_actionable`
asserts the message contains `"Screen Recording"` and `"grant"`. Those tokens are
macOS-specific. Gate that existing test with `#[cfg(target_os = "macos")]` so it
only runs on macOS, and rely on the new Linux test (Step 1) for Linux coverage.
Make this edit in Step 2's file as well.

- [ ] **Step 4: Commit**

```bash
git add crates/mcp-server/src/error_map.rs
git commit -m "feat(server): platform-aware permission remediation text (macOS + Linux)"
```

---

### Task 8: End-to-end verification, docs, and cross-platform build proof

Prove the whole Linux slice works together, refresh the docs for Linux, and
confirm the macOS build remains intact by construction.

**Files:**
- Modify: `README.md`
- Modify: `docs/agent-registration.md`
- Modify: `CLAUDE.md` (Linux build/test notes)

**Interfaces:**
- Consumes: everything from Tasks 1–7.
- Produces: a verified, documented Linux read slice.

- [ ] **Step 1: Full workspace build + test on Linux**

Run: `cargo build --workspace && cargo test --workspace`
Expected: PASS — all non-ignored unit + integration tests across `core`,
`mcp-server`, and both platform crates. (`#[ignore]` live tests excluded.)

- [ ] **Step 2: Run every Linux live test on the reference box**

Run:
```bash
cargo test -p vantage-platform-linux --test clipboard_live -- --ignored
cargo test -p vantage-platform-linux --test capture_live   -- --ignored
cargo test -p vantage-platform-linux --test ocr_live       -- --ignored
cargo test -p vantage-platform-linux --test windows_live   -- --ignored
```
Expected: all PASS on the GNOME/Wayland session (OCR after Task 4's install).
Record any that must be skipped (e.g. no a11y bus) explicitly rather than
silently — do not claim a pass that did not run.

- [ ] **Step 3: Manual smoke run of the binary on Linux**

Run:
```bash
cargo build --release
printf '%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"cli","version":"0"}}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | ./target/release/vantage-mcp
```
Expected: two JSON-RPC responses on stdout; `tools/list` lists exactly
`list_windows`, `read_window_text`, `capture_region`, `read_clipboard`. Logs on
stderr only, never mixed into stdout.

- [ ] **Step 4: Update `README.md` for Linux**

Under Prerequisites, add a Linux section: works on X11 and Wayland; requires a
desktop session with the AT-SPI accessibility bus enabled (for `read_window_text`
and window titles), screen-capture permission (Wayland: approve the
`xdg-desktop-portal` prompt), and `libtesseract` + `tesseract-ocr-eng` installed
(for OCR). Change the top-line "macOS only" framing to "macOS and Linux". Keep the
tool table and "stdout is sacred" section as-is. Move the implemented Linux item
out of the Roadmap into Features.

- [ ] **Step 5: Update `docs/agent-registration.md` for Linux**

Add the Linux permission-granting notes (AT-SPI bus, screen-capture portal) and a
note that the same stdio binary path is registered the same way; permission
failures surface as the actionable MCP errors from Task 7.

- [ ] **Step 6: Update `CLAUDE.md`**

Correct the build note: the workspace now builds on **both** macOS and Linux;
`vantage-mcp` selects its backend via `cfg(target_os)` and each platform crate
exposes a uniform `backends()`. Add the Linux live-test commands alongside the
macOS ones. Note the Linux backend map (AT-SPI / xcap / Tesseract / arboard).

- [ ] **Step 7: Commit**

```bash
git add README.md docs/agent-registration.md CLAUDE.md
git commit -m "docs: document Linux support (build, permissions, registration)"
```

---

## Self-Review

**Spec coverage** (spec §-by-§ → task):
- §1 objective / success criteria → cross-platform build (Task 1), AT-SPI text (5–6), capture both sessions (3), clipboard both sessions (2), actionable errors (7), stdout clean (Task 1 boot test), core frozen (all — no `crates/core` edits). ✅
- §2.2 binary selection → Task 1 (cfg deps + `backends()` on both crates + `compile_error!`). ✅
- §2.3 session detection → confined to capture; Task 3 reads `XDG_SESSION_TYPE` only in the conditional portal fallback. ✅
- §2.4 AT-SPI window/text → Task 5 (list + id hash + runtime bridge), Task 6 (text walk). ✅
- §2.5 xcap capture + portal fallback → Task 3 (primary + conditional Step 5). ✅
- §2.6 Tesseract OCR → Task 4. ✅
- §2.7 arboard clipboard → Task 2. ✅
- §2.8 error model (enum frozen, text platform-aware) → Task 7. ✅
- §4 testing (boot on Linux, four `#[ignore]` live tests, verification matrix) → Tasks 1–6 tests + Task 8 e2e. ✅
- §5 dependencies → added per-capability in Tasks 2–5. ✅
- §6 risks (AT-SPI spike, xcap Wayland, tesseract packaging, a11y bus) → Task 5 fallback note, Task 3 spike, Task 4 install step, Task 7 error text. ✅

**Placeholder scan:** The AT-SPI FFI bodies in Tasks 5–6 ship as skeletons with
explicit `unimplemented`-returning `block_on` blocks and numbered step comments
that the task's own steps replace with real calls, then verify via the `#[ignore]`
live tests. This matches the precedent set by the macOS plan (AX/Vision Tasks 8/10
were spikes with the same structure) and is required because the exact `atspi`
0.26 call surface must be resolved against the pinned version at implementation
time. No `TODO`/`TBD` in shipping steps; the conditional portal fallback (Task 3
Step 5) is explicitly gated on the spike outcome, not a hidden gap.

**Type consistency:** `backends()` has the identical 4-tuple signature in both
platform crates (Task 1 Steps 3–4) and is consumed once in `main.rs` (Step 7).
`window_id_hash(bus_name, object_path) -> u32` is defined in Task 5 Step 2 and
consumed in Tasks 5–6. `enumerate()` returns `Vec<(WindowInfo, String, String)>`
in Task 5 and is consumed identically in Task 6. The stub struct names
(`LinuxWindowInspector`, `LinuxScreenCapturer`, `LinuxTextRecognizer`,
`LinuxClipboard`) are stable from Task 1 onward; `backends()` never changes after
Task 1, only the module bodies do. `classify_capture_error` (Task 3) and the
`CaptureError` variants match the frozen core enum.
