use std::sync::Mutex;

use enigo::Key;
use vantage_core::{CaptureError, InputController, MouseButton, RgbaImage, WindowInfo};

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
    fn write_clipboard(
        &self,
        text: Option<&str>,
        image: Option<&RgbaImage>,
    ) -> Result<(), CaptureError> {
        // On Linux (X11 and Wayland) the clipboard offer is withdrawn when the
        // `arboard::Clipboard` instance drops, so serve it from a detached thread
        // via `set().wait()` (the wl-copy mechanism): the offer persists after we
        // return, until another client replaces the selection.
        use arboard::SetExtLinux;
        enum Payload {
            Text(String),
            Image(u32, u32, Vec<u8>),
        }
        let payload = match (text, image) {
            (Some(t), _) => Payload::Text(t.to_owned()),
            (None, Some(img)) => Payload::Image(img.width, img.height, img.pixels.clone()),
            (None, None) => {
                return Err(CaptureError::Internal(
                    "write_clipboard needs text or an image".into(),
                ))
            }
        };
        let (tx, rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);
        std::thread::Builder::new()
            .name("vantage-clipboard".into())
            .spawn(move || match arboard::Clipboard::new() {
                Ok(mut board) => {
                    let _ = tx.send(Ok(()));
                    let _ = match payload {
                        Payload::Text(t) => board.set().wait().text(t),
                        Payload::Image(w, h, bytes) => {
                            board.set().wait().image(arboard::ImageData {
                                width: w as usize,
                                height: h as usize,
                                bytes: std::borrow::Cow::Owned(bytes),
                            })
                        }
                    };
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                }
            })
            .map_err(|e| CaptureError::Internal(format!("spawn clipboard thread: {e}")))?;
        match rx.recv() {
            Ok(Ok(())) => {
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

    fn click(
        &self,
        x: i32,
        y: i32,
        button: MouseButton,
        double: bool,
    ) -> Result<(), CaptureError> {
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
            .map_err(|e| classify_input_error(&e.to_string()))?;
        if double {
            enigo
                .button(btn, Direction::Click)
                .map_err(|e| classify_input_error(&e.to_string()))?;
        }
        Ok(())
    }

    fn focus_window(&self, target: &WindowInfo) -> Result<(), CaptureError> {
        let rt = self.rt.lock().expect("runtime mutex");
        rt.block_on(async {
            let conn = connect().await?;
            grab_focus_by_id(conn.connection(), target.window_id).await
        })
    }

    fn move_mouse(&self, x: i32, y: i32) -> Result<(), CaptureError> {
        use enigo::{Coordinate, Enigo, Mouse, Settings};
        let mut enigo =
            Enigo::new(&Settings::default()).map_err(|e| classify_input_error(&e.to_string()))?;
        enigo
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(|e| classify_input_error(&e.to_string()))
    }

    fn key_press(&self, keys: &str) -> Result<(), CaptureError> {
        use enigo::{Direction, Enigo, Keyboard, Settings};
        let (mods, main) = parse_combo(keys)?;
        let mut enigo =
            Enigo::new(&Settings::default()).map_err(|e| classify_input_error(&e.to_string()))?;
        for m in &mods {
            enigo
                .key(*m, Direction::Press)
                .map_err(|e| classify_input_error(&e.to_string()))?;
        }
        let pressed = enigo
            .key(main, Direction::Click)
            .map_err(|e| classify_input_error(&e.to_string()));
        // Always release held modifiers, even if the main key failed.
        for m in mods.iter().rev() {
            let _ = enigo.key(*m, Direction::Release);
        }
        pressed
    }
}

/// Parse a combo like "ctrl+shift+t" into (held modifiers, final key). Pure and
/// unit-tested. Unknown tokens yield an error mapped to `invalid_params`.
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

/// Map a synthetic-input failure to an actionable error. On Wayland, native
/// input injection is compositor-restricted (enigo's default x11rb path only
/// reaches X11/XWayland); surface that as `Unsupported`.
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

#[cfg(test)]
mod combo_tests {
    use super::*;

    #[test]
    fn parses_modifiers_and_named_keys() {
        let (m, k) = parse_combo("ctrl+shift+t").unwrap();
        assert_eq!(m, vec![Key::Control, Key::Shift]);
        assert_eq!(k, Key::Unicode('t'));
        assert_eq!(parse_combo("cmd+c").unwrap().0, vec![Key::Meta]);
        assert_eq!(parse_combo("enter").unwrap().1, Key::Return);
        assert_eq!(parse_combo("f5").unwrap().1, Key::F5);
        assert_eq!(parse_combo("  alt + Tab ").unwrap(), (vec![Key::Alt], Key::Tab));
    }

    #[test]
    fn rejects_unknown_tokens_and_empty() {
        assert!(parse_combo("ctrl+nope").is_err());
        assert!(parse_combo("").is_err());
        assert!(parse_combo("notakey").is_err());
    }
}
