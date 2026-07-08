# Design Spec: vantage-mcp — Linux read slice (Spec A)

| Field | Value |
|---|---|
| Source PRD | `PRD-desktop-capture-mcp.md` |
| Predecessor spec | `2026-07-08-vantage-mcp-phase0-1-macos-design.md` |
| Scope of this spec | Linux backend mirroring the Phase 1 macOS read slice + cross-platform binary restructure |
| Target platform | Linux (X11 **and** Wayland, runtime-detected), plus preserving the existing macOS build |
| Transport | stdio (unchanged) |
| Status | Approved design, ready for implementation plan |
| Date | 2026-07-08 |

This is **sub-project A** of a three-part effort to complete the project roadmap
(user directive: finish the whole roadmap, Linux first):

- **Spec A (this doc)** — Linux read slice + cross-platform binary. Priority.
- **Spec B** — richer capture surface (`capture_window`, `list_displays`) across macOS + Linux.
- **Spec C** — gated act tools (`InputController`: clipboard-write / type / click / focus) behind a policy gate, across macOS + Linux.

B and C each get their own spec → plan → implementation cycle and both build on
the backend foundation laid here.

---

## 1. Objective

Deliver a working, verifiable Linux read slice that is **behaviourally identical**
to the macOS Phase 1 slice: an agent can enumerate windows, read a window's text
via the accessibility tree, OCR a screen region to text, and read the clipboard —
over stdio, logging strictly on stderr, text-first by default.

The same `vantage-mcp` binary must build on both macOS and Linux, selecting its
backend at compile time. On Linux the backend must work on both X11 and Wayland
sessions, detecting the session (or delegating detection to the underlying
libraries) at runtime.

### Success criteria

1. `cargo build --workspace` succeeds on Linux **and** remains green on macOS. The
   `vantage-mcp` binary links the Linux backend on Linux and the macOS backend on
   macOS; neither pulls the other's OS dependencies.
2. On a Linux desktop session (GNOME/Wayland is the reference environment), an agent
   reads the text content of a native window in a single tool round-trip via AT-SPI.
3. `capture_region` returns real pixels/OCR text on both X11 and Wayland.
4. `read_clipboard` reads text and image from the Linux clipboard on both session types.
5. Missing accessibility (a11y bus down) or denied screen capture (portal denial)
   returns a **distinct, actionable** error — never a silent hang or generic failure.
6. Nothing but the JSON-RPC stream is ever written to stdout (unchanged invariant).
7. The `core` contract (traits, value types, `CaptureError` enum) is **unchanged** —
   the Linux backend implements exactly the same traits the macOS backend does.

### Non-goals (deferred to Spec B / C or later)

- `capture_window`, `list_displays`, multi-display enumeration (Spec B).
- Any act/write tool — clipboard write, type, click, focus (Spec C).
- Wayland-native window enumeration beyond what AT-SPI provides (e.g. wlroots
  `foreign-toplevel`). AT-SPI is the portable window source for this slice.
- OCR engines other than Tesseract.

---

## 2. Architecture

### 2.1 Crate layout (after this spec)

```
crates/
  core/                     # UNCHANGED — traits, types, CaptureError
  platform/
    macos/                  # existing; add a uniform `backends()` fn
    linux/                  # NEW: vantage-platform-linux
      src/
        lib.rs              # cfg-gated modules + pub fn backends()
        windows.rs          # LinuxWindowInspector  (AT-SPI2)
        capture.rs          # LinuxScreenCapturer   (xcap, portal fallback)
        ocr.rs              # LinuxTextRecognizer    (Tesseract)
        clipboard.rs        # LinuxClipboard         (arboard)
      tests/
        windows_live.rs     # #[ignore]
        capture_live.rs     # #[ignore]
        ocr_live.rs         # #[ignore]  (reuses ../macos fixture concept)
        clipboard_live.rs   # #[ignore]
        fixtures/hello.png   # OCR fixture (copy of the macOS one)
  mcp-server/               # main.rs + Cargo.toml become platform-selecting
```

`core` is untouched. `mcp-server` gains cfg-gated backend selection. `platform/macos`
gains one function (`backends()`) and is otherwise untouched.

### 2.2 Cross-platform binary selection

Each platform crate exposes one uniform constructor so `main.rs` never names a
concrete `Mac*`/`Linux*` type:

```rust
// in each platform crate (macos and linux), behind its os cfg:
pub fn backends() -> (
    Arc<dyn WindowInspector>,
    Arc<dyn ScreenCapturer>,
    Arc<dyn TextRecognizer>,
    Arc<dyn ClipboardAccess>,
);
```

`crates/mcp-server/Cargo.toml`:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
vantage-platform-macos = { path = "../platform/macos" }

