use vantage_core::{CaptureError, InputController, MouseButton, WindowInfo};

pub struct LinuxInputController;

impl LinuxInputController {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxInputController {
    fn default() -> Self {
        Self::new()
    }
}

impl InputController for LinuxInputController {
    fn write_clipboard(&self, _text: &str) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported(
            "linux clipboard_write not yet implemented".into(),
        ))
    }
    fn type_text(&self, _text: &str) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported(
            "linux type_text not yet implemented".into(),
        ))
    }
    fn click(&self, _x: i32, _y: i32, _button: MouseButton) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported(
            "linux click not yet implemented".into(),
        ))
    }
    fn focus_window(&self, _target: &WindowInfo) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported(
            "linux focus_window not yet implemented".into(),
        ))
    }
}
