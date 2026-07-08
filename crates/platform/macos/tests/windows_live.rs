//! Live tests: require a logged-in macOS session with at least one on-screen
//! window, plus Screen Recording (for titles) and Accessibility permissions.
//! Run manually: `cargo test -p vantage-platform-macos --test windows_live -- --ignored`

#![cfg(target_os = "macos")]

use vantage_core::{WindowFilter, WindowInspector};
use vantage_platform_macos::MacWindowInspector;

#[test]
#[ignore = "requires live macOS session + permissions"]
fn lists_at_least_one_window() {
    let inspector = MacWindowInspector::new();
    let windows = inspector
        .list_windows(WindowFilter { app_filter: None, on_screen_only: true })
        .expect("list_windows");
    assert!(!windows.is_empty(), "expected at least one on-screen window");
    assert!(windows.iter().any(|w| !w.app.is_empty()));
}

#[test]
#[ignore = "requires live macOS session + Accessibility permission"]
fn reads_some_text_from_first_window() {
    let inspector = MacWindowInspector::new();
    let windows = inspector
        .list_windows(WindowFilter { app_filter: None, on_screen_only: true })
        .unwrap();
    let target = windows.first().expect("a window");
    let text = inspector.read_window_text(target.window_id, 20).expect("read_window_text");
    // Content varies; assert the call path works and returns the struct.
    let _ = text.truncated;
}
