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
    fn write_clipboard(&self, text: &str) -> Result<(), CaptureError> {
        // On Linux (X11 and Wayland) the clipboard offer is withdrawn when the
        // `arboard::Clipboard` instance drops — so a plain set_text from this
        // short-lived call would leave nothing to paste. Serve the offer from a
        // detached thread via `set().wait().text()` (the same mechanism wl-copy
        // uses): it registers the offer, then blocks serving it until another
        // client replaces the selection, so the text persists after we return.
        use arboard::SetExtLinux;
        let text = text.to_owned();
        let (tx, rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);
        std::thread::Builder::new()
            .name("vantage-clipboard".into())
            .spawn(move || match arboard::Clipboard::new() {
                Ok(mut board) => {
                    let _ = tx.send(Ok(()));
                    // Blocks until the selection is replaced; keeps the offer alive.
                    let _ = board.set().wait().text(text);
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                }
            })
            .map_err(|e| CaptureError::Internal(format!("spawn clipboard thread: {e}")))?;
        match rx.recv() {
            Ok(Ok(())) => {
                // Let the offer register on the display server before returning
                // so an immediate read observes it.
                std::thread::sleep(std::time::Duration::from_millis(60));
                Ok(())
            }
            Ok(Err(e)) => Err(CaptureError::Internal(format!("clipboard open: {e}"))),
            Err(e) => Err(CaptureError::Internal(format!("clipboard thread: {e}"))),
        }
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
