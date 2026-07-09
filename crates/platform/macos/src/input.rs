use vantage_core::{CaptureError, InputController, MouseButton, WindowInfo};

pub struct MacInputController;

impl MacInputController {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacInputController {
    fn default() -> Self {
        Self::new()
    }
}

impl InputController for MacInputController {
    fn write_clipboard(&self, _text: &str) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported(
            "macos clipboard_write not yet implemented".into(),
        ))
    }
    fn type_text(&self, _text: &str) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported(
            "macos type_text not yet implemented".into(),
        ))
    }
    fn click(&self, _x: i32, _y: i32, _button: MouseButton) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported(
            "macos click not yet implemented".into(),
        ))
    }
    fn focus_window(&self, _target: &WindowInfo) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported(
            "macos focus_window not yet implemented".into(),
        ))
    }
}
