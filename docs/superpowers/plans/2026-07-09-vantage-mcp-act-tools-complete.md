# vantage-mcp Complete the PRD act surface (Spec C.1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fill the PRD §6 act-tool gaps: rename `clipboard_write`→`write_clipboard` (+ image), add `move_mouse` and `key_press`, add `click.double`, and make the act gate per-tool selectable.

**Architecture:** Extend the `InputController` trait; implement the new/changed methods via enigo + arboard in both backends (with a pure, unit-testable `parse_combo` for key combos). Replace the handler's `allow_act: bool` with an allowed-act-tool list resolved in `main` from `VANTAGE_ACT_TOOLS`/`--act-tools` (subset) or `--allow-act`/`VANTAGE_ALLOW_ACT` (all); the handler removes unmounted act routes before merging.

**Tech Stack:** Rust 1.95; enigo 0.6 (`Key`, `move_mouse`, `button`); arboard 3 (`set().wait().image()`); rmcp `ToolRouter::remove_route`.

## Global Constraints

- **Extend `InputController` additively; update every impl + mock together** (`LinuxInputController`, `MacInputController`, `NoInput`, the `RecInput` test mocks). `CaptureError`/`ErrorKind` unchanged.
- **Security unchanged:** an act tool not in the allowed set is not mounted (absent from `tools/list`, uncallable). Gate is on iff the allowed set is non-empty.
- **`parse_combo` is pure** (string → modifiers+key), unit-testable without a display. Unknown token → an error mapped to `invalid_params`.
- **Text-first, stdout-sacred, spawn_blocking, audit-log each act call, no production `unwrap()`/`panic!`.** Commit after each task's tests pass. Conventional commits.

---

### Task 1: Extend `InputController` + backends (move_mouse, key_press, click.double, write_clipboard image)

**Files:**
- Modify: `crates/core/src/traits.rs` (trait signatures)
- Modify: `crates/platform/linux/src/input.rs` (real impls + `parse_combo` + unit tests)
- Modify: `crates/platform/macos/src/input.rs` (mirror impls)
- Modify: `crates/mcp-server/src/handler.rs` (update `NoInput` + `RecInput` mocks to compile)

**Interfaces:**
- Produces: updated `InputController` — `write_clipboard(Option<&str>, Option<&RgbaImage>)`, `click(i32,i32,MouseButton,bool)`, `move_mouse(i32,i32)`, `key_press(&str)`; a `parse_combo(&str) -> Result<(Vec<enigo::Key>, enigo::Key), CaptureError>` in each input backend.

- [ ] **Step 1: Update the trait in `crates/core/src/traits.rs`**

```rust
pub trait InputController: Send + Sync {
    fn write_clipboard(
        &self,
        text: Option<&str>,
        image: Option<&RgbaImage>,
    ) -> Result<(), CaptureError>;
    fn type_text(&self, text: &str) -> Result<(), CaptureError>;
    fn click(&self, x: i32, y: i32, button: MouseButton, double: bool)
        -> Result<(), CaptureError>;
    fn focus_window(&self, target: &WindowInfo) -> Result<(), CaptureError>;
    fn move_mouse(&self, x: i32, y: i32) -> Result<(), CaptureError>;
    fn key_press(&self, keys: &str) -> Result<(), CaptureError>;
}
```

`RgbaImage` is already imported in `traits.rs`.

- [ ] **Step 2: Write `parse_combo` + unit tests in `crates/platform/linux/src/input.rs`**

Add (module-level):

