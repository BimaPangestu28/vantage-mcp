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
        Err(CaptureError::Unsupported(
            "linux capture not yet implemented".into(),
        ))
    }
}
