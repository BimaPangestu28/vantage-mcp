# Design Spec: vantage-mcp — Gated act tools (Spec C)

| Field | Value |
|---|---|
| Source PRD | `PRD-desktop-capture-mcp.md` |
| Predecessor specs | Phase 0/1 macOS; Linux read-slice (A); richer capture (B) |
| Scope | Four act tools behind a default-off policy gate, across macOS + Linux |
| Target platform | macOS + Linux; input injection is X11/macOS-solid, Wayland-limited |
| Transport | stdio (unchanged) |
| Status | Approved design, ready for implementation plan |
| Date | 2026-07-09 |

Sub-project **C** of the roadmap effort (A ✅, B ✅ merged). Introduces the first
**write** capability — kept structurally isolated and disabled by default.

---

## 1. Objective

Let an authorized operator enable a small set of act tools — `clipboard_write`,
`type_text`, `click`, `focus_window` — while guaranteeing that, by default, no
tool capable of a side effect is even reachable.

### Success criteria

1. **Default off:** with no gate set, the act tools are absent from `tools/list`
   and calling one fails as an unknown method. Read tools are unchanged.
2. **Explicit opt-in:** with `--allow-act` OR `VANTAGE_ALLOW_ACT=1`, the four act
   tools appear and work (subject to platform limits). Startup logs a clear
   stderr warning that act tools are enabled.
3. `clipboard_write` sets the clipboard on macOS and Linux (X11 + Wayland).
4. `focus_window` focuses the target window on macOS and Linux (X11 + Wayland via
   AT-SPI); `type_text`/`click` work on macOS and Linux/X11 and return an
   actionable error where the Wayland compositor forbids synthetic input.
5. The gate is resolved once at startup; no per-call env lookups. stdout stays
   JSON-RPC only.

### Non-goals

- Image clipboard write (text only this spec).
- Drag, scroll, key-chord/modifier combos, key-hold (just `type_text` + single `click`).
- Making Wayland input injection work where the compositor refuses it.

---

## 2. Architecture

### 2.1 Security model — structural isolation + gate

The act tools are a **separate rmcp tool-router group**. rmcp 2.1.0 `ToolRouter`
supports `merge`; two `#[tool_router(router = …)]` impl blocks produce a read
router and an act router. `Vantage` holds the composed router in a **field**:

```rust
#[derive(Clone)]
pub struct Vantage {
    windows: Arc<dyn WindowInspector>,
    capturer: Arc<dyn ScreenCapturer>,
    ocr: Arc<dyn TextRecognizer>,
    clipboard: Arc<dyn ClipboardAccess>,
    input: Arc<dyn InputController>,
    tool_router: ToolRouter<Self>,
}

pub fn new(..., input: Arc<dyn InputController>, allow_act: bool) -> Self {
    let mut tool_router = Self::read_tool_router();
    if allow_act {
        tool_router.merge(Self::act_tool_router());
    }
    Self { ..., input, tool_router }
}
```

`#[tool_handler(router = self.tool_router)]` serves exactly the mounted set. When
`allow_act` is false the act router is never merged, so those tools do not exist
at runtime — the strongest defense against prompt-injection-driven side effects.

**Gate resolution** (once, in `main`): enabled if the `--allow-act` CLI flag is
present OR `VANTAGE_ALLOW_ACT` is set to a truthy value (`1`/`true`/`yes`,
case-insensitive). On enable, `main` logs `warn!("act tools ENABLED via {source}")`.
Each act tool call logs an audit line to stderr (`info!`).

### 2.2 Core extensions

**New trait `InputController`** (`traits.rs`) — the deferred Phase-3 capability:

```rust
pub trait InputController: Send + Sync {
    fn write_clipboard(&self, text: &str) -> Result<(), CaptureError>;
    fn type_text(&self, text: &str) -> Result<(), CaptureError>;
    fn click(&self, x: i32, y: i32, button: MouseButton) -> Result<(), CaptureError>;
    fn focus_window(&self, target: &WindowInfo) -> Result<(), CaptureError>;
}
```

**New type** `MouseButton { Left, Right, Middle }` (`types.rs`), with a serde
default of `Left`. `CaptureError`/`ErrorKind` unchanged (act failures use
`Unsupported`/`Internal`/`WindowNotFound`; a new variant is not required).

