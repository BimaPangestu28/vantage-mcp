# Design Spec: vantage-mcp — Phase 0 + Phase 1 (macOS-first)

| Field | Value |
|---|---|
| Source PRD | `PRD-desktop-capture-mcp.md` |
| Scope of this spec | Phase 0 (skeleton) + Phase 1 (MVP read slice), macOS only |
| Target platform | macOS 12.3+ |
| Transport | stdio |
| Status | Approved design, ready for implementation plan |
| Date | 2026-07-08 |

---

## 1. Objective

Deliver a working, verifiable MVP read slice of the desktop-capture MCP server on macOS:
an agent can enumerate windows, read a window's text via the accessibility tree, OCR a
screen region to text, and read the clipboard — all over stdio, with logging strictly on
stderr and a text-first token-economy default.

Linux/X11, image-output polish, and gated act tools are explicitly **out of scope** for
this spec; they follow as later phases and must not require reworking the core.

### Success criteria (from PRD §12, scoped to this phase)

1. Agent reads the text content of an arbitrary native window on macOS in a single tool round-trip.
2. Default text-first path keeps a typical `read_window_text` / `capture_region` result to a few thousand tokens (no accidental full-screen image dumps).
3. Permission-denied states return a distinct, actionable error 100% of the time — never a silent hang or a generic failure.
4. Nothing but the JSON-RPC stream is ever written to stdout.

---

## 2. Workspace layout

```
vantage-mcp/
  Cargo.toml                # [workspace]
  crates/
    mcp-server/             # binary: rmcp handlers, stdio transport, error mapping, wiring
    core/                   # library: capability traits, orchestration, error types
    platform/
      macos/                # library: trait impls (CGWindowList, AXUIElement, Vision, arboard, xcap)
```

Dependency direction (enforced):

- `mcp-server` → `core` (traits only) + `platform-macos` (constructed once in `main`, injected as `dyn Trait`).
- `platform-macos` → `core` (implements its traits).
- `core` depends on **no** platform crate.

This keeps tool handlers platform-agnostic. Adding Linux/X11 later means a new
`platform/linux-x11` crate implementing the same traits and a one-line swap in `main`.

---

## 3. Capability traits (`core`)

Defined in `core`, implemented in `platform/macos`. All are object-safe (`dyn`-compatible).
Platform calls are synchronous; async orchestration lives in `mcp-server` via `spawn_blocking`.

```rust
pub trait WindowInspector: Send + Sync {
    fn list_windows(&self, filter: WindowFilter) -> Result<Vec<WindowInfo>, CaptureError>;
    fn read_window_text(&self, window_id: WindowId, depth: Option<u32>) -> Result<WindowText, CaptureError>;
}

pub trait ScreenCapturer: Send + Sync {
    fn capture_region(&self, bounds: Bounds) -> Result<RgbaImage, CaptureError>;
}

pub trait TextRecognizer: Send + Sync {
    fn recognize(&self, image: &RgbaImage) -> Result<String, CaptureError>;
}

pub trait ClipboardAccess: Send + Sync {
    fn read(&self, prefer: ClipboardPrefer) -> Result<ClipboardContent, CaptureError>;
    // write() defined but returns Unsupported in this phase (act tool, Phase 3)
}

// Defined for contract completeness; all methods return CaptureError::Unsupported in this phase.
pub trait InputController: Send + Sync { /* type_text, click, move_mouse, key_press, focus_window */ }
```

Supporting types (`core::types`): `WindowId`, `WindowInfo { window_id, app, title, bounds, focused }`,
`Bounds { x, y, width, height }`, `WindowFilter { app_filter, on_screen_only }`,
`WindowText { text, truncated }`, `RgbaImage { width, height, pixels }`,
`ClipboardPrefer { Text, Image }`, `ClipboardContent { kind, text, image }`.

---

## 4. MCP tool surface (this phase)

All read tools. Every capture tool defaults to the cheapest useful output.

| Tool | Params | Default | Returns |
|---|---|---|---|
| `list_windows` | `{ app_filter?, on_screen_only? }` | `on_screen_only=true` | `[{ window_id, app, title, bounds, focused }]` |
| `read_window_text` | `{ window_id, depth? }` | `depth` server-capped | `{ text, truncated }` |
| `capture_region` | `{ bounds, output?, max_dimension? }` | `output=text` | `{ text?, image? }` |
| `read_clipboard` | `{ prefer? }` | `prefer=text` | `{ kind, text?, image? }` |

### Behavior requirements

- `capture_region` with `output=text` (the default) runs OCR and returns **no pixels**.
- `capture_region` with `output=image`/`both` downscales the returned PNG so its largest
  side ≤ `max_dimension`; a server-side default cap (e.g. 1024) is enforced even when the
  caller omits `max_dimension`.
- `read_window_text` is the preferred, cheapest path for window content; `depth` is capped
  server-side to bound token cost, and `truncated=true` signals the cap was hit.
- Tool schemas are generated via `schemars` and surfaced through rmcp's tool macros.

---

## 5. Platform implementation (`platform/macos`)

