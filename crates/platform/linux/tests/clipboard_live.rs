//! Live clipboard test. Mutates the real system clipboard.
//! Run manually: `cargo test -p vantage-platform-linux --test clipboard_live -- --ignored`
#![cfg(target_os = "linux")]

use vantage_core::{ClipboardAccess, ClipboardKind, ClipboardPrefer};
use vantage_platform_linux::LinuxClipboard;

#[test]
#[ignore = "mutates the real system clipboard; needs a desktop session"]
fn reads_back_written_text() {
    let mut board = arboard::Clipboard::new().unwrap();
    board.set_text("vantage-clip-test").unwrap();

    let clip = LinuxClipboard::new();
    let content = clip.read(ClipboardPrefer::Text).expect("read");
    assert_eq!(content.kind, ClipboardKind::Text);
    assert_eq!(content.text.as_deref(), Some("vantage-clip-test"));
}
