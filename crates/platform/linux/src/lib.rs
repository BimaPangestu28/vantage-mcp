//! Linux backend implementations of vantage-core capability traits.
//!
//! Everything OS-specific is gated behind `#[cfg(target_os = "linux")]` so this
//! crate compiles to an empty lib on non-Linux hosts (mirrors the macOS crate).
//!
//! `capture` (xcap) and `ocr` (tesseract) are optional features (both ON by
//! default) because they need system libraries at build time. When a feature is
//! off, a stub backend is used that returns an actionable `Unsupported` error.
#[cfg(target_os = "linux")]
use std::sync::Arc;

#[cfg(target_os = "linux")]
use vantage_core::{ClipboardAccess, ScreenCapturer, TextRecognizer, WindowInspector};

#[cfg(target_os = "linux")]
mod atspi_conn;
#[cfg(target_os = "linux")]
mod clipboard;
#[cfg(target_os = "linux")]
mod windows;

// Capture: real impl when `capture` is enabled, stub otherwise.
#[cfg(all(target_os = "linux", feature = "capture"))]
mod capture;
#[cfg(all(target_os = "linux", not(feature = "capture")))]
#[path = "capture_stub.rs"]
mod capture;

// OCR: real impl when `ocr` is enabled, stub otherwise.
#[cfg(all(target_os = "linux", feature = "ocr"))]
mod ocr;
#[cfg(all(target_os = "linux", not(feature = "ocr")))]
#[path = "ocr_stub.rs"]
mod ocr;

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
#[allow(clippy::type_complexity)] // the 4-tuple is the deliberate backend-set seam
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
