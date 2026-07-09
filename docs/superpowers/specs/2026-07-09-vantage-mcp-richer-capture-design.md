# Design Spec: vantage-mcp — Richer capture surface (Spec B)

| Field | Value |
|---|---|
| Source PRD | `PRD-desktop-capture-mcp.md` |
| Predecessor specs | Phase 0/1 macOS; Linux read-slice (Spec A) |
| Scope | Two new read tools — `list_displays` and `capture_window` — on macOS and Linux |
| Target platform | macOS + Linux (X11 fully; Wayland: `list_displays` yes, `capture_window` `Unsupported`) |
| Transport | stdio (unchanged) |
| Status | Approved design, ready for implementation plan |
| Date | 2026-07-09 |

Sub-project **B** of the three-part roadmap effort (A: Linux read-slice ✅ merged;
B: this; C: gated act tools). Builds on the Spec A backend foundation.

---

## 1. Objective

Broaden the capture surface beyond a single screen region:

- **`list_displays`** — enumerate monitors (id, name, bounds, scale, primary),
  so an agent can target a specific display or reason about multi-monitor layout.
- **`capture_window`** — capture one window by `window_id` (from `list_windows`),
  returning text-first output like `capture_region`. Handles occlusion / true
  per-window grab rather than "whatever is at these screen coordinates".

### Success criteria

1. `list_displays` returns every monitor with correct bounds/scale on macOS and
   Linux (X11 + Wayland). Verifiable on the Linux reference box.
2. `capture_window` captures the correct window on macOS and Linux/X11, returning
   the same `{ text, image }` shape as `capture_region` (text-first default).
