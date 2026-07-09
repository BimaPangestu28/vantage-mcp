# Design Spec: vantage-mcp — Complete the PRD act surface (Spec C.1)

| Field | Value |
|---|---|
| Source PRD | `PRD-desktop-capture-mcp.md` §6.2, §6.3, §9 |
| Predecessor spec | `2026-07-09-vantage-mcp-act-tools-design.md` (Spec C) |
| Scope | Fill the act-tool gaps left by Spec C so §6 is fully implemented |
| Target platform | macOS + Linux (X11 solid; native-Wayland input compositor-limited) |
| Status | Approved design, ready for implementation plan |
| Date | 2026-07-09 |

Spec C shipped four act tools behind a single on/off gate. The PRD lists **six**
act tools plus per-tool disable and a couple of params. Spec C.1 closes that gap.

---

## 1. Objective

Bring the act-tool surface to full PRD §6.2/§6.3 conformance:

- Rename `clipboard_write` → **`write_clipboard`** (PRD naming).
- `write_clipboard` supports **image** as well as text.
- Add **`move_mouse`** and **`key_press`** (modifier+key combos).
- `click` gains a **`double`** option.
- The act gate is **per-tool selectable** via config, not just all-or-nothing.

### Success criteria

1. `tools/list` (gate on, all) shows: `write_clipboard`, `type_text`, `click`,
   `focus_window`, `move_mouse`, `key_press` (+ the six read tools = 12 total).
2. `VANTAGE_ACT_TOOLS=write_clipboard,click` mounts exactly those two act tools;
   `--allow-act`/`VANTAGE_ALLOW_ACT=1` mounts all six; neither → none.
3. `write_clipboard` accepts `{text?, image?}` (≥1 required); text and image both
   round-trip through `read_clipboard`.
4. `key_press` parses `"ctrl+shift+t"`-style combos; an unparseable combo returns
   `invalid_params`.
5. `click { …, double: true }` performs a double-click.
6. Existing read tools, the security isolation (unmounted when disabled), and all
   Spec A/B/C behaviour are unchanged.

### Non-goals

- Scroll, drag, key-hold/repeat, chorded mouse — not in PRD §6.2.
- Clipboard history/watcher (PRD out-of-scope).

---

## 2. Architecture

### 2.1 Core trait changes (`InputController`, additive/extend)

```rust
pub trait InputController: Send + Sync {
    // CHANGED: text and/or image (was `write_clipboard(&str)`).
    fn write_clipboard(&self, text: Option<&str>, image: Option<&RgbaImage>) -> Result<(), CaptureError>;
    fn type_text(&self, text: &str) -> Result<(), CaptureError>;
    // CHANGED: + double.
    fn click(&self, x: i32, y: i32, button: MouseButton, double: bool) -> Result<(), CaptureError>;
    fn focus_window(&self, target: &WindowInfo) -> Result<(), CaptureError>;
    // NEW:
    fn move_mouse(&self, x: i32, y: i32) -> Result<(), CaptureError>;
    fn key_press(&self, keys: &str) -> Result<(), CaptureError>;
}
```

All implementors (`LinuxInputController`, `MacInputController`) and every mock
(`NoInput`, `RecInput`s) update together. `CaptureError`/`ErrorKind` unchanged.

### 2.2 Per-tool gate

`main` computes the set of act tool names to mount:

- If `VANTAGE_ACT_TOOLS` (env) or `--act-tools=<csv>` (flag) is present and
  non-empty → **exactly** those names (validated against the known set; unknown
  names logged as a stderr warning and ignored).
- Else if `--allow-act` / `VANTAGE_ALLOW_ACT` truthy → **all six**.
- Else → **none**.

`Vantage::new(..., allowed_act: Vec<String>)` (replaces the `allow_act: bool`).
The handler builds the full `act_tool_router`, then `remove_route`s any act tool
whose name is not in `allowed_act`, and merges the result only if non-empty. A
`const ACT_TOOL_NAMES: [&str; 6]` drives validation + removal. Gate is "on" iff
`allowed_act` is non-empty; the security guarantee (unmounted → invisible +
uncallable) is unchanged, now at per-tool granularity. Startup still logs a
`warn!` naming which act tools are enabled.

