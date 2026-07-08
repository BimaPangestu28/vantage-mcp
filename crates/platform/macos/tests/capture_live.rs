//! Live test: requires Screen Recording permission and a connected display.
//! Run manually: `cargo test -p vantage-platform-macos --test capture_live -- --ignored`

#![cfg(target_os = "macos")]

use vantage_core::{Bounds, ScreenCapturer};
use vantage_platform_macos::MacScreenCapturer;

#[test]
#[ignore = "requires Screen Recording permission + a display"]
fn captures_a_small_region() {
    let capturer = MacScreenCapturer::new();
    let img = capturer
        .capture_region(Bounds {
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        })
        .expect("capture");
    // On a HiDPI/Retina display the returned image is at native pixel
    // resolution, i.e. `scale_factor()`x the requested 64x64-point region
    // (e.g. 128x128 at 2x) -- that's correct and desirable (higher
    // resolution for OCR), so we only assert it's at least as large as the
    // requested region and internally consistent, rather than pinning an
    // exact size.
    assert!(img.width >= 64, "expected width >= 64, got {}", img.width);
    assert!(
        img.height >= 64,
        "expected height >= 64, got {}",
        img.height
    );
    assert_eq!(img.pixels.len(), (img.width * img.height * 4) as usize);
}
