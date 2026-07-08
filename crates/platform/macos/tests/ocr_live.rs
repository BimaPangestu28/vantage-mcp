//! Live OCR test for the macOS Vision `TextRecognizer` backend.
//!
//! Vision text recognition requires no TCC permission and runs in-process, but
//! it still depends on the macOS Vision framework being present at runtime, so
//! the test is `#[ignore]`d by default. Run it explicitly with:
//!
//! ```sh
//! cargo test -p vantage-platform-macos --test ocr_live -- --ignored
//! ```

#![cfg(target_os = "macos")]

use vantage_core::{RgbaImage, TextRecognizer};
use vantage_platform_macos::MacTextRecognizer;

/// Loads a committed high-contrast "HELLO" fixture and asserts Vision OCR
/// recovers the word.
#[test]
#[ignore = "requires macOS Vision framework at runtime"]
fn recognizes_rendered_text() {
    let fixture_bytes = include_bytes!("fixtures/hello.png");
    let decoded = image::load_from_memory(fixture_bytes)
        .expect("fixture hello.png should decode")
        .to_rgba8();
    let (width, height) = (decoded.width(), decoded.height());
    let source_image = RgbaImage {
        width,
        height,
        pixels: decoded.into_raw(),
    };

    let recognizer = MacTextRecognizer::new();
    let recognized_text = recognizer
        .recognize(&source_image)
        .expect("ocr should succeed");

    assert!(
        recognized_text.to_uppercase().contains("HELLO"),
        "expected HELLO in OCR output, got: {recognized_text:?}"
    );
}
