use enigo::Key;
use vantage_core::{CaptureError, InputController, MouseButton, RgbaImage, WindowInfo};

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
    fn write_clipboard(
        &self,
        text: Option<&str>,
        image: Option<&RgbaImage>,
    ) -> Result<(), CaptureError> {
        // macOS `Clipboard` is Send and NSPasteboard is system-owned, so a plain
        // set persists without a serving thread.
        let mut board = arboard::Clipboard::new()
            .map_err(|e| CaptureError::Internal(format!("clipboard open: {e}")))?;
        match (text, image) {
            (Some(t), _) => board
                .set_text(t.to_owned())
                .map_err(|e| CaptureError::Internal(format!("clipboard set_text: {e}"))),
            (None, Some(img)) => board
                .set_image(arboard::ImageData {
                    width: img.width as usize,
                    height: img.height as usize,
                    bytes: std::borrow::Cow::Borrowed(&img.pixels),
                })
                .map_err(|e| CaptureError::Internal(format!("clipboard set_image: {e}"))),
            (None, None) => Err(CaptureError::Internal(
                "write_clipboard needs text or an image".into(),
            )),
        }
    }

    fn type_text(&self, text: &str) -> Result<(), CaptureError> {
        use enigo::{Enigo, Keyboard, Settings};
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| CaptureError::Internal(format!("input init: {e}")))?;
        enigo
            .text(text)
            .map_err(|e| CaptureError::Internal(format!("input type: {e}")))
    }

    fn click(&self, x: i32, y: i32, button: MouseButton, double: bool) -> Result<(), CaptureError> {
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
            .map_err(|e| CaptureError::Internal(format!("input click: {e}")))?;
        if double {
            enigo
                .button(btn, Direction::Click)
                .map_err(|e| CaptureError::Internal(format!("input click: {e}")))?;
        }
        Ok(())
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

    fn move_mouse(&self, x: i32, y: i32) -> Result<(), CaptureError> {
        use enigo::{Coordinate, Enigo, Mouse, Settings};
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| CaptureError::Internal(format!("input init: {e}")))?;
        enigo
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(|e| CaptureError::Internal(format!("input move: {e}")))
    }

    fn key_press(&self, keys: &str) -> Result<(), CaptureError> {
        use enigo::{Direction, Enigo, Keyboard, Settings};
        let (mods, main) = parse_combo(keys)?;
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| CaptureError::Internal(format!("input init: {e}")))?;
        for m in &mods {
            enigo
                .key(*m, Direction::Press)
                .map_err(|e| CaptureError::Internal(format!("input key: {e}")))?;
        }
        let pressed = enigo
            .key(main, Direction::Click)
            .map_err(|e| CaptureError::Internal(format!("input key: {e}")));
        for m in mods.iter().rev() {
            let _ = enigo.key(*m, Direction::Release);
        }
        pressed
    }
}

/// Parse a combo like "ctrl+shift+t" into (held modifiers, final key).
fn parse_combo(keys: &str) -> Result<(Vec<Key>, Key), CaptureError> {
    let tokens: Vec<&str> = keys
        .split('+')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    let (last, rest) = tokens
        .split_last()
        .ok_or_else(|| CaptureError::Internal("empty key combo".into()))?;
    let mut mods = Vec::with_capacity(rest.len());
    for tok in rest {
        mods.push(
            modifier_key(tok)
                .ok_or_else(|| CaptureError::Internal(format!("unknown modifier: {tok:?}")))?,
        );
    }
    Ok((mods, named_or_char_key(last)?))
}

fn modifier_key(tok: &str) -> Option<Key> {
    match tok.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(Key::Control),
        "alt" | "option" => Some(Key::Alt),
        "shift" => Some(Key::Shift),
        "meta" | "cmd" | "command" | "super" | "win" => Some(Key::Meta),
        _ => None,
    }
}

fn named_or_char_key(tok: &str) -> Result<Key, CaptureError> {
    let key = match tok.to_ascii_lowercase().as_str() {
        "enter" | "return" => Key::Return,
        "tab" => Key::Tab,
        "esc" | "escape" => Key::Escape,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        _ => {
            let mut chars = tok.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Unicode(c),
                _ => return Err(CaptureError::Internal(format!("unknown key: {tok:?}"))),
            }
        }
    };
    Ok(key)
}