```rust
use enigo::Key;

/// Parse a combo like "ctrl+shift+t" into (held modifiers, final key).
/// Pure; unit-tested. Unknown tokens yield an InvalidInput error.
fn parse_combo(keys: &str) -> Result<(Vec<Key>, Key), CaptureError> {
    let mut parts = keys.split('+').map(|p| p.trim()).filter(|p| !p.is_empty()).peekable();
    let mut mods = Vec::new();
    let mut main: Option<Key> = None;
    let tokens: Vec<&str> = parts.by_ref().collect();
    if tokens.is_empty() {
        return Err(CaptureError::InvalidInput_msg("empty key combo"));
    }
    let (last, rest) = tokens.split_last().unwrap();
    for tok in rest {
        mods.push(modifier_key(tok).ok_or_else(|| {
            CaptureError::Internal(format!("unknown modifier in combo: {tok:?}"))
        })?);
    }
    main = Some(named_or_char_key(last)?);
    Ok((mods, main.unwrap()))
}

fn modifier_key(tok: &str) -> Option<Key> {
    match tok.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(Key::Control),
        "alt" | "option" => Some(Key::Alt),
        "shift" => Some(Key::Shift),
        "meta" | "cmd" | "command" | "super" | "win" => Some(Key::Meta),
        _ => None,
    }
}

fn named_or_char_key(tok: &str) -> Result<Key, CaptureError> {
    let lower = tok.to_ascii_lowercase();
    let key = match lower.as_str() {
        "enter" | "return" => Key::Return,
        "tab" => Key::Tab,
        "esc" | "escape" => Key::Escape,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "f1" => Key::F1, "f2" => Key::F2, "f3" => Key::F3, "f4" => Key::F4,
        "f5" => Key::F5, "f6" => Key::F6, "f7" => Key::F7, "f8" => Key::F8,
        "f9" => Key::F9, "f10" => Key::F10, "f11" => Key::F11, "f12" => Key::F12,
        _ => {
            let mut chars = tok.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Unicode(c),
                _ => {
                    return Err(CaptureError::Internal(format!("unknown key in combo: {tok:?}")))
                }
            }
        }
    };
    Ok(key)
}

#[cfg(test)]
mod combo_tests {
    use super::*;
    #[test]
    fn parses_modifiers_and_named_keys() {
        let (m, k) = parse_combo("ctrl+shift+t").unwrap();
        assert_eq!(m, vec![Key::Control, Key::Shift]);
        assert_eq!(k, Key::Unicode('t'));
        assert_eq!(parse_combo("cmd+c").unwrap().0, vec![Key::Meta]);
        assert_eq!(parse_combo("enter").unwrap().1, Key::Return);
        assert_eq!(parse_combo("f5").unwrap().1, Key::F5);
        assert!(parse_combo("ctrl+nope").is_err());
        assert!(parse_combo("").is_err());
    }
}
```

Note: there is no `CaptureError::InvalidInput_msg`; use
`CaptureError::Internal("empty key combo".into())` for the empty case (mapped to
an error; the handler validates presence too). Adjust the empty-combo line to
`Err(CaptureError::Internal("empty key combo".into()))`.

- [ ] **Step 3: Implement the new/changed methods in `crates/platform/linux/src/input.rs`**

Replace `write_clipboard`, `click`, and the `type_text`-adjacent stubs; add
`move_mouse`, `key_press`. `write_clipboard` (Linux) — serve text or image from
the wait-thread:

```rust
    fn write_clipboard(
        &self,
        text: Option<&str>,
        image: Option<&RgbaImage>,
    ) -> Result<(), CaptureError> {
        use arboard::SetExtLinux;
        enum Payload {
            Text(String),
            Image(u32, u32, Vec<u8>),
        }
        let payload = match (text, image) {
            (Some(t), _) => Payload::Text(t.to_owned()),
            (None, Some(img)) => Payload::Image(img.width, img.height, img.pixels.clone()),
            (None, None) => {
                return Err(CaptureError::InvalidInput(vantage_core::Bounds {
                    x: 0, y: 0, width: 0, height: 0,
                }))
            }
        };
        let (tx, rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);
        std::thread::Builder::new()
            .name("vantage-clipboard".into())
            .spawn(move || match arboard::Clipboard::new() {
                Ok(mut board) => {
                    let _ = tx.send(Ok(()));
                    let _ = match payload {
                        Payload::Text(t) => board.set().wait().text(t),
                        Payload::Image(w, h, bytes) => board.set().wait().image(arboard::ImageData {
                            width: w as usize,
                            height: h as usize,
                            bytes: std::borrow::Cow::Owned(bytes),
                        }),
                    };
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                }
            })
            .map_err(|e| CaptureError::Internal(format!("spawn clipboard thread: {e}")))?;
        match rx.recv() {
            Ok(Ok(())) => {
                std::thread::sleep(std::time::Duration::from_millis(60));
                Ok(())
            }
            Ok(Err(e)) => Err(CaptureError::Internal(format!("clipboard open: {e}"))),
            Err(e) => Err(CaptureError::Internal(format!("clipboard thread: {e}"))),
        }
    }
```