[target.'cfg(target_os = "linux")'.dependencies]
vantage-platform-linux = { path = "../platform/linux" }
```

`crates/mcp-server/src/main.rs`:

```rust
#[cfg(target_os = "macos")]
use vantage_platform_macos as backend;
#[cfg(target_os = "linux")]
use vantage_platform_linux as backend;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("vantage-mcp supports macOS and Linux only");

// ...
let (windows, capturer, ocr, clipboard) = backend::backends();
```

Because each platform crate's OS-specific dependencies live under a
`[target.'cfg(...)'.dependencies]` table, `cargo build --workspace` on Linux builds
`vantage-platform-macos` as an empty lib (its modules are `#[cfg(target_os="macos")]`)
and vice-versa. This is already the observed behaviour for the macOS crate on Linux.

### 2.3 Session detection

Runtime X11-vs-Wayland divergence is confined to **capture** (§2.5). Window/text
(AT-SPI), clipboard (arboard), and OCR (Tesseract) are session-agnostic at our layer —
the libraries handle the transport. Where capture needs to branch, it reads
`XDG_SESSION_TYPE` (and presence of `WAYLAND_DISPLAY`) at runtime.

### 2.4 Window inspection — AT-SPI2 (`windows.rs`)

AT-SPI2 is the Linux mirror of macOS's Accessibility (AX) API and works over D-Bus on
both X11 and Wayland. Crate: **`atspi`** (async, built on `zbus`).

**Async→sync bridge.** The `WindowInspector` trait is synchronous and is already
invoked from `tokio::task::spawn_blocking` in the handler. `LinuxWindowInspector`
owns a dedicated **current-thread tokio runtime** and calls `runtime.block_on(...)`
inside its trait methods. Because those methods run on a `spawn_blocking` thread
(not a runtime worker thread), a nested `block_on` on a private runtime is safe.
*Risk note (spike):* if the `atspi` async surface proves awkward, fall back to
`zbus::blocking` proxies speaking the AT-SPI interfaces directly. The choice is
confined to `windows.rs` because it sits behind the `WindowInspector` trait.

**`list_windows(filter)`**:
1. Connect to the a11y bus; walk the AT-SPI desktop → applications → frames (role
   `FRAME`/`WINDOW`).
2. Per frame: `app` = owning application accessible's name; `title` = frame name;
   `bounds` = `Component.GetExtents(ATSPI_COORD_TYPE_SCREEN)`; `focused` = `GetState`
   contains `ACTIVE`; on-screen = state contains `SHOWING`/`VISIBLE`.
3. `on_screen_only` filters by the showing/visible state; `app_filter` exact-matches `app`.
4. `window_id` (u32) = a stable hash (e.g. FNV-1a) of `format!("{bus_name}\u{0}{object_path}")`.

**`read_window_text(window_id, depth)`**:
1. Re-enumerate frames exactly as above; hash each and match `window_id`
   (stateless resolve-by-id, mirroring how the macOS backend re-resolves an id → pid).
   First match wins on the astronomically unlikely hash collision (documented).
2. No match → `CaptureError::WindowNotFound(window_id)`.
3. Depth-first walk the matched frame's accessible subtree up to `depth` and a node
   budget (e.g. 2000 nodes), collecting `Text` interface content and `Name`/`Description`
   string attributes; join with newlines. Set `truncated = true` when either bound stops
   the walk.
4. A11y bus unreachable / AT-SPI disabled → `CaptureError::AccessibilityPermissionDenied`.

### 2.5 Screen capture — xcap (`capture.rs`)

Primary path is **`xcap`** (already the macOS capture dependency), which abstracts X11
and Wayland (portal/pipewire). Logic mirrors the macOS `capture.rs`: enumerate monitors,
pick the one containing the region's top-left corner, capture, translate global→monitor-local
coords, crop to `Bounds`, return RGBA8. Zero/out-of-range region → `InvalidBounds`.

*Risk note (spike, verifiable on the reference box):* if xcap's Wayland path returns black
frames or fails without a distinguishable error on GNOME/Mutter, add a runtime fallback:
when `XDG_SESSION_TYPE=wayland`, capture via the **`ashpd`** portal
(`org.freedesktop.portal.Screenshot`) — take a full-screen shot, then crop to `Bounds`.
Portal/permission failures map to `CaptureError::ScreenRecordingPermissionDenied`. The
primary-vs-fallback decision is made during the capture spike and recorded in the commit.

### 2.6 OCR — Tesseract (`ocr.rs`)

Crate **`tesseract`** (safe binding to libtesseract/leptonica). `recognize(&RgbaImage)`
feeds the raw RGBA buffer to Tesseract via `set_frame`/`set_image` (width, height,
bytes-per-pixel = 4, bytes-per-line = width*4), then returns `get_text()`.

**System dependency:** `libtesseract-dev libleptonica-dev tesseract-ocr-eng`. Not present
on the reference box; installed during implementation to run the live test. If the library
or `eng` traineddata is missing at runtime, return
`CaptureError::Internal("Tesseract/…")` with an actionable "install …" message
(surfaced via `error_map`).

