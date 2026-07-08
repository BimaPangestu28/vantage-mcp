//! Live OCR test. Requires the crate built with the `ocr` feature (default) and
//! libtesseract + the `eng` traineddata installed on the host.
//! Run manually: `cargo test -p vantage-platform-linux --test ocr_live -- --ignored`
#![cfg(all(target_os = "linux", feature = "ocr"))]

use vantage_core::{RgbaImage, TextRecognizer};
use vantage_platform_linux::LinuxTextRecognizer;

#[test]
#[ignore = "requires libtesseract + eng traineddata"]
fn recognizes_rendered_text() {
    let bytes = include_bytes!("fixtures/hello.png");
    let decoded = image::load_from_memory(bytes)
        .expect("fixture hello.png should decode")
        .to_rgba8();
    let (width, height) = (decoded.width(), decoded.height());
    let img = RgbaImage {
        width,
        height,
        pixels: decoded.into_raw(),
    };

    let ocr = LinuxTextRecognizer::new();
    let text = ocr.recognize(&img).expect("ocr");
    assert!(
        text.to_uppercase().contains("HELLO"),
        "expected HELLO in OCR output, got: {text:?}"
    );
}
