use std::sync::Mutex;

use vantage_core::{CaptureError, InputController, MouseButton, WindowInfo};

use crate::windows::{connect, grab_focus_by_id};

pub struct LinuxInputController {
    // Private current-thread runtime for the async AT-SPI `focus_window` call,
    // mirroring `LinuxWindowInspector`.
    rt: Mutex<tokio::runtime::Runtime>,
}

impl LinuxInputController {
    pub fn new() -> Self {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread runtime for AT-SPI input");
        Self { rt: Mutex::new(rt) }
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
    fn type_text(&self, text: &str) -> Result<(), CaptureError> {
        use enigo::{Enigo, Keyboard, Settings};
        let mut enigo =
            Enigo::new(&Settings::default()).map_err(|e| classify_input_error(&e.to_string()))?;
        enigo
            .text(text)
            .map_err(|e| classify_input_error(&e.to_string()))
    }

    fn click(&self, x: i32, y: i32, button: MouseButton) -> Result<(), CaptureError> {
        use enigo::{Button, Coordinate, Direction, Enigo, Mouse, Settings};
        let mut enigo =
            Enigo::new(&Settings::default()).map_err(|e| classify_input_error(&e.to_string()))?;
        enigo
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(|e| classify_input_error(&e.to_string()))?;
        let btn = match button {
            MouseButton::Left => Button::Left,
            MouseButton::Right => Button::Right,
            MouseButton::Middle => Button::Middle,
        };
        enigo
            .button(btn, Direction::Click)
            .map_err(|e| classify_input_error(&e.to_string()))
    }
    fn focus_window(&self, target: &WindowInfo) -> Result<(), CaptureError> {
        let rt = self.rt.lock().expect("runtime mutex");
        rt.block_on(async {
            let conn = connect().await?;
            grab_focus_by_id(conn.connection(), target.window_id).await
        })
    }
}

/// Map a synthetic-input failure to an actionable error. On Wayland, native
/// input injection is compositor-restricted (enigo's default x11rb path only
/// reaches X11/XWayland); surface that as `Unsupported` rather than a generic
/// failure.
fn classify_input_error(msg: &str) -> CaptureError {
    let m = msg.to_lowercase();
    if m.contains("wayland") || m.contains("libei") || m.contains("portal") || m.contains("display")
    {
        CaptureError::Unsupported(format!(
            "synthetic input was refused ({msg}). On Wayland, input injection needs \
             compositor/portal support (limited on GNOME); it works under X11/XWayland."
        ))
    } else {
        CaptureError::Internal(format!("input: {msg}"))
    }
}
