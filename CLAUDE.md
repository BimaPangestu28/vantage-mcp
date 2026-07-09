# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`vantage-mcp` is a Model Context Protocol (MCP) server that gives an LLM agent
**read-only** access to the desktop over stdio: enumerate windows and displays,
read a window's accessibility text, capture a screen region or a whole window
(OCR text or image), and read the clipboard. This is the read slice on **macOS and Linux** (X11 +
Wayland), **no "act" tools** (no typing/clicking/clipboard writes). See
`PRD-desktop-capture-mcp.md` and `docs/superpowers/specs/` /
`docs/superpowers/plans/` for the full spec and task plans; `README.md`
documents the tool surface and required per-OS permissions.

## Commands

```bash
cargo build --release              # binary → target/release/vantage-mcp
cargo test                         # all non-ignored tests (workspace)
cargo test -p vantage-mcp-server   # server unit + boot tests
cargo test --test boot             # the stdio handshake integration test
cargo fmt --all                    # rustfmt (applied per-task; keep clean)
cargo clippy --workspace --all-targets

# Linux without the capture/OCR system libs (see docs/agent-registration.md):
cargo build --no-default-features          # capture + OCR become Unsupported stubs
cargo test  --no-default-features          # everything else builds + tests

# Live tests are #[ignore]d (need a desktop session + permissions). Run one:
cargo test -p vantage-platform-macos --test windows_live -- --ignored
cargo test -p vantage-platform-linux --test windows_live   -- --ignored  # AT-SPI, no libs needed
cargo test -p vantage-platform-linux --test clipboard_live -- --ignored  # arboard, no libs needed
cargo test -p vantage-platform-linux --test capture_live   -- --ignored  # needs capture feature + libs
cargo test -p vantage-platform-linux --test ocr_live       -- --ignored  # needs ocr feature + libtesseract
```

Toolchain is pinned to Rust **1.95** via `rust-toolchain.toml`.

## Architecture

Four-crate Cargo workspace with a strict dependency direction —
`core ← platform/{macos,linux} ← mcp-server`:

- **`crates/core`** (`vantage_core`) — platform-agnostic contract. Defines the
  four capability **traits** (`WindowInspector`, `ScreenCapturer` — which now
  covers `capture_region`, `capture_window`, and `list_displays` —
  `TextRecognizer`, `ClipboardAccess` in `traits.rs`), the value **types**
  (`types.rs`, incl. `DisplayInfo`), and the domain error enum `CaptureError` +
  its coarse `ErrorKind` (`error.rs`). No OS dependencies. **Near-frozen
  contract** — both platform backends implement exactly these traits; extend it
  only deliberately (as Spec B did), updating every backend + mock together.

- **`crates/platform/macos`** (`vantage_platform_macos`) — the `Mac*` structs,
  one file per capability: `windows.rs` (CGWindowList + AXUIElement), `capture.rs`
  (xcap), `ocr.rs` (Vision), `clipboard.rs` (arboard).

- **`crates/platform/linux`** (`vantage_platform_linux`) — the `Linux*` structs:
  `windows.rs` (AT-SPI2 over D-Bus, via `atspi`/`zbus` on a private tokio
  runtime — the sync trait methods `block_on` it), `atspi_conn.rs` (window-id
  FNV-1a hashing), `capture.rs` (xcap, X11+Wayland), `ocr.rs` (Tesseract),
  `clipboard.rs` (arboard). Capture and OCR are **optional Cargo features**
  (`capture`, `ocr`, both default-on) because they need system libs; when off,
  `capture_stub.rs`/`ocr_stub.rs` return an actionable `Unsupported`.

  Each platform crate gates everything behind its `#[cfg(target_os = "...")]`
  and exposes one uniform `pub fn backends() -> (Arc<dyn WindowInspector>, …)`.
  The opposite-platform crate compiles to an empty lib, so `cargo build
  --workspace` works on both OSes.

- **`crates/mcp-server`** (bin `vantage-mcp`) — the MCP/JSON-RPC layer, built
  on `rmcp` 2.1.0. `handler.rs` holds `Vantage`, which stores the four backends
  as `Arc<dyn Trait>` and exposes the six `#[tool]` methods (`list_windows`,
  `read_window_text`, `capture_region`, `capture_window`, `list_displays`,
  `read_clipboard`); the two capture tools share `parse_mode`/`clamp_max_dim`/
  `frame_to_output` (the OCR + downscale + PNG pipeline). `main.rs` selects the
  backend by `cfg(target_os)` (`compile_error!` on unsupported OSes) and calls
  `backend::backends()` — it never names a concrete `Mac*`/`Linux*` type. The
  server forwards `capture`/`ocr` features to the Linux crate, so
  `--no-default-features` yields a server buildable without GUI/OCR system libs.

**Key design invariants** (violate these and you break the point of the project):

- **Dependency injection via trait objects.** `Vantage` never names a concrete
  `Mac*`/`Linux*` type — it holds `Arc<dyn WindowInspector>` etc. Tests inject
  mocks (see the `MockWindows` / `NoScreen` / `FakeOcr` fakes in `handler.rs`
  tests); both platform backends slot in via `backends()`. Keep new capability
  logic behind a core trait, not inline in the handler.

- **stdout is sacred — JSON-RPC only.** All logging goes to **stderr** via
  `tracing` (`logging::init()`). Never `println!` or write to stdout. The
  `boot.rs` test asserts every stdout line is a `"jsonrpc":"2.0"` object and
  will fail on any stray output.

- **Text-first / token-frugal.** `read_window_text` never returns pixels;
  `capture_region` defaults to `output:"text"` (OCR only, no image). Images
  are always downscaled to `max_dimension` (default+cap 1024). Preserve these
  defaults — they exist to keep agent token cost low.

- **Errors map through `CaptureError` → `to_mcp_error` (`error_map.rs`).**
  Permission-denied variants become `invalid_request` with an actionable
  remediation message and must **never** collapse into `internal_error`. The
  `CaptureError` enum is platform-neutral (frozen); only the remediation *text*
  in `error_map.rs` is `cfg`-gated per OS (macOS System Settings vs Linux
  AT-SPI bus / screen-capture portal). When adding a backend failure mode, map
  it onto an existing variant, or add a `CaptureError` variant + `ErrorKind` +
  a `to_mcp_error` arm — never a raw string.

- **Blocking backend calls run on `spawn_blocking`.** The backends are
  synchronous (FFI on macOS; on Linux the AT-SPI backend `block_on`s its own
  runtime); every `#[tool]` method wraps the call in
  `tokio::task::spawn_blocking` so the async runtime isn't blocked. Follow this
  pattern for new tools.

- **rmcp quirk:** a tool's `outputSchema` root must be a JSON **object**.
  Returning a bare `Vec<_>` panics at `tools/list` schema generation — wrap
  collections in a struct (see `ListWindowsResult { windows }`).

## Conventions

- Follows the global standards in `~/.claude/CLAUDE.md`: no `unwrap()`/`panic!()`
  in production paths (tests may), `anyhow` at the binary boundary +
  `thiserror` for the domain error, conventional commit messages
  (`feat:`/`fix:`/`refactor(scope):`), tests for business logic.
- Development is **TDD, task-by-task** per the plan in `docs/superpowers/plans/`.
  Live/permission-requiring tests are `#[ignore]`d with a reason string and a
  copy-paste run command in the file header — mirror that when adding one.
