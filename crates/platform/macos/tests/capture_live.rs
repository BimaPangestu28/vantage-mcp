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
        .capture_region(Bounds { x: 0, y: 0, width: 64, height: 64 })
        .expect("capture");
    assert_eq!(img.width, 64);
    assert_eq!(img.height, 64);
    assert_eq!(img.pixels.len(), 64 * 64 * 4);
}
