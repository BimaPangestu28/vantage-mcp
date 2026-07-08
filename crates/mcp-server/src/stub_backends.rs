//! TEMPORARY — replaced by real macOS backends in Task 12.
//!
//! `vantage-platform-macos` does not yet implement the `vantage-core`
//! capability traits (that work lands in Tasks 8-11). These zero-sized
//! stand-ins let `vantage-mcp-server` construct a full `Vantage` handler and
//! boot over stdio *today*, proving the stdio/stderr/error-mapping
//! foundation works before the real capture backends exist. Every method
//! returns `CaptureError::Unsupported` since none of them can do real work
//! yet. Task 12 swaps these constructions in `main.rs` for the real
//! `vantage_platform_macos::Mac*` backends and deletes this module.

use vantage_core::{
    Bounds, CaptureError, ClipboardAccess, ClipboardContent, ClipboardPrefer, RgbaImage,
    ScreenCapturer, TextRecognizer, WindowFilter, WindowId, WindowInfo, WindowInspector, WindowText,
};

const NOT_IMPLEMENTED: &str = "not implemented until Task 12";

pub struct StubWindowInspector;

impl WindowInspector for StubWindowInspector {
    fn list_windows(&self, _filter: WindowFilter) -> Result<Vec<WindowInfo>, CaptureError> {
        Err(CaptureError::Unsupported(NOT_IMPLEMENTED.into()))
    }

    fn read_window_text(
        &self,
        _window_id: WindowId,
        _depth: u32,
    ) -> Result<WindowText, CaptureError> {
        Err(CaptureError::Unsupported(NOT_IMPLEMENTED.into()))
    }
}

pub struct StubScreenCapturer;

impl ScreenCapturer for StubScreenCapturer {
    fn capture_region(&self, _bounds: Bounds) -> Result<RgbaImage, CaptureError> {
        Err(CaptureError::Unsupported(NOT_IMPLEMENTED.into()))
    }
}

pub struct StubTextRecognizer;

impl TextRecognizer for StubTextRecognizer {
    fn recognize(&self, _image: &RgbaImage) -> Result<String, CaptureError> {
        Err(CaptureError::Unsupported(NOT_IMPLEMENTED.into()))
    }
}

pub struct StubClipboard;

impl ClipboardAccess for StubClipboard {
    fn read(&self, _prefer: ClipboardPrefer) -> Result<ClipboardContent, CaptureError> {
        Err(CaptureError::Unsupported(NOT_IMPLEMENTED.into()))
    }
}
