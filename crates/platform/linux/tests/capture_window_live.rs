//! Live capture_window test. On Wayland this asserts the actionable Unsupported
//! path; on X11 a bogus window resolves to WindowNotFound (not a panic).
//! Run: `cargo test -p vantage-platform-linux --test capture_window_live -- --ignored`
#![cfg(all(target_os = "linux", feature = "capture"))]

use vantage_core::{Bounds, CaptureError, ScreenCapturer, WindowInfo};
use vantage_platform_linux::LinuxScreenCapturer;

fn dummy_target() -> WindowInfo {
    WindowInfo {
        window_id: 1,
        app: "nonexistent-app".into(),
        title: "nonexistent-title".into(),
        bounds: Bounds {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        },
        focused: false,
    }
}

#[test]
#[ignore = "requires a desktop session"]
fn capture_window_behaviour_matches_session() {
    let cap = LinuxScreenCapturer::new();
    let is_wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|v| v.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
        || std::env::var("WAYLAND_DISPLAY").is_ok();
    let result = cap.capture_window(&dummy_target());
    if is_wayland {
        match result {
            Err(CaptureError::Unsupported(_)) => {}
            other => panic!("expected Unsupported on Wayland, got {other:?}"),
        }
    } else {
        match result {
            Err(CaptureError::WindowNotFound(_)) => {}
            other => panic!("expected WindowNotFound for a bogus window on X11, got {other:?}"),
        }
    }
}