| Capability | Implementation | Crate(s) |
|---|---|---|
| `list_windows` | `CGWindowListCopyWindowInfo` (on-screen list) → map fields; `focused` derived from frontmost app + main window | `core-graphics`, `objc2-app-kit` |
| `read_window_text` | Resolve `AXUIElement` for the window's pid, walk the AX tree depth-first to `depth`, collect `AXValue`/`AXTitle`/`AXDescription` text | `objc2`, `accessibility` / `accessibility-sys` |
| `capture_region` | Capture the display containing `bounds` via `xcap`, crop to `bounds`, return `RgbaImage` | `xcap`, `image` |
| OCR (`recognize`) | `VNRecognizeTextRequest` over the captured image, accurate recognition level, join observations | `objc2-vision`, `objc2-foundation` |
| `read_clipboard` | `arboard` text/image get | `arboard` |

**OCR fallback:** if Vision (`objc2-vision`) proves too costly to bind reliably, swap the
`TextRecognizer` impl for the pure-Rust `ocrs` crate. Because OCR sits behind the trait,
this swap touches only `platform/macos` and no handler or core logic.

**Threading:** AX, CoreGraphics, and Vision calls are synchronous and potentially
main-thread-sensitive. Handlers call them inside `tokio::task::spawn_blocking`. AX tree
reads and Vision requests are safe off the main thread; if any call requires the main
thread it is isolated in the platform crate, not leaked to handlers.

---

## 6. Error model

`core::CaptureError` — one enum, actionable variants mapped to MCP errors with a message
the agent can surface as a fix:

```rust
pub enum CaptureError {
    ScreenRecordingPermissionDenied,   // "Grant Screen Recording permission to this process"
    AccessibilityPermissionDenied,     // "Grant Accessibility permission to this process"
    WindowNotFound(WindowId),
    InvalidBounds(Bounds),             // region outside all display bounds
    Unsupported(String),               // capability not available in this phase/platform
    Internal(String),
}
```

Mapping rule: permission-denied variants never collapse into `Internal`. Detection uses the
platform signal (e.g. empty/failed `CGWindowList` capture attempt, `AXError` of
`kAXErrorAPIDisabled`/`NotAuthorized`) to distinguish "permission" from "not found".

---

## 7. Non-functional enforcement

- **Stdout is sacred.** `tracing_subscriber` configured with a **stderr** writer only. A
  startup self-check logs (to stderr) confirmation that the stdout guard is in place. No
  dependency is allowed to `println!`; the stdio transport owns stdout exclusively.
- **Token economy.** Text-first defaults (§4). Image output is opt-in and downscaled.
- **Graceful degradation.** Any unavailable capability returns `Unsupported` with a clear
  message rather than hanging or panicking.
- **Latency targets (from PRD §7):** `list_windows` / `read_window_text` well under 200 ms;
  `capture_region` + OCR under ~1 s. Not a gate for this phase, but the design (region-scoped
  capture, no full-screen default) is built to meet them.

---

## 8. Testing strategy

- **`core` unit tests (no OS):** mock backends implementing the traits. Cover output-mode
  selection (`text` vs `image` vs `both`), `max_dimension` downscaling math, `depth` capping
  / `truncated` flag, and `CaptureError` → MCP error mapping.
- **`platform/macos` integration tests:** marked `#[ignore]` (require TCC grants + a live
  display); run manually. Assert `list_windows` returns the test window, `read_window_text`
  returns non-empty text for a known app, `capture_region` + OCR reads known on-screen text.
- **End-to-end verification:** register the binary with a local MCP agent (Claude Code /
  Claude Desktop) over stdio; call `list_windows` then `read_window_text` and confirm a
  single-round-trip text result. This is the phase's definition of done.

---

## 9. Transport & permissions (this phase)

- **Transport:** stdio. The agent spawns the binary as a child process. `rmcp` with
  `server`, `transport-io`, `macros` features.
- **macOS permissions (dev posture):** Screen Recording + Accessibility are granted to the
  launching terminal/agent during development. Signed `.app` bundle / LaunchAgent daemon +
  local HTTP is deferred to Phase 3 (when act tools land). If capture/AX silently fail
  because TCC did not attach, that surfaces as a permission-denied `CaptureError`, not a hang.

---

## 10. Out of scope (later phases, must not require rework)

- Linux/X11 and Wayland backends (new platform crates behind the same traits).
- `capture_window`, `list_displays`, full image-output surface (Phase 2).
- Gated act tools + policy gate: `write_clipboard`, `focus_window`, `type_text`, `click`,
  `move_mouse`, `key_press` (Phase 3). `InputController`/`ClipboardAccess::write` exist as
  `Unsupported` stubs now so the contract is stable.
- macOS signing / entitlements, clipboard history, remote HTTP transport, Windows.

---

## 11. Open questions carried forward (not blocking this phase)

- Final product context / naming / license (PRD §13 Q2).
- Act-tool default posture — disabled vs confirm-required vs policy-file (PRD §13 Q3);
  decided when Phase 3 is specced.
- Whether AX text is reliable enough across apps or needs OCR as a standard fallback
  (PRD §13 Q4) — this phase gathers evidence via real end-to-end use.
