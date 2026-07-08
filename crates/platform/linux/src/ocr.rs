//! Linux `TextRecognizer` backed by Tesseract (libtesseract/leptonica).
//!
//! Compiled only when the `ocr` feature is enabled (it needs the tesseract
//! system libraries at build time); the `ocr_stub` module stands in otherwise.

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
    fn recognize(&self, image: &RgbaImage) -> Result<String, CaptureError> {
        // Build an English recognizer, feed the raw RGBA frame (4 bytes/pixel,
        // stride = width*4), then read the recognized text. The `tesseract`
        // 0.15 builder methods consume and return `self`.
        let api = tesseract::Tesseract::new(None, Some("eng")).map_err(|e| {
            CaptureError::Internal(format!(
                "Tesseract init failed ({e}). Install libtesseract + the 'eng' \
                 traineddata (e.g. apt install libtesseract-dev tesseract-ocr-eng)."
            ))
        })?;
        let bytes_per_pixel: i32 = 4;
        let bytes_per_line = image.width as i32 * bytes_per_pixel;
        let text = api
            .set_frame(
                &image.pixels,
                image.width as i32,
                image.height as i32,
                bytes_per_pixel,
                bytes_per_line,
            )
            .and_then(|mut api| api.get_text())
            .map_err(|e| CaptureError::Internal(format!("tesseract recognize: {e}")))?;
        Ok(text)
    }
}