### 2.7 Clipboard — arboard (`clipboard.rs`)

`arboard` handles X11 and Wayland internally. Near 1:1 with the macOS `clipboard.rs`:
`prefer=Text` returns text if present else tries image else `Empty`; `prefer=Image` the
converse. Image comes back as RGBA8 → wrapped in `RgbaImage`.

### 2.8 Error model

`CaptureError` and `ErrorKind` are **unchanged** — the six variants are platform-neutral
and sufficient. Only the human-facing **remediation text in `error_map.rs`** becomes
platform-aware (cfg-gated), so a Linux `AccessibilityPermissionDenied` explains the a11y
bus / accessibility settings instead of macOS System Settings, and a
`ScreenRecordingPermissionDenied` explains portal/screen-capture permission. Keeping the
enum stable preserves it as the shared cross-platform contract.

Linux → variant mapping:

| Situation | Variant |
|---|---|
| a11y bus unreachable / AT-SPI unavailable | `AccessibilityPermissionDenied` |
| screen-capture portal denied / capture blocked | `ScreenRecordingPermissionDenied` |
| `window_id` not resolvable | `WindowNotFound(id)` |
| region zero-size / outside all monitors | `InvalidBounds(bounds)` |
| tesseract lib/data missing, xcap/portal internal failure | `Internal(msg)` |

---

## 3. Data flow (unchanged from macOS slice)

The handler layer (`mcp-server`) is untouched behaviourally: each `#[tool]` method
clones the relevant `Arc<dyn Trait>` backend, runs the blocking backend call on
`spawn_blocking`, maps `CaptureError` via `to_mcp_error`, and returns text-first output.
Only the concrete backend injected in `main` differs by platform.

---

## 4. Testing

| Layer | What | Runs where |
|---|---|---|
| core unit | unchanged | anywhere |
| handler mock tests | unchanged (platform-agnostic mocks) | anywhere |
| `boot.rs` integration | now **builds and runs on Linux** — first real cross-platform proof | Linux (here) + macOS |
| `windows_live.rs` `#[ignore]` | AT-SPI list + text against the live session | Linux desktop session |
| `capture_live.rs` `#[ignore]` | xcap region capture returns a correctly-sized RGBA buffer | Linux desktop session |
| `ocr_live.rs` `#[ignore]` | Tesseract recovers "HELLO" from the committed fixture | Linux w/ tesseract installed |
| `clipboard_live.rs` `#[ignore]` | arboard text round-trip | Linux desktop session |

Live tests follow the macOS convention: `#[ignore]` with a reason string and a
copy-paste run command in the file header.

### Verification matrix (honesty about this environment)

- ✅ Verifiable on the reference box (GNOME/Wayland): workspace build, all non-ignored
  tests, `boot.rs`, and the four `#[ignore]` live tests (OCR after installing tesseract;
  capture spike confirms xcap-vs-portal here).
- ⚠️ macOS: changes are additive only (`backends()` fn + cfg deps; no Mac logic touched).
  Cannot be compiled/tested here without a macOS machine — validated by cfg-correctness
  review and by the fact that no macOS source file's behaviour changes. Flagged in the
  plan's final task.

---

## 5. Dependencies added (Linux target only)

Under `crates/platform/linux/Cargo.toml` `[target.'cfg(target_os = "linux")'.dependencies]`:

- `atspi` — AT-SPI2 client (window enumeration + accessibility text).
- `zbus` — D-Bus (transitive via atspi; direct if the blocking fallback is used).
- `tokio` — current-thread runtime for the async→sync bridge (features: `rt`).
- `xcap` — screen capture (shared with macOS).
- `ashpd` — portal fallback for Wayland capture (only if the spike requires it).
- `tesseract` — OCR binding to libtesseract/leptonica.
- `arboard` — clipboard.
- `image` — buffer handling (workspace dep).

Exact versions are pinned during implementation against Rust 1.95 and each other; the
plan's first Linux task resolves the dependency graph.

---

## 6. Open risks

1. **AT-SPI reliability (highest).** Analogous to the macOS AX spike. Async→sync bridge,
   window-id hashing, and subtree text extraction are the hard parts. Mitigation: spike
   task with the `zbus::blocking` fallback, all confined to `windows.rs`.
2. **xcap on Wayland.** May need the `ashpd` portal fallback. Directly testable on the
   reference box, so the risk is resolved during implementation, not left open.
3. **Tesseract packaging.** Requires a system library + traineddata. Handled by an
   actionable runtime error and documented install step; not bundled.
4. **a11y bus not enabled.** Some minimal sessions ship AT-SPI off. Surfaced as an
   actionable `AccessibilityPermissionDenied`, matching the macOS permission posture.
