//! Live test: mutates the real system clipboard. Clipboard access requires
//! no TCC permission on macOS, but this is `#[ignore]`d because it clobbers
//! whatever the user currently has copied.
//! Run manually: `cargo test -p vantage-platform-macos --test clipboard_live -- --ignored`

#![cfg(target_os = "macos")]

use vantage_core::{ClipboardAccess, ClipboardKind, ClipboardPrefer};
use vantage_platform_macos::MacClipboard;

#[test]
#[ignore = "mutates the real system clipboard"]
fn reads_back_written_text() {
    let mut board = arboard::Clipboard::new().unwrap();
    board.set_text("vantage-clip-test").unwrap();

    let clip = MacClipboard::new();
    let content = clip.read(ClipboardPrefer::Text).expect("read");
    assert_eq!(content.kind, ClipboardKind::Text);
    assert_eq!(content.text.as_deref(), Some("vantage-clip-test"));
}
