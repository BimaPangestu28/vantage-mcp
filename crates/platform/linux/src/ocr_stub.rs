//! Stub `TextRecognizer` used when the `ocr` feature is disabled (i.e. the crate
//! was built without libtesseract). Returns an actionable `Unsupported` error.
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
            "OCR was disabled at build time (the `ocr` feature / libtesseract were \
             unavailable)"
                .into(),
        ))
    }
}