Note: `CaptureError::InvalidInput(Bounds)` is the closest existing variant for
"no content"; the handler rejects the empty case earlier with a clear
`invalid_params`, so this backend branch is a defensive fallback. (Do not add a
new error variant.)

`click` (+ double), `move_mouse`, `key_press`:

```rust
    fn click(
        &self,
        x: i32,
        y: i32,
        button: MouseButton,
        double: bool,
    ) -> Result<(), CaptureError> {
        use enigo::{Button, Coordinate, Direction, Enigo, Mouse, Settings};
        let mut enigo =
            Enigo::new(&Settings::default()).map_err(|e| classify_input_error(&e.to_string()))?;
        enigo
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(|e| classify_input_error(&e.to_string()))?;
        let btn = match button {
            MouseButton::Left => Button::Left,
            MouseButton::Right => Button::Right,
            MouseButton::Middle => Button::Middle,
        };
        enigo
            .button(btn, Direction::Click)
            .map_err(|e| classify_input_error(&e.to_string()))?;
        if double {
            enigo
                .button(btn, Direction::Click)
                .map_err(|e| classify_input_error(&e.to_string()))?;
        }
        Ok(())
    }

    fn move_mouse(&self, x: i32, y: i32) -> Result<(), CaptureError> {
        use enigo::{Coordinate, Enigo, Mouse, Settings};
        let mut enigo =
            Enigo::new(&Settings::default()).map_err(|e| classify_input_error(&e.to_string()))?;
        enigo
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(|e| classify_input_error(&e.to_string()))
    }

    fn key_press(&self, keys: &str) -> Result<(), CaptureError> {
        use enigo::{Direction, Enigo, Keyboard, Settings};
        let (mods, main) = parse_combo(keys)?;
        let mut enigo =
            Enigo::new(&Settings::default()).map_err(|e| classify_input_error(&e.to_string()))?;
        for m in &mods {
            enigo.key(*m, Direction::Press).map_err(|e| classify_input_error(&e.to_string()))?;
        }
        let r = enigo.key(main, Direction::Click).map_err(|e| classify_input_error(&e.to_string()));
        // Always release modifiers, even if the main key failed.
        for m in mods.iter().rev() {
            let _ = enigo.key(*m, Direction::Release);
        }
        r
    }
```

- [ ] **Step 4: Mirror the impls in `crates/platform/macos/src/input.rs`**

Same method bodies (duplicate `parse_combo`/`modifier_key`/`named_or_char_key`
into the macOS file — the platform crates do not share code; both depend on
enigo). macOS `write_clipboard`: `Clipboard` is `Send`, so no wait-thread:

```rust
    fn write_clipboard(
        &self,
        text: Option<&str>,
        image: Option<&RgbaImage>,
    ) -> Result<(), CaptureError> {
        let mut board = arboard::Clipboard::new()
            .map_err(|e| CaptureError::Internal(format!("clipboard open: {e}")))?;
        match (text, image) {
            (Some(t), _) => board
                .set_text(t.to_owned())
                .map_err(|e| CaptureError::Internal(format!("clipboard set_text: {e}"))),
            (None, Some(img)) => board
                .set_image(arboard::ImageData {
                    width: img.width as usize,
                    height: img.height as usize,
                    bytes: std::borrow::Cow::Borrowed(&img.pixels),
                })
                .map_err(|e| CaptureError::Internal(format!("clipboard set_image: {e}"))),
            (None, None) => Err(CaptureError::Internal("no clipboard content".into())),
        }
    }
```

