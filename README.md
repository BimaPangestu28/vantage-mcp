# vantage-mcp

## Overview

`vantage-mcp` is a Model Context Protocol (MCP) server that gives an LLM agent
read access to the desktop: enumerate windows, read a window's text content,
capture a screen region as OCR'd text (or an image), and read the clipboard.
It speaks MCP over stdio, so it can be registered with any MCP-capable agent
(Claude Code, Claude Desktop, etc.) as a subprocess.

This build is **read-only** and runs on **macOS and Linux** (Linux covers both
X11 and Wayland, detected at runtime). There are no "act" tools (no typing,
clicking, or clipboard writes) — see [Roadmap](#roadmap).

Every tool is **text-first by design**: `read_window_text` never returns
pixels, and `capture_region` defaults to OCR text with no image payload. This
keeps token cost low for the common case; ask for `output: "image"` or
`"both"` on `capture_region` only when visual layout actually matters.

## Features

- `list_windows` — enumerate on-screen windows (id, owning app, title,
  bounds, focus state).
- `read_window_text` — read a window's content via the platform accessibility
  API (macOS AX / Linux AT-SPI). Cheapest way to get a window's content; prefer
  this over a screenshot + OCR round trip.
- `capture_region` — capture a screen region; defaults to OCR text only, can
  optionally also/instead return a downscaled base64 PNG.
- `capture_window` — capture one window by id (from `list_windows`), same
  text-first output as `capture_region`. macOS and Linux/X11; on Linux/Wayland
  returns an actionable error (compositors don't allow per-window capture).
- `list_displays` — enumerate connected monitors (id, name, bounds, scale
  factor, primary).
- `read_clipboard` — read the system clipboard (text by default, or an
  image as base64 PNG).

**Act tools** (write/side-effecting) are **disabled by default** and gated —
see [Act tools](#act-tools-gated): `write_clipboard`, `type_text`, `click`,
`move_mouse`, `key_press`, `focus_window`.

## Prerequisites

- Rust 1.95 (pinned via `rust-toolchain.toml`).

**macOS** (12.3 or later): two permissions granted **to whatever process
launches this binary** (your terminal, or the agent app itself if it spawns the
process directly):
  - **Screen Recording** — required for `capture_region` and for window
    *titles* in `list_windows`. System Settings → Privacy & Security →
    Screen Recording.
  - **Accessibility** — required for `read_window_text`. System Settings →
    Privacy & Security → Accessibility.

**Linux** (X11 or Wayland):
  - The **accessibility (AT-SPI) bus** must be enabled for `read_window_text`
    and `list_windows` (on by default on GNOME/KDE).
  - **Screen capture** for `capture_region`: on Wayland, approve the
    `xdg-desktop-portal` prompt on first use; on X11 no prompt is shown.
  - A full build needs system libraries for capture (xcap) and OCR (Tesseract);
    see [docs/agent-registration.md](docs/agent-registration.md) for the exact
    `apt` packages, or build with `--no-default-features` to skip them (those
    two tools then return an actionable `Unsupported` error).

On both platforms, a missing permission yields an actionable MCP error naming
what to enable and where — the tools do not hang or crash. See
[docs/agent-registration.md](docs/agent-registration.md) for the exact steps
and what the error looks like.

## Build

```bash
cargo build --release
```

The binary is produced at `target/release/vantage-mcp`. On Linux a full build
needs the capture/OCR system libraries (see [Prerequisites](#prerequisites)); to
build without them, use `cargo build --release --no-default-features` (capture
and OCR then return an actionable `Unsupported` error, other tools work).

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

**`capture_window`**

| param | type | default | description |
|---|---|---|---|
| `window_id` | integer | — (required) | a `window_id` from `list_windows` |
| `output` | `"text"` \| `"image"` \| `"both"`, optional | `"text"` | what to return |
| `max_dimension` | integer, optional | `1024` | cap on the image's longest side (always enforced) |

Returns `{ text, image }`, same shape as `capture_region`. macOS and Linux/X11;
on Linux/Wayland returns an actionable error (per-window capture isn't permitted
by the compositor — use `capture_region` there). Roughly equivalent to
`list_windows` → take `bounds` → `capture_region`, but grabs the true window
(handles occlusion / off-screen).

**`list_displays`**

Takes no parameters. Returns `{ displays: [{ display_id, name, bounds,
scale_factor, is_primary }, ...] }`.

### Act tools (gated)

Act tools **write** / cause side effects, so they are **off by default and
structurally isolated**: unless the operator opts in, they are not mounted —
absent from `tools/list` and uncallable, so a prompt-injection cannot reach
them.

**Enabling.** Launch with `--allow-act` **or** `VANTAGE_ALLOW_ACT=1`
(`true`/`yes` accepted) to mount **all** act tools. To mount only a **subset**,
list them (comma-separated) via `VANTAGE_ACT_TOOLS=write_clipboard,click` or
`--act-tools=write_clipboard,click` — a subset list wins over the all-switch, and
unknown names are warned and ignored. On enable, a warning naming the mounted act
tools is logged to stderr.

| tool | params | notes |
|---|---|---|
| `write_clipboard` | `{ text?, image? }` | set the clipboard (≥1 required; `image` is base64 PNG). macOS + Linux X11/Wayland |
| `type_text` | `{ text }` | synthetic keystrokes; macOS + Linux/X11 (XWayland). Native Wayland injection is compositor-restricted → actionable error |
| `click` | `{ x, y, button?, double? }` | click at absolute coords; `button` = `"left"` (default)/`"right"`/`"middle"`; `double` for double-click. Same platform notes as `type_text` |
| `move_mouse` | `{ x, y }` | move the pointer. Same platform notes as `type_text` |
| `key_press` | `{ keys }` | key combo, e.g. `"ctrl+shift+t"`, `"cmd+c"`, `"enter"`, `"f5"`. Same platform notes as `type_text` |
| `focus_window` | `{ window_id }` | raise/focus a window (from `list_windows`). Linux via AT-SPI (X11 + Wayland); macOS activates the owning app |

Each returns `{ ok: true }` on success. Every act call is logged to stderr for
auditability.

**`read_clipboard`**

| param | type | default | description |
|---|---|---|---|
| `prefer` | `"text"` \| `"image"`, optional | `"text"` | which clipboard representation to prefer |

Returns `{ kind, text, image }` (`kind` is `"text"`, `"image"`, or `"empty"`).

### stdout is sacred

stdout carries **only** JSON-RPC — nothing else is ever written there. All
logging goes to stderr via `tracing`. If you see anything on stdout that
isn't a JSON-RPC message, that's a bug: file an issue.

## Agent (DeepSeek)

`crates/agent` (`vantage-agent`) is a ready-to-run example client: a DeepSeek
agent that drives these tools over MCP. It spawns this server, exposes its tools
to `deepseek-chat`, and runs a tool-calling loop — one-shot or interactive REPL,
read tools free, act tools gated + confirmed. See
[crates/agent/README.md](crates/agent/README.md).

```bash
export DEEPSEEK_API_KEY=sk-...
make build
cargo run -p vantage-agent -- "which windows are open, and which is focused?"
```

## Roadmap

This build covers, on **macOS and Linux** (Linux via AT-SPI for windows/text,
xcap for capture on X11 and Wayland, Tesseract for OCR, arboard for clipboard):
the read slice, window-level capture + display enumeration (`capture_window`,
`list_displays`), and the gated act tools (`clipboard_write`, `type_text`,
`click`, `focus_window`) — the full original roadmap.

Possible future work: image clipboard-write, richer input (scroll, key chords,
drag), and first-class native-Wayland input via the libei/RemoteDesktop portal.

See `PRD-desktop-capture-mcp.md` for the full product spec, and
`docs/superpowers/specs/` for the per-slice design specs.