`backends()` returns a **5-tuple** (adds `Arc<dyn InputController>`); both
platform crates and `main` update. A `NoInput` test double is added for the
handler tests.

### 2.3 Backends

Chosen to minimize hand-written, unverifiable per-platform input FFI:

- **`clipboard_write`** — arboard `set_text`. macOS + Linux (X11 + Wayland).
- **`focus_window`** —
  - **Linux:** reuse Spec A's AT-SPI enumeration to resolve the `WindowInfo` to
    its frame, then `ComponentProxy::grab_focus()`. Works on X11 **and** Wayland
    over D-Bus — no X11-specific code. (Runs on the backend's own tokio runtime,
    same async→sync bridge as `windows.rs`; the input backend either shares that
    pattern or delegates to a small AT-SPI helper.)
  - **macOS:** `AXUIElementPerformAction(AXRaise)` + activate the owning app.
- **`type_text` + `click`** — the **`enigo`** crate, a cross-platform input
  library (macOS CGEvent / Linux X11 XTEST / Wayland libei). One dependency
  instead of three FFI backends; also shrinks the macOS-unverifiable surface. On
  Wayland/GNOME, enigo's libei path may require a portal grant or fail — mapped to
  an actionable `Unsupported`/`Internal` error, never a silent no-op.

**Feature-gating:** if `enigo` drags in heavy Linux system libraries (libei/xdo)
the way xcap did, put `type_text`/`click` behind an `input` Cargo feature
(default-on) with a stub, mirroring the `capture`/`ocr` pattern. Decided when the
dependency graph is resolved in the plan; `clipboard_write` and `focus_window`
(arboard + AT-SPI, already present) never need it.

### 2.4 Act tools (MCP, gated)

| Tool | Params | Returns |
|---|---|---|
| `clipboard_write` | `{ text: string }` | `{ ok: true }` |
| `type_text` | `{ text: string }` | `{ ok: true }` |
| `click` | `{ x: int, y: int, button?: "left"\|"right"\|"middle" }` | `{ ok: true }` |
| `focus_window` | `{ window_id: int }` | `{ ok: true }` |

`focus_window` resolves `window_id` → `WindowInfo` via `WindowInspector` (like
`capture_window`); unknown id → `WindowNotFound`. All run on `spawn_blocking` and
map errors via `to_mcp_error`. A shared `AckOutput { ok: bool }` result type.

---

## 3. Testing

| Test | Where |
|---|---|
| Gate: `allow_act=false` → act tools absent from the router; `true` → present | unit (handler) |
| Each act tool handler forwards to `InputController` (mock records the call) | unit (handler) |
| `focus_window` unknown id → `WindowNotFound` | unit (mock) |
| Gate resolution (flag/env truthy parsing) | unit (main helper) |
| `clipboard_write` live (arboard round-trip via read_clipboard) | Linux box ✅ |
| `focus_window` live (AT-SPI grab_focus on a real window) | Linux box ✅ |
| `type_text`/`click` live | Linux X11 + macOS; on Wayland asserts attempt/So-error |

### Verification matrix (this environment = GNOME/Wayland)

- ✅ Here: gate on/off (unit + e2e `tools/list`), `clipboard_write`,
  `focus_window` (AT-SPI grab_focus), all handler/mock tests, gate parsing.
- ⚠️ Not here: `type_text`/`click` real injection on Wayland is compositor-limited
  (best-effort; asserts it returns Ok or an actionable error, not a panic);
  macOS act paths (no Mac). Flagged in the plan; validated by cross-compile +
  mock-exercised handler logic.

---

## 4. Risks

1. **Prompt-injection reaching act tools (highest, mitigated by design).** Act
   tools are unmounted when the gate is off — not reachable. The gate is
   operator-controlled at launch, never agent-controlled at runtime.
2. **enigo on Wayland.** May not inject into native Wayland windows on GNOME.
   Mapped to an actionable error; documented. `clipboard_write`/`focus_window`
   are unaffected (arboard + AT-SPI).
3. **enigo system deps.** Possible libei/xdo build requirement → feature-gate like
   capture/OCR, resolved in the plan.
4. **macOS act code unverifiable here.** Kept minimal (enigo for input, small AX
   FFI for focus); validated by inspection + cross-compile feature check.
