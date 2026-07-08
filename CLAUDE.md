# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`vantage-mcp` is a Model Context Protocol (MCP) server that gives an LLM agent
**read-only** access to the desktop over stdio: enumerate windows, read a
window's accessibility text, capture a screen region (OCR text or image), and
read the clipboard. This is the Phase 0/1 build: **macOS only, no "act" tools**
(no typing/clicking/clipboard writes). See `PRD-desktop-capture-mcp.md` and
`docs/superpowers/specs/` / `docs/superpowers/plans/` for the full spec and
task plan; `README.md` documents the tool surface and required macOS
permissions.

## Commands

```bash
cargo build --release              # binary → target/release/vantage-mcp
cargo test                         # all non-ignored tests (workspace)
cargo test -p vantage-mcp-server   # server unit + boot tests
cargo test --test boot             # the stdio handshake integration test
cargo fmt                          # rustfmt (applied per-task; keep clean)
cargo clippy --all-targets

# Live macOS tests are #[ignore]d (need a session + TCC permissions). Run one:
cargo test -p vantage-platform-macos --test windows_live -- --ignored
cargo test -p vantage-platform-macos --test ocr_live      -- --ignored   # Vision only, no permission
```

Toolchain is pinned to Rust **1.95** via `rust-toolchain.toml`.

## Architecture

Three-crate Cargo workspace with a strict dependency direction —
`core ← platform/macos ← mcp-server`:

- **`crates/core`** (`vantage_core`) — platform-agnostic contract. Defines the
  four capability **traits** (`WindowInspector`, `ScreenCapturer`,
  `TextRecognizer`, `ClipboardAccess` in `traits.rs`), the value **types**
  (`types.rs`), and the domain error enum `CaptureError` + its coarse
  `ErrorKind` (`error.rs`). No OS dependencies.

- **`crates/platform/macos`** (`vantage_platform_macos`) — the macOS
  implementations (`Mac*` structs), one file per capability: `windows.rs`
  (CGWindowList + AXUIElement tree walk), `capture.rs` (xcap), `ocr.rs`
  (Vision `VNRecognizeTextRequest`), `clipboard.rs` (arboard). Everything is
  gated behind `#[cfg(target_os = "macos")]` in `lib.rs` — so on a non-macOS
  host `vantage-core` and this crate's *lib* still compile (the modules vanish),
  but the `vantage-mcp` **binary does not**: `main.rs` names `backend::Mac*`
  unconditionally, so it only builds on macOS. Build/test the full workspace on
  macOS.

- **`crates/mcp-server`** (bin `vantage-mcp`) — the MCP/JSON-RPC layer, built
  on `rmcp` 2.1.0. `handler.rs` holds `Vantage`, which stores the four
  backends as `Arc<dyn Trait>` and exposes the `#[tool]` methods. `main.rs`
  constructs the concrete `Mac*` backends and injects them.

**Key design invariants** (violate these and you break the point of the project):

- **Dependency injection via trait objects.** `Vantage` never names a `Mac*`
  type — it holds `Arc<dyn WindowInspector>` etc. Tests inject mocks (see the
  `MockWindows` / `NoScreen` / `FakeOcr` fakes in `handler.rs` tests); the
  Linux/Wayland backends will slot in the same way. Keep new capability logic
  behind a core trait, not inline in the handler.

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
  "grant it in System Settings → …" message and must **never** collapse into
  `internal_error`. When adding a backend failure mode, add a `CaptureError`
  variant + `ErrorKind` + a `to_mcp_error` arm rather than returning a raw
  string.

- **Blocking backend calls run on `spawn_blocking`.** The `Mac*` backends are
  synchronous FFI; every `#[tool]` method wraps the call in
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