Give `MacInputController::click`/`move_mouse`/`key_press` the same enigo bodies as
Linux (map errors to `CaptureError::Internal`).

- [ ] **Step 5: Update the mocks in `handler.rs` to the new signatures**

Every `impl InputController for …` (`NoInput`, the two `RecInput`s) needs:
`write_clipboard(Option<&str>, Option<&RgbaImage>)`, `click(.., bool)`, plus new
`move_mouse` and `key_press`. For `NoInput`, all return `Unsupported("mock")`.
Update the Task-2/Task-4 `RecInput`s to the new `write_clipboard`/`click`
signatures and add no-op `move_mouse`/`key_press` (or record where a test needs it).

- [ ] **Step 6: Build + parser test**

Run: `cargo build --workspace && cargo test -p vantage-platform-linux combo_tests`
Expected: PASS — compiles; `parses_modifiers_and_named_keys` passes.

- [ ] **Step 7: Commit**

```bash
git add crates
git commit -m "feat(act): extend InputController (move_mouse, key_press, click.double, clipboard image)"
```

---

### Task 2: Handler tools — rename + image write + move_mouse + key_press + click.double

**Files:**
- Modify: `crates/mcp-server/src/handler.rs`
- Test: inline handler tests

**Interfaces:**
- Consumes: the extended `InputController`; `image` crate for base64 PNG decode (already a mcp-server dep, plus `base64`).
- Produces: act tools `write_clipboard {text?, image?}`, `move_mouse {x,y}`, `key_press {keys}`, `click {x,y,button?,double?}`; `write_clipboard` replaces `clipboard_write`.

- [ ] **Step 1: Replace `clipboard_write` with `write_clipboard` (text + image)**

Rename the tool method and params; decode the optional base64 PNG:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteClipboardParams {
    /// Text to place on the clipboard.
    #[serde(default)]
    pub text: Option<String>,
    /// Image to place on the clipboard, as a base64-encoded PNG.
    #[serde(default)]
    pub image: Option<String>,
}
```

```rust
/// Write text and/or an image to the system clipboard. (Act tool.)
#[tool(description = "Write text and/or a base64-PNG image to the clipboard.")]
pub async fn write_clipboard(
    &self,
    Parameters(params): Parameters<WriteClipboardParams>,
) -> Result<Json<AckOutput>, ErrorData> {
    use base64::Engine;
    if params.text.is_none() && params.image.is_none() {
        return Err(ErrorData::invalid_params(
            "write_clipboard requires at least one of `text` or `image`".into(),
            None,
        ));
    }
    tracing::info!(
        "act: write_clipboard (text={}, image={})",
        params.text.is_some(),
        params.image.is_some()
    );
    let image = match params.image {
        Some(b64) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64.as_bytes())
                .map_err(|e| ErrorData::invalid_params(format!("image is not valid base64: {e}"), None))?;
            let decoded = image::load_from_memory(&bytes)
                .map_err(|e| ErrorData::invalid_params(format!("image is not a valid PNG: {e}"), None))?
                .to_rgba8();
            Some(vantage_core::RgbaImage {
                width: decoded.width(),
                height: decoded.height(),
                pixels: decoded.into_raw(),
            })
        }
        None => None,
    };
    let text = params.text;
    let input = self.input.clone();
    tokio::task::spawn_blocking(move || {
        input.write_clipboard(text.as_deref(), image.as_ref())
    })
    .await
    .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(to_mcp_error)?;
    Ok(Json(AckOutput { ok: true }))
}
```

- [ ] **Step 2: Add `double` to `click`**

Add `#[serde(default)] pub double: bool` to `ClickParams`, and pass it through:
`input.click(x, y, button, double)` in the `click` tool body.

