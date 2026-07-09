//! Live act-tool tests. Mutates real input state (clipboard / window focus).
//! Run: `cargo test -p vantage-platform-linux --test input_live -- --ignored`
#![cfg(target_os = "linux")]

use vantage_core::{ClipboardAccess, ClipboardPrefer, InputController};
use vantage_platform_linux::{LinuxClipboard, LinuxInputController};

#[test]
#[ignore = "mutates the real system clipboard"]
fn clipboard_write_then_read_roundtrips() {
    let input = LinuxInputController::new();
    input.write_clipboard("vantage-act-test").expect("write");
    let content = LinuxClipboard::new()
        .read(ClipboardPrefer::Text)
        .expect("read");
    assert_eq!(content.text.as_deref(), Some("vantage-act-test"));
}

#[test]
#[ignore = "focuses a real window via AT-SPI grab_focus"]
fn focus_first_window_does_not_error() {
    use vantage_core::{WindowFilter, WindowInspector};
    use vantage_platform_linux::LinuxWindowInspector;
    let ins = LinuxWindowInspector::new();
    let ws = ins
        .list_windows(WindowFilter {
            app_filter: None,
            on_screen_only: true,
        })
        .unwrap();
    if let Some(w) = ws.first() {
        LinuxInputController::new().focus_window(w).expect("focus");
    }
}
