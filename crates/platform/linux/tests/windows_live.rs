//! Live AT-SPI tests: require a desktop session with the accessibility bus
//! enabled and at least one on-screen application window.
//! Run manually: `cargo test -p vantage-platform-linux --test windows_live -- --ignored`
#![cfg(target_os = "linux")]

use vantage_core::{WindowFilter, WindowInspector};
use vantage_platform_linux::LinuxWindowInspector;

#[test]
#[ignore = "requires live desktop session + AT-SPI accessibility bus"]
fn lists_at_least_one_window() {
    let inspector = LinuxWindowInspector::new();
    let windows = inspector
        .list_windows(WindowFilter {
            app_filter: None,
            on_screen_only: true,
        })
        .expect("list_windows");
    assert!(
        !windows.is_empty(),
        "expected at least one on-screen window"
    );
    assert!(windows.iter().any(|w| !w.app.is_empty()));
}

#[test]
#[ignore = "requires live desktop session + AT-SPI accessibility bus"]
fn reads_some_text_from_first_window() {
    let inspector = LinuxWindowInspector::new();
    let windows = inspector
        .list_windows(WindowFilter {
            app_filter: None,
            on_screen_only: true,
        })
        .unwrap();
    let target = windows.first().expect("a window");
    let text = inspector
        .read_window_text(target.window_id, 20)
        .expect("read_window_text");
    // Content varies across apps; assert the call path returns the struct.
    let _ = text.truncated;
}
