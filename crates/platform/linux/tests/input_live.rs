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
