//! macOS `ClipboardAccess` implementation backed by `arboard`.
//!
//! Clipboard access requires no TCC permission on macOS, unlike screen
//! capture or accessibility. Reading an unavailable format (e.g. asking for
//! image data when the clipboard holds only text) is treated as absence,
//! not an error — only a failure to open the clipboard itself is surfaced
//! as `CaptureError::Internal`.

use vantage_core::{
    CaptureError, ClipboardAccess, ClipboardContent, ClipboardKind, ClipboardPrefer, RgbaImage,
};

pub struct MacClipboard;

impl MacClipboard {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacClipboard {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads clipboard text, treating "format not present" as absence rather
/// than an error.
fn read_text(board: &mut arboard::Clipboard) -> Option<String> {
    board.get_text().ok()
}

/// Reads clipboard image data and converts it to `vantage_core::RgbaImage`,
/// treating "format not present" as absence rather than an error.
fn read_image(board: &mut arboard::Clipboard) -> Option<RgbaImage> {
    let image = board.get_image().ok()?;
    Some(RgbaImage {
        width: image.width as u32,
        height: image.height as u32,
        pixels: image.bytes.into_owned(),
    })
}

impl ClipboardAccess for MacClipboard {
    fn read(&self, prefer: ClipboardPrefer) -> Result<ClipboardContent, CaptureError> {
        let mut board = arboard::Clipboard::new()
            .map_err(|error| CaptureError::Internal(format!("clipboard open: {error}")))?;

        let (text, image) = match prefer {
            ClipboardPrefer::Text => {
                let text = read_text(&mut board);
                let image = if text.is_none() {
                    read_image(&mut board)
                } else {
                    None
                };
                (text, image)
            }
            ClipboardPrefer::Image => {
                let image = read_image(&mut board);
                let text = if image.is_none() {
                    read_text(&mut board)
                } else {
                    None
                };
                (text, image)
            }
        };

        let kind = if text.is_some() {
            ClipboardKind::Text
        } else if image.is_some() {
            ClipboardKind::Image
        } else {
            ClipboardKind::Empty
        };

        Ok(ClipboardContent { kind, text, image })
    }
}
