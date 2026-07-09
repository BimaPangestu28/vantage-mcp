use crate::error::CaptureError;
use crate::types::{
    Bounds, ClipboardContent, ClipboardPrefer, DisplayInfo, MouseButton, RgbaImage, WindowFilter,
    WindowId, WindowInfo, WindowText,
};

pub trait WindowInspector: Send + Sync {
    fn list_windows(&self, filter: WindowFilter) -> Result<Vec<WindowInfo>, CaptureError>;
    fn read_window_text(&self, window_id: WindowId, depth: u32)
        -> Result<WindowText, CaptureError>;
}

pub trait ScreenCapturer: Send + Sync {
    fn capture_region(&self, bounds: Bounds) -> Result<RgbaImage, CaptureError>;
    /// Enumerate connected displays (monitors).
    fn list_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError>;
    /// Capture a single window, identified by an already-resolved `WindowInfo`
    /// (from `WindowInspector::list_windows`). Not supported on Wayland.
    fn capture_window(&self, target: &WindowInfo) -> Result<RgbaImage, CaptureError>;
}

pub trait TextRecognizer: Send + Sync {
    fn recognize(&self, image: &RgbaImage) -> Result<String, CaptureError>;
}

pub trait ClipboardAccess: Send + Sync {
    fn read(&self, prefer: ClipboardPrefer) -> Result<ClipboardContent, CaptureError>;
}

/// Write/act capability. Kept behind the server's default-off act gate.
pub trait InputController: Send + Sync {
    /// Write text and/or an image to the system clipboard.
    fn write_clipboard(
        &self,
        text: Option<&str>,
        image: Option<&RgbaImage>,
    ) -> Result<(), CaptureError>;
    fn type_text(&self, text: &str) -> Result<(), CaptureError>;
    fn click(
        &self,
        x: i32,
        y: i32,
        button: MouseButton,
        double: bool,
    ) -> Result<(), CaptureError>;
    fn focus_window(&self, target: &WindowInfo) -> Result<(), CaptureError>;
    fn move_mouse(&self, x: i32, y: i32) -> Result<(), CaptureError>;
    /// Press a modifier+key combo, e.g. "ctrl+shift+t".
    fn key_press(&self, keys: &str) -> Result<(), CaptureError>;
}
