//! Linux backend implementations of vantage-core capability traits.
//!
//! Everything OS-specific is gated behind `#[cfg(target_os = "linux")]` so this
//! crate compiles to an empty lib on non-Linux hosts (mirrors the macOS crate).
#[cfg(target_os = "linux")]
use std::sync::Arc;

#[cfg(target_os = "linux")]
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
