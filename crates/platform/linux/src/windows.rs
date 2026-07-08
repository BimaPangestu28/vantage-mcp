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
        Err(CaptureError::Unsupported(
            "linux window inspection not yet implemented".into(),
        ))
    }

    fn read_window_text(
        &self,
        _window_id: WindowId,
        _depth: u32,
    ) -> Result<WindowText, CaptureError> {
        Err(CaptureError::Unsupported(
            "linux window text not yet implemented".into(),
        ))
    }
}
