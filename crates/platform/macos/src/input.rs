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
    fn write_clipboard(&self, text: &str) -> Result<(), CaptureError> {
        let mut board = arboard::Clipboard::new()
            .map_err(|e| CaptureError::Internal(format!("clipboard open: {e}")))?;
        board
            .set_text(text.to_owned())
            .map_err(|e| CaptureError::Internal(format!("clipboard set_text: {e}")))
    }
    fn type_text(&self, text: &str) -> Result<(), CaptureError> {
        use enigo::{Enigo, Keyboard, Settings};
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| CaptureError::Internal(format!("input init: {e}")))?;
        enigo
            .text(text)
            .map_err(|e| CaptureError::Internal(format!("input type: {e}")))
    }

    fn click(&self, x: i32, y: i32, button: MouseButton) -> Result<(), CaptureError> {
        use enigo::{Button, Coordinate, Direction, Enigo, Mouse, Settings};
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| CaptureError::Internal(format!("input init: {e}")))?;
        enigo
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(|e| CaptureError::Internal(format!("input move: {e}")))?;
        let btn = match button {
            MouseButton::Left => Button::Left,
            MouseButton::Right => Button::Right,
            MouseButton::Middle => Button::Middle,
        };
        enigo
            .button(btn, Direction::Click)
            .map_err(|e| CaptureError::Internal(format!("input click: {e}")))
    }
    fn focus_window(&self, target: &WindowInfo) -> Result<(), CaptureError> {
        // Resolve the window's owning app and activate it (brings the app and
        // its windows to the front). Coarser than raising the exact window, but
        // reliable and needs no AX action FFI.
        use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
        let pid = crate::windows::resolve_window_pid(target.window_id)?;
        match NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
            Some(app) => {
                app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
                Ok(())
            }
            None => Err(CaptureError::WindowNotFound(target.window_id)),
        }
    }
}
