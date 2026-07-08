# vantage-mcp

## Overview

`vantage-mcp` is a Model Context Protocol (MCP) server that gives an LLM agent
read access to the desktop: enumerate windows, read a window's text content,
capture a screen region as OCR'd text (or an image), and read the clipboard.
It speaks MCP over stdio, so it can be registered with any MCP-capable agent
(Claude Code, Claude Desktop, etc.) as a subprocess.

This is a **Phase 0/1** build: macOS only, read-only. There are no "act"
tools (no typing, clicking, or clipboard writes) — see [Roadmap](#roadmap).

Every tool is **text-first by design**: `read_window_text` never returns
pixels, and `capture_region` defaults to OCR text with no image payload. This
keeps token cost low for the common case; ask for `output: "image"` or
`"both"` on `capture_region` only when visual layout actually matters.

## Features

- `list_windows` — enumerate on-screen windows (id, owning app, title,
  bounds, focus state).
- `read_window_text` — read a window's content via the macOS accessibility
  (AX) tree. Cheapest way to get a window's content; prefer this over a
  screenshot + OCR round trip.
- `capture_region` — capture a screen region; defaults to OCR text only, can
  optionally also/instead return a downscaled base64 PNG.
- `read_clipboard` — read the system clipboard (text by default, or an
  image as base64 PNG).

## Prerequisites

- macOS 12.3 or later.
- Rust 1.95 (pinned via `rust-toolchain.toml`).
- Two macOS permissions granted **to whatever process launches this binary**
  (your terminal, or the agent app itself if it spawns the process directly):
  - **Screen Recording** — required for `capture_region` and for window
    *titles* in `list_windows`. System Settings → Privacy & Security →
    Screen Recording.
  - **Accessibility** — required for `read_window_text`. System Settings →
    Privacy & Security → Accessibility.

  Without these, the affected tools return an actionable MCP error naming
  the missing permission and where to grant it — they do not hang or crash.
  See [docs/agent-registration.md](docs/agent-registration.md) for the exact
  steps and what the error looks like.

## Build

```bash
cargo build --release
```

The binary is produced at `target/release/vantage-mcp`.

## Usage

`vantage-mcp` speaks MCP over stdio: JSON-RPC requests in on stdin, responses
out on stdout, one message per line. It expects the standard MCP handshake
(`initialize`, then a `notifications/initialized` notification) before
serving further requests.

### Tools

**`list_windows`**

| param | type | default | description |
|---|---|---|---|
| `app_filter` | string, optional | none | only return windows owned by this app name |
| `on_screen_only` | bool, optional | `true` | restrict to on-screen windows |

Returns `{ windows: [{ window_id, app, title, bounds, focused }, ...] }`.

**`read_window_text`**

| param | type | default | description |
|---|---|---|---|
| `window_id` | integer | — (required) | a `window_id` from `list_windows` |
| `depth` | integer, optional | `20` | accessibility-tree walk depth, capped at `50` |

Returns `{ text, truncated }`. Requires Accessibility permission.

**`capture_region`**

| param | type | default | description |
|---|---|---|---|
| `bounds` | `{x, y, width, height}` | — (required) | screen region to capture |
| `output` | `"text"` \| `"image"` \| `"both"`, optional | `"text"` | what to return |
| `max_dimension` | integer, optional | `1024` | cap on the image's longest side (always enforced) |

Returns `{ text, image }` (`image` is a base64-encoded PNG, present only
when `output` includes an image). Requires Screen Recording permission.

**`read_clipboard`**

| param | type | default | description |
|---|---|---|---|
| `prefer` | `"text"` \| `"image"`, optional | `"text"` | which clipboard representation to prefer |

Returns `{ kind, text, image }` (`kind` is `"text"`, `"image"`, or `"empty"`).

### stdout is sacred

stdout carries **only** JSON-RPC — nothing else is ever written there. All
logging goes to stderr via `tracing`. If you see anything on stdout that
isn't a JSON-RPC message, that's a bug: file an issue.

## Roadmap

This build covers Phase 0 (skeleton) and Phase 1 (macOS MVP read slice) of
the project plan. Later phases, not yet implemented:

- **Linux/X11 backend** — window enumeration via EWMH/X11, accessibility
  text via AT-SPI, OCR via Tesseract, mirroring the macOS capability set.
- **Full image-output surface** — richer capture options (e.g. window-level
  capture, multi-display listing) beyond the current region capture.
- **Gated act tools** — clipboard write, type, click, focus — behind an
  explicit policy gate and disabled by default, kept structurally separate
  from the read tools to avoid prompt-injection-driven side effects.
- **Wayland backend** — portal-based capture (`xdg-desktop-portal`
  ScreenCast) and input (libei InputCapture portal) for modern Linux
  sessions.

See `PRD-desktop-capture-mcp.md` for the full product spec.