- [ ] **Step 3: Add `move_mouse` and `key_press` tools**

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveMouseParams { pub x: i32, pub y: i32 }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct KeyPressParams {
    /// Key combo, e.g. "ctrl+shift+t" or "enter".
    pub keys: String,
}
```

```rust
/// Move the mouse to absolute screen coordinates. (Act tool.)
#[tool(description = "Move the mouse to (x, y).")]
pub async fn move_mouse(
    &self,
    Parameters(p): Parameters<MoveMouseParams>,
) -> Result<Json<AckOutput>, ErrorData> {
    tracing::info!("act: move_mouse ({},{})", p.x, p.y);
    let input = self.input.clone();
    tokio::task::spawn_blocking(move || input.move_mouse(p.x, p.y))
        .await
        .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?
        .map_err(to_mcp_error)?;
    Ok(Json(AckOutput { ok: true }))
}

/// Press a key combo (modifier+key). (Act tool.)
#[tool(description = "Press a key combo, e.g. ctrl+shift+t.")]
pub async fn key_press(
    &self,
    Parameters(p): Parameters<KeyPressParams>,
) -> Result<Json<AckOutput>, ErrorData> {
    tracing::info!("act: key_press {:?}", p.keys);
    let input = self.input.clone();
    tokio::task::spawn_blocking(move || input.key_press(&p.keys))
        .await
        .map_err(|e| ErrorData::internal_error(format!("task join error: {e}"), None))?
        .map_err(to_mcp_error)?;
    Ok(Json(AckOutput { ok: true }))
}
```

- [ ] **Step 4: Update tests for the rename + new signatures**

Rename `clipboard_write_forwards_to_input` to use `write_clipboard`/`WriteClipboardParams`
(text case: `{ text: Some("hello"), image: None }`; assert the mock got
`Some("hello")`, no image). Add a `write_clipboard` missing-both →
`invalid_params` test. Update the `RecInput` mocks' `write_clipboard`/`click`
signatures. Add a forwarding test for `move_mouse`/`key_press`.

- [ ] **Step 5: Build + test**

Run: `cargo test -p vantage-mcp-server`
Expected: PASS — all handler tests incl. the new tools and the missing-content rejection.

- [ ] **Step 6: Commit**

```bash
git add crates/mcp-server/src/handler.rs
git commit -m "feat(act): write_clipboard (text+image), move_mouse, key_press, click.double"
```

---

### Task 3: Per-tool gate (allowed-act list)

**Files:**
- Modify: `crates/mcp-server/src/handler.rs` (`Vantage::new` + router assembly + `ACT_TOOL_NAMES`)
- Modify: `crates/mcp-server/src/main.rs` (resolve the allowed set)
- Test: handler gate tests + main resolution test

**Interfaces:**
- Produces: `Vantage::new(..., allowed_act: Vec<String>)` (replaces `allow_act: bool`); `main::act_tools(args, allow_env, tools_env) -> Vec<String>`.

- [ ] **Step 1: `ACT_TOOL_NAMES` + per-tool router assembly in `handler.rs`**

```rust
/// The names of the act tools, for gate validation + selective mounting.
pub const ACT_TOOL_NAMES: [&str; 6] = [
    "write_clipboard", "type_text", "click", "focus_window", "move_mouse", "key_press",
];
```

Change `new`:

```rust
pub fn new(
    windows: Arc<dyn WindowInspector>,
    capturer: Arc<dyn ScreenCapturer>,
    ocr: Arc<dyn TextRecognizer>,
    clipboard: Arc<dyn ClipboardAccess>,
    input: Arc<dyn InputController>,
    allowed_act: Vec<String>,
) -> Self {
    let mut tool_router = Self::read_tool_router();
    if !allowed_act.is_empty() {
        let mut act = Self::act_tool_router();
        for name in ACT_TOOL_NAMES {
            if !allowed_act.iter().any(|a| a == name) {
                act.remove_route(name);
            }
        }
        tool_router.merge(act);
    }
    Self { windows, capturer, ocr, clipboard, input, tool_router }
}
```

- [ ] **Step 2: Resolve the allowed set in `main.rs`**

Replace `act_enabled` with:

```rust
/// Resolve which act tools to mount. `VANTAGE_ACT_TOOLS` / `--act-tools=<csv>`
/// selects a subset; `--allow-act` / `VANTAGE_ALLOW_ACT` (truthy) selects all;
/// otherwise none. Unknown names are warned and ignored.
fn act_tools(
    args: impl Iterator<Item = String>,
    allow_env: Option<String>,
    tools_env: Option<String>,
) -> Vec<String> {
    let mut flag_all = false;
    let mut flag_csv: Option<String> = None;
    for a in args {
        if a == "--allow-act" {
            flag_all = true;
        } else if let Some(csv) = a.strip_prefix("--act-tools=") {
            flag_csv = Some(csv.to_string());
        }
    }
    let allow_env = allow_env
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let csv = flag_csv.or(tools_env);
    if let Some(csv) = csv.filter(|s| !s.trim().is_empty()) {
        return csv
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .filter(|s| {
                let ok = handler::ACT_TOOL_NAMES.contains(&s.as_str());
                if !ok {
                    tracing::warn!("ignoring unknown act tool in config: {s:?}");
                }
                ok
            })
            .collect();
    }
    if flag_all || allow_env {
        return handler::ACT_TOOL_NAMES.iter().map(|s| s.to_string()).collect();
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn e() -> std::iter::Empty<String> { std::iter::empty() }
    #[test]
    fn none_by_default() {
        assert!(act_tools(e(), None, None).is_empty());
    }
    #[test]
    fn all_via_flag_or_env() {
        assert_eq!(act_tools(["--allow-act".into()].into_iter(), None, None).len(), 6);
        assert_eq!(act_tools(e(), Some("1".into()), None).len(), 6);
    }
    #[test]
    fn subset_via_env_or_flag_and_drops_unknown() {
        let v = act_tools(e(), None, Some("write_clipboard, click, bogus".into()));
        assert_eq!(v, vec!["write_clipboard".to_string(), "click".to_string()]);
        let f = act_tools(["--act-tools=key_press".into()].into_iter(), Some("1".into()), None);
        assert_eq!(f, vec!["key_press".to_string()]); // csv wins over all-switch
    }
}
```

Wire into `main` (replace the `allow_act` block):

```rust
let allowed_act = act_tools(
    std::env::args(),
    std::env::var("VANTAGE_ALLOW_ACT").ok(),
    std::env::var("VANTAGE_ACT_TOOLS").ok(),
);
if !allowed_act.is_empty() {
    tracing::warn!("act tools ENABLED: {}", allowed_act.join(", "));
}
let (windows, capturer, ocr, clipboard, input) = backend::backends();
let service = Vantage::new(windows, capturer, ocr, clipboard, input, allowed_act)
    .serve(stdio())
    .await?;
```

`handler` module must expose `ACT_TOOL_NAMES` (it is `pub`). Ensure `main.rs`
can name it (`handler::ACT_TOOL_NAMES`).

- [ ] **Step 3: Update handler gate tests + `vantage_gated`**

Change `vantage_gated(allow_act: bool)` to `vantage_gated(allowed: &[&str])`:

```rust
fn vantage_gated(allowed: &[&str]) -> Vantage {
    Vantage::new(
        Arc::new(MockWindows::default()),
        Arc::new(NoScreen),
        Arc::new(NoOcr),
        Arc::new(NoClip),
        Arc::new(NoInput),
        allowed.iter().map(|s| s.to_string()).collect(),
    )
}
```

Update the gate test to cover subset mounting:

```rust
#[test]
fn act_gate_mounts_only_allowed_tools() {
    let off = vantage_gated(&[]);
    assert!(!off.tool_router.has_route("write_clipboard"));
    assert!(off.tool_router.has_route("list_windows"));

    let subset = vantage_gated(&["write_clipboard", "click"]);
    assert!(subset.tool_router.has_route("write_clipboard"));
    assert!(subset.tool_router.has_route("click"));
    assert!(!subset.tool_router.has_route("key_press"));
    assert!(!subset.tool_router.has_route("type_text"));

    let all = vantage_gated(&ACT_TOOL_NAMES);
    for n in ACT_TOOL_NAMES {
        assert!(all.tool_router.has_route(n), "{n} should be mounted");
    }
}
```

Fix every other `Vantage::new(...)`/`vantage_gated(true|false)` call site to the
new signature (`vantage_gated(&[])` for off, `vantage_gated(&ACT_TOOL_NAMES)` or
a specific slice for on; inline `Vantage::new(..., vec![...])`).

- [ ] **Step 4: Build + test**

Run: `cargo build --workspace && cargo test -p vantage-mcp-server && cargo test -p vantage-mcp-server --bin vantage-mcp act_tools`
Expected: PASS — gate matrix + main resolution tests.

- [ ] **Step 5: Commit**

```bash
git add crates/mcp-server
git commit -m "feat(act): per-tool gate via VANTAGE_ACT_TOOLS / --act-tools (PRD 6.3)"
```

---

### Task 4: End-to-end + docs

**Files:**
- Modify: `README.md`, `docs/agent-registration.md`, `CLAUDE.md`
- Test: e2e via the built binary

- [ ] **Step 1: Full build/test/clippy/fmt**

Run: `cargo build --workspace && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all --check`
Expected: PASS/clean.

- [ ] **Step 2: E2E gate matrix**

Build release, then verify via `tools/list`:
- no gate → 6 tools (read only).
- `VANTAGE_ACT_TOOLS=write_clipboard,click` → 8 tools (read 6 + those 2).
- `VANTAGE_ALLOW_ACT=1` → 12 tools (read 6 + act 6).
And a `write_clipboard` with `{text}` then `read_clipboard` round-trip; a
`write_clipboard` with a small base64-PNG `{image}` returns `{ok:true}`.

- [ ] **Step 3: Update docs**

- `README.md`: act-tools table → the six tools with correct params
  (`write_clipboard {text?,image?}`, `click {…,double?}`, `move_mouse`,
  `key_press {keys}`); document `VANTAGE_ACT_TOOLS` / `--act-tools` per-tool gate.
- `docs/agent-registration.md`: mention per-tool selection alongside `--allow-act`.
- `CLAUDE.md`: six act tools, `allowed_act` list on `Vantage::new`, `ACT_TOOL_NAMES`.

- [ ] **Step 4: Commit**

```bash
git add README.md docs/agent-registration.md CLAUDE.md
git commit -m "docs: full act-tool surface + per-tool gate"
```

---

## Self-Review

**Spec coverage:**
- §2.1 trait changes (write_clipboard text+image, click.double, move_mouse, key_press) → Task 1. ✅
- §2.2 per-tool gate (allowed_act list, VANTAGE_ACT_TOOLS/--act-tools resolution, remove_route) → Task 3. ✅
- §2.3 handler tools (rename, image decode, move_mouse, key_press, click.double) → Task 2. ✅
- §2.4 backends (arboard image via wait-thread, enigo move/click-double/key combos, pure parse_combo) → Task 1. ✅
- §3 testing (parse_combo units, gate matrix, main resolution, forwarding mocks, missing-content reject, live text+image) → Tasks 1–4. ✅

**Placeholder scan:** No TODO/TBD. The `parse_combo` empty-case error type is
corrected in a Step-2 note (`CaptureError::Internal`, not a nonexistent variant).
`move_mouse`/`key_press`/`click` live injection is intentionally not fired (session
side-effects) — a stated verification boundary, not a gap. macOS is inspection +
cross-compile as in Spec C.

**Type consistency:** `InputController`'s six methods share one signature across
both backends, `NoInput`, and the `RecInput` mocks. `write_clipboard(Option<&str>,
Option<&RgbaImage>)` and `click(.., bool)` are used identically in trait, backends,
handler, and tests. `allowed_act: Vec<String>` flows from `main::act_tools` →
`Vantage::new` → router assembly; `ACT_TOOL_NAMES` (6 entries) is the single source
for validation, removal, and the "all" set. `WriteClipboardParams`/`MoveMouseParams`/
`KeyPressParams`/`ClickParams.double` match their tool methods.
