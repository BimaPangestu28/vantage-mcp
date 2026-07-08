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
