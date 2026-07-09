//! Live display-enumeration test. Needs a desktop session with >= 1 monitor.
//! Run: `cargo test -p vantage-platform-linux --test displays_live -- --ignored`
#![cfg(all(target_os = "linux", feature = "capture"))]

use vantage_core::ScreenCapturer;
use vantage_platform_linux::LinuxScreenCapturer;

#[test]
#[ignore = "requires a desktop session with a display"]
fn lists_at_least_one_display() {
    let cap = LinuxScreenCapturer::new();
    let displays = cap.list_displays().expect("list_displays");
    assert!(!displays.is_empty(), "expected at least one display");
    assert!(displays
        .iter()
        .any(|d| d.bounds.width > 0 && d.bounds.height > 0));
}
