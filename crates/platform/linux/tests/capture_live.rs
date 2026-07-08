//! Live capture test. Needs a desktop session; on Wayland the compositor may
//! prompt for screen-capture permission the first time.
//! Run manually: `cargo test -p vantage-platform-linux --test capture_live -- --ignored`
#![cfg(target_os = "linux")]

use vantage_core::{Bounds, ScreenCapturer};
use vantage_platform_linux::LinuxScreenCapturer;

#[test]
#[ignore = "requires a desktop session + screen-capture permission"]
fn captures_a_small_region() {
    let capturer = LinuxScreenCapturer::new();
    let img = capturer
        .capture_region(Bounds {
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        })
        .expect("capture");
    assert_eq!(img.width, 64);
    assert_eq!(img.height, 64);
    assert_eq!(img.pixels.len(), 64 * 64 * 4);
}
