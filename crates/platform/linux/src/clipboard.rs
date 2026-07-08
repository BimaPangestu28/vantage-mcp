use vantage_core::{
    CaptureError, ClipboardAccess, ClipboardContent, ClipboardKind, ClipboardPrefer, RgbaImage,
};

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
    fn read(&self, prefer: ClipboardPrefer) -> Result<ClipboardContent, CaptureError> {
        let mut board = arboard::Clipboard::new()
            .map_err(|e| CaptureError::Internal(format!("clipboard open: {e}")))?;

        let get_text = |b: &mut arboard::Clipboard| b.get_text().ok();
        let get_image = |b: &mut arboard::Clipboard| {
            b.get_image().ok().map(|img| RgbaImage {
                width: img.width as u32,
                height: img.height as u32,
                pixels: img.bytes.into_owned(),
            })
        };

        let (text, image) = match prefer {
            ClipboardPrefer::Text => {
                let t = get_text(&mut board);
                let i = if t.is_none() { get_image(&mut board) } else { None };
                (t, i)
            }
            ClipboardPrefer::Image => {
                let i = get_image(&mut board);
                let t = if i.is_none() { get_text(&mut board) } else { None };
                (t, i)
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
