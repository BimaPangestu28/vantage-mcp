use vantage_core::{CaptureError, RgbaImage, TextRecognizer};

pub struct LinuxTextRecognizer;

impl LinuxTextRecognizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxTextRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRecognizer for LinuxTextRecognizer {
    fn recognize(&self, _image: &RgbaImage) -> Result<String, CaptureError> {
        Err(CaptureError::Unsupported(
            "linux ocr not yet implemented".into(),
        ))
    }
}
