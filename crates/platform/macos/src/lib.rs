//! macOS backend implementations of vantage-core capability traits.
#[cfg(target_os = "macos")]
use std::sync::Arc;

#[cfg(target_os = "macos")]
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