3. On Linux/**Wayland**, `capture_window` returns a distinct, actionable error
   (the compositor does not permit capturing arbitrary application windows) —
   never a wrong-region capture or a hang.
4. Unknown `window_id` → `WindowNotFound`. Core contract stays coherent; the two
   new `ScreenCapturer` methods are implemented by every backend and every mock.
5. `capture_region`, the four Spec A tools, and the text-first / stdout-sacred
   invariants are unchanged.

### Non-goals

- Video / continuous capture, cursor capture, per-window capture on Wayland.
- Any act/write tool (Spec C).
- New OCR engines or a new capture library (reuse xcap + the existing OCR/image path).

---

## 2. Architecture

### 2.1 Core extensions (`crates/core`)

Spec A froze the core; Spec B deliberately extends it (additively):

**New value type** (`types.rs`):

```rust
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DisplayInfo {
    pub display_id: u32,
    pub name: String,
    pub bounds: Bounds,
    pub scale_factor: f32,
    pub is_primary: bool,
}
```

**Extend the `ScreenCapturer` trait** (`traits.rs`) — two new methods, so the
handler keeps its single `capturer: Arc<dyn ScreenCapturer>` (no fifth backend):

```rust
pub trait ScreenCapturer: Send + Sync {
    fn capture_region(&self, bounds: Bounds) -> Result<RgbaImage, CaptureError>;
    // NEW:
    fn list_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError>;
    fn capture_window(&self, target: &WindowInfo) -> Result<RgbaImage, CaptureError>;
}
```

`capture_window` takes the already-resolved `WindowInfo` (not a bare id) so each
backend has the identity data it needs — `window_id` (CGWindowID on macOS),
`app`, `title`, `bounds` — without the capturer having to reverse the
platform-specific id. `CaptureError` and `ErrorKind` are unchanged.

Every implementor updates: `MacScreenCapturer`, `LinuxScreenCapturer`, the
Linux `capture_stub`, and the `handler.rs` test doubles (`NoScreen`,
`FakeScreen`, `LargeFakeScreen`).

### 2.2 `list_displays` tool

Backend (both platforms, identical logic via xcap):

```
Monitor::all() -> for each: DisplayInfo {
    display_id = m.id(), name = m.name(), bounds from m.x()/y()/width()/height(),
    scale_factor = m.scale_factor(), is_primary = m.is_primary()
}
```

Handler tool `list_displays` (no args) → `Json<ListDisplaysResult { displays: Vec<DisplayInfo> }>`
(object-root wrapper, per the rmcp `outputSchema`-must-be-object rule already
noted for `list_windows`). Maps errors via `to_mcp_error`, runs on `spawn_blocking`.

### 2.3 `capture_window` tool

Handler tool `capture_window { window_id: u32, output?: "text"|"image"|"both",
max_dimension?: u32 }` → `Json<CaptureOutput>` (the same struct `capture_region`
returns).

Flow (in the handler, which holds both `windows` and `capturer`):

1. Resolve: `let info = self.windows.list_windows(all)?.into_iter().find(|w| w.window_id == window_id).ok_or(WindowNotFound)`.
2. `let frame = self.capturer.capture_window(&info)?`.
3. Reuse the exact `capture_region` mode logic (`Text`/`Image`/`Both`, default
   text, `max_dimension` cap 1024) over `frame` — OCR via `self.ocr`, downscale +
   `rgba_to_base64_png` via `image_out`. Factor the shared post-capture logic
   (frame → `CaptureOutput`) into one helper used by both `capture_region` and
   `capture_window` to avoid duplication.

Backends:

- **macOS** (`MacScreenCapturer::capture_window`): `xcap::Window::all()`, find the
  window whose `id() == target.window_id` (xcap's macOS window id is the
  CGWindowID our `window_id` already is). `capture_image()` → RGBA8. Not found →
  `WindowNotFound(target.window_id)`. Capture failure classified as today.
- **Linux/X11** (`LinuxScreenCapturer::capture_window`): if `XDG_SESSION_TYPE`
  is `wayland` (or `WAYLAND_DISPLAY` set) → `CaptureError::Unsupported(...)` with
  an actionable message. Otherwise `xcap::Window::all()`, match by
  `app_name() == target.app && title() == target.title`; if several match,
  prefer one whose `x()/y()/width()/height()` equals `target.bounds`; else first.
  None → `WindowNotFound`. `capture_image()` → RGBA8.
- **Linux stub** (`capture_stub.rs`): `Unsupported` (as today).

### 2.4 Error mapping

No new `CaptureError` variants. Wayland `capture_window` uses `Unsupported`
(maps to `invalid_request` "unsupported on this platform: …"); a not-found
window uses `WindowNotFound` → `invalid_params`. `error_map.rs` needs no change
beyond what Spec A did.

---

## 3. Testing

| Test | Where |
|---|---|
| `DisplayInfo` shape / `list_displays` handler with a mock capturer | anywhere (unit) |
| `capture_window` handler: resolves id → WindowInfo → capturer, reuses mode logic; unknown id → error | anywhere (mock) |
| `list_displays` live (real monitors, bounds/scale) | Linux box ✅ + macOS |
| `capture_window` live | Linux **X11** + macOS |
| `capture_window` returns Unsupported on Wayland | Linux Wayland box ✅ |
| Existing Spec A tests + mocks updated for the 2 new trait methods, still green | Linux box ✅ |

### Verification matrix (this environment = GNOME/Wayland)

- ✅ Here: `list_displays` unit + live; `capture_window` handler/mock logic;
  `capture_window` Wayland-Unsupported path; full workspace build + tests + clippy.
- ⚠️ Not here: `capture_window` live on X11 and macOS (no X11 session / no Mac).
  Written to mirror verified patterns; validated by cross-compile feature check
  and the shared handler logic that *is* exercised by mocks. Flagged in the plan.

---

## 4. Risks

1. **xcap macOS window id ≠ CGWindowID.** If xcap's macOS `Window::id()` is not
   the CGWindowID, the macOS `id()==window_id` match fails. Mitigation: fall back
   to matching by `pid`/`title`/`bounds` like the X11 path; decided when the
   macOS path can be run. Documented in the plan's macOS task.
2. **X11 title/app matching ambiguity.** Two same-app windows with identical
   titles: bounds tie-break, else first — documented. Rare; acceptable for a read
   tool.
3. **xcap `Window` capture on Wayland.** We short-circuit to `Unsupported` before
   calling xcap, so no accidental wrong-region capture.
