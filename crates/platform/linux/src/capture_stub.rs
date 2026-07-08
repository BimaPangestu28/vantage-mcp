//! Stub `ScreenCapturer` used when the `capture` feature is disabled (i.e. the
//! crate was built without the xcap system libraries). Returns an actionable
//! `Unsupported` error rather than silently producing empty captures.
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
            "screen capture was disabled at build time (the `capture` feature / xcap \
             system libraries were unavailable)"
                .into(),
        ))
    }
}
