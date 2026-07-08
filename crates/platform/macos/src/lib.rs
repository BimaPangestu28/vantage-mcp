//! macOS backend implementations of vantage-core capability traits.
#[cfg(target_os = "macos")]
mod capture;
#[cfg(target_os = "macos")]
mod ocr;
#[cfg(target_os = "macos")]
mod windows;
#[cfg(target_os = "macos")]
pub use capture::MacScreenCapturer;
#[cfg(target_os = "macos")]
pub use ocr::MacTextRecognizer;
#[cfg(target_os = "macos")]
pub use windows::MacWindowInspector;
