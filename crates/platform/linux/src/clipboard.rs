use vantage_core::{CaptureError, ClipboardAccess, ClipboardContent, ClipboardPrefer};

pub struct LinuxClipboard;

impl LinuxClipboard {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardAccess for LinuxClipboard {
    fn read(&self, _prefer: ClipboardPrefer) -> Result<ClipboardContent, CaptureError> {
        Err(CaptureError::Unsupported(
            "linux clipboard not yet implemented".into(),
        ))
    }
}
