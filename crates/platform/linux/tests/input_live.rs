//! Live act-tool tests. Mutates real input state (clipboard / window focus).
//! Run: `cargo test -p vantage-platform-linux --test input_live -- --ignored`
#![cfg(target_os = "linux")]

use vantage_core::{ClipboardAccess, ClipboardPrefer, InputController, RgbaImage};
use vantage_platform_linux::{LinuxClipboard, LinuxInputController};

/// Text then image, in one test so the two writes don't race for the global
/// clipboard (each `write_clipboard` serves the selection from a thread until
/// it is replaced; running them concurrently would let one supersede the other).
#[test]
#[ignore = "mutates the real system clipboard"]
fn clipboard_write_text_then_image_roundtrips() {
    let input = LinuxInputController::new();

    input
        .write_clipboard(Some("vantage-act-test"), None)
        .expect("write text");
    let text = LinuxClipboard::new()
        .read(ClipboardPrefer::Text)
        .expect("read text");
    assert_eq!(text.text.as_deref(), Some("vantage-act-test"));

    // 2x2 solid red RGBA.
    let img = RgbaImage {
        width: 2,
        height: 2,
        pixels: [255u8, 0, 0, 255].repeat(4),
    };
    input
        .write_clipboard(None, Some(&img))
        .expect("write image");
    let content = LinuxClipboard::new()
        .read(ClipboardPrefer::Image)
        .expect("read image");
    let out = content.image.expect("clipboard has an image");
    assert_eq!((out.width, out.height), (2, 2));
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