### 2.3 Tools (handler)

| Tool | Params | Backend |
|---|---|---|
| `write_clipboard` | `{ text?: string, image?: string(base64 PNG) }` | decode image → `RgbaImage`; ≥1 of text/image or `invalid_params` |
| `move_mouse` | `{ x, y }` | `InputController::move_mouse` |
| `key_press` | `{ keys: string }` | `InputController::key_press` |
| `click` | `{ x, y, button?, double?: bool }` | `click(.., double)` |

`type_text`, `focus_window` unchanged except the trait ripple. All return
`AckOutput { ok }`, run on `spawn_blocking`, map via `to_mcp_error`, log an audit
line.

### 2.4 Backends

- **`write_clipboard` image** — Linux: the existing detached serving thread now
  calls `set().wait().text(t)` or `set().wait().image(ImageData{width,height,bytes})`;
  macOS: `set_image` (Clipboard is `Send` there). Base64 PNG is decoded to
  `RgbaImage` in the handler (reusing `image`), passed to the backend.
- **`move_mouse`** — enigo `move_mouse(x, y, Coordinate::Abs)`.
- **`click` double** — enigo: click, and if `double`, click again.
- **`key_press`** — parse in a shared `parse_combo(&str) -> Result<(Vec<Key>, Key), CaptureError>`
  helper: `+`-split; map modifier tokens `ctrl|control`, `alt|option`,
  `shift`, `meta|cmd|command|super|win` → `Key::Control/Alt/Shift/Meta`; the final
  token → `Key::Unicode(c)` for a single char, else a named key
  (`enter|return`, `tab`, `esc|escape`, `space`, `backspace`, `delete`, `up|down|left|right`,
  `f1`..`f12`). Press modifiers (`Direction::Press`), the main key
  (`Direction::Click`), then release modifiers in reverse (`Direction::Release`).
  Unknown token → `CaptureError::InvalidInput`-style → `invalid_params`.

The combo parser is pure (no I/O) → **unit-testable without a display**.

---

## 3. Testing

| Test | Where |
|---|---|
| `parse_combo` unit: `"ctrl+shift+t"`, `"cmd+c"`, `"enter"`, `"f5"`, unknown → err | anywhere (pure) |
| Gate: `allowed_act=[]` → none mounted; `["write_clipboard"]` → only that; all six | unit (handler) |
| Gate resolution: `VANTAGE_ACT_TOOLS` subset / `--allow-act` all / none | unit (main) |
| Each tool forwards to `InputController` (mock records args) | unit (handler) |
| `write_clipboard` missing both text+image → `invalid_params` | unit |
| `write_clipboard` text + image round-trip via `read_clipboard` | Linux live |
| existing Spec C tests updated for the new trait signatures, still green | Linux box ✅ |

### Verification matrix (GNOME/Wayland)

- ✅ Here: parser units, gate matrix + resolution, forwarding mocks,
  `write_clipboard` text+image live, full build/test/clippy/fmt, e2e tool counts.
- ⚠️ Not here: live `move_mouse`/`key_press`/`click` injection (session
  side-effects); macOS paths (no Mac). Grounded in real enigo/arboard APIs +
  mock/parser coverage; flagged in the plan.

---

## 4. Risks

1. **key combo coverage.** The named-key table is finite; unknown keys return an
   actionable error rather than a wrong keystroke. Extendable later.
2. **arboard image persistence on Wayland.** Same drop-clears-offer issue as text;
   handled by the existing wait-thread (now serving image too).
3. **Per-tool gate mis-parse.** Unknown `VANTAGE_ACT_TOOLS` entries are warned and
   ignored, never silently mounting an unintended tool.
4. **macOS act code unverifiable here.** Minimal, enigo/arboard-based; inspection + cross-compile.
