# PRD: Desktop Capture & Control MCP Server

| Field | Value |
|---|---|
| Working codename | `vantage-mcp` (placeholder, rename TBD) |
| Author | Bima Pangestu |
| Status | Draft v0.1 |
| Last updated | 2026-07-08 |
| Target platforms | macOS 12.3+, Linux (X11 first, Wayland later) |
| Delivery | Single static Rust binary exposing an MCP server |

---

## 1. Summary

A headless MCP server, written in Rust, that gives an LLM agent the ability to see and act on the local desktop. It exposes desktop capabilities (screen capture, window content reading, OCR, clipboard, and input control) as MCP tools so any MCP-capable agent can drive them.

The design priority is a productivity surface: let an agent read what is on screen or inside a specific window, extract text via OCR, work with the clipboard, and optionally act on the desktop, while keeping context cost low and the security boundary explicit.

This is not a desktop app with its own UI. The agent is the interface. There is no primary tray or hotkey surface in v1.

---

## 2. Problem & Motivation

Agents today are mostly blind to the local desktop. They can call APIs and read files, but they cannot see what is currently on screen, read the contents of a native app that has no API, or interact with GUI-only workflows.

The gap is largest for:

- Native or legacy apps with no scriptable API, where the only access path is the accessibility tree or pixels on screen.
- Ad-hoc extraction tasks, for example "read the text in this dialog" or "grab the table from that window."
- Clipboard-mediated workflows, where the agent needs to see or set what the user just copied.

Rust is a strong fit for this because it produces a single dependency-free static binary, gives predictable low latency, and has direct access to the platform native APIs required for capture and accessibility.

---

## 3. Goals & Non-Goals

### Goals

1. Expose a clean set of MCP tools for reading desktop state (displays, windows, window text, screen regions, clipboard).
2. Provide server-side OCR so the agent can request text instead of raw pixels.
3. Keep context (token) cost low by defaulting to text over images and supporting region- and window-scoped capture with downscaling.
4. Provide an optional, gated set of act tools (clipboard write, type, click, focus) with a clear security boundary separating them from read tools.
5. Run reliably on macOS and Linux/X11 as v1, with a platform abstraction that lets Wayland be filled in later without reworking the core.
6. Ship as a single signed binary that is straightforward to register with a local agent.

### Non-Goals (v1)

- No global input listening / keystroke recording as a first-class feature (does not map cleanly to the MCP request/response model; deferred).
- No screen video recording to file (only still capture in v1).
- No Windows support in v1.
- No built-in GUI, tray, or global hotkey surface.
- No remote multi-user hosting. The server runs on one user machine.

---

## 4. Target Users & Use Cases

### Primary user

A developer or power user running an MCP-capable agent (Claude Code, Claude Desktop, or a custom or Greentic-style agent runtime) locally, who wants the agent to see and optionally act on their desktop.

### Primary use cases

1. **Read a window without an API.** Agent calls `list_windows`, picks the target, calls `read_window_text` to get the accessibility tree as text.
2. **Extract text from a region.** Agent calls `capture_region` with `output=text`; server captures the region and returns OCR text only. No image enters context.
3. **Understand visual layout.** When layout matters, agent calls `capture_region` with `output=image` and a bounded `max_dimension`, receiving a downscaled screenshot.
4. **Clipboard workflows.** Agent reads what the user copied via `read_clipboard`, transforms it, and writes it back via `write_clipboard` (gated).
5. **Light desktop automation.** Agent focuses a window and types or clicks to complete a GUI step (gated act tools).

---

## 5. Scope

### In scope (v1)

- Display and window enumeration.
- Accessibility-based window text reading.
- Still screen capture, region- and window-scoped, with OCR option.
- Clipboard read and write (text and image).
- Gated input control: type text, click, move mouse, key press, focus window.
- macOS and Linux/X11 backends.

### Out of scope (v1, candidate for later)

- Wayland capture and input (portal-based, deferred to a later phase).
- Clipboard history store and watcher.
- Video recording.
- Global input capture / macro recording.
- Windows backend.

---

## 6. Functional Requirements: MCP Tool Surface

Tools are split into two classes. This split is a security boundary, not just an organizational one (see Section 9).

### 6.1 Read tools (information)

| Tool | Params | Returns | Notes |
|---|---|---|---|
| `list_displays` | none | array of `{id, bounds, scale, primary}` | Cheap. |
| `list_windows` | `{app_filter?, on_screen_only?}` | array of `{window_id, app, title, bounds, focused}` | Primary entry point for the agent to orient. |
| `read_window_text` | `{window_id, depth?}` | structured text of accessibility tree | Cheapest way to get window content. Prefer over screenshot+OCR. |
| `capture_region` | `{bounds, output, max_dimension?}` | text, image (base64 PNG), or both | `output` is `text` \| `image` \| `both`. `text` runs server-side OCR and returns no pixels. |
| `capture_window` | `{window_id, output, max_dimension?}` | text, image, or both | Same output modes as `capture_region`. |
| `read_clipboard` | `{prefer?}` | `{kind, text?, image?}` | `prefer` = `text` \| `image`. |

### 6.2 Act tools (mutation, gated)

| Tool | Params | Returns | Notes |
|---|---|---|---|
| `write_clipboard` | `{text?, image?}` | `{ok}` | Requires policy gate. |
| `focus_window` | `{window_id}` | `{ok}` | Requires policy gate. |
| `type_text` | `{text}` | `{ok}` | Requires policy gate. Simulated keyboard input. |
| `click` | `{x, y, button?, double?}` | `{ok}` | Requires policy gate. Coordinates validated against display bounds. |
| `move_mouse` | `{x, y}` | `{ok}` | Requires policy gate. |
| `key_press` | `{keys}` | `{ok}` | Requires policy gate. Modifier + key combos. |

### 6.3 Tool behavior requirements

- Every capture tool must default toward the cheapest useful output. If the caller does not specify `output`, default to `text`.
- Image returns must be downscaled to `max_dimension` (default cap enforced server-side even if the caller omits it).
- Tools must return actionable, structured errors on permission denial (for example a distinct error type for "Screen Recording permission not granted" versus "window not found"), so the agent can surface a fix rather than a generic failure.
- Act tools must be individually disable-able via config and must pass through the policy gate before executing.

---

## 7. Non-Functional Requirements

- **Single binary.** No runtime dependencies beyond system OCR/accessibility frameworks. On Linux, Tesseract is an allowed system dependency.
- **Stdout is sacred.** When using stdio transport, nothing may write to stdout except the JSON-RPC stream. All logging routes to stderr via `tracing_subscriber` with a stderr writer. Every dependency is audited for stdout writes.
- **Latency.** `list_windows` and `read_window_text` should return in well under 200 ms on a typical machine. Capture + OCR of a region target under ~1 s.
- **Token economy.** A full-screen screenshot as base64 is not an acceptable default output. Text-first defaults, downscaling, and region scoping are hard requirements, not optimizations.
- **Portability.** Core logic is platform-agnostic behind traits. Platform backends are swappable and independently testable.
- **Graceful degradation.** If a capability is unavailable on the current platform or display server (for example accessibility text on a locked-down Wayland session), the tool returns a clear "unsupported on this platform" error rather than hanging or crashing.

---

## 8. Architecture

### 8.1 Workspace layout

```
vantage-mcp/
  crates/
    mcp-server/      # rmcp: tool handlers, transport wiring, policy gate
    core/            # orchestration, OCR pipeline, capability traits
    platform/
      macos/         # ScreenCaptureKit, AXUIElement, Vision OCR, enigo
      linux-x11/     # xcap, XRecord/X11, atspi, arboard, enigo
      linux-wayland/ # (later) ashpd ScreenCast + InputCapture portals
```

### 8.2 Capability traits (in `core`)

The core defines the contract; platform crates implement it:

- `ScreenCapturer` (regions, windows, displays)
- `WindowInspector` (enumeration, accessibility text)
- `ClipboardAccess` (read/write text and image)
- `InputController` (type, click, move, key, focus)
- `TextRecognizer` (OCR)

MCP tool handlers in `mcp-server` depend only on these traits, never on a specific platform.

### 8.3 SDK

- `rmcp` (official Rust MCP SDK, 0.3.x) with `server`, `transport-io`, and `macros` features.
- Consider `rust-mcp-sdk` only if remote HTTP hosting with OAuth becomes a hard requirement (see Section 8.4).
- Reference prior art: the `terminator` MCP server (cross-platform desktop automation over accessibility) for tool design and AX handling patterns.

### 8.4 Transport & permissions (critical, decide first)

Transport choice and macOS permissions are coupled and must be decided together.

- **Local agent (default assumption for v1):** stdio transport. The agent spawns the binary as a child process.
- **Remote agent:** stdio cannot reach the desktop. This requires a streamable HTTP server running on the user machine, with auth. Deferred unless required.

**macOS permission constraint.** TCC permissions (Screen Recording, Accessibility) attach to the running process. A bare CLI binary spawned over stdio by a parent app cannot cleanly hold these grants. Mitigation options:

1. Ship as a signed `.app` bundle (or code-signed binary with the correct entitlements), or
2. Run the server as a persistent LaunchAgent daemon that holds its own TCC grants, with the agent connecting over local HTTP.

Option 2 pushes macOS toward a daemon + local HTTP model rather than pure stdio. This is an explicit open decision (see Section 12).

**Linux.** X11 is straightforward. Wayland requires portals (`xdg-desktop-portal` ScreenCast over PipeWire for capture, and the newer InputCapture portal via libei for input), which is why Wayland is a later phase.

---

## 9. Security Model

An MCP server that can both read the screen and control input, driven by an LLM, is a prompt-injection amplifier. It combines access to data (screen and window contents), the ability to act (input control), and exposure to untrusted content (text on screen or inside another app may contain injected instructions). Screen or window content the agent reads can carry instructions that then drive the act tools.

Requirements:

1. **Read/act separation is enforced, not cosmetic.** Read tools and act tools are distinct capabilities that can be enabled independently.
2. **Act tools are gated.** Every act tool passes through a policy gate before executing. Default posture ships act tools disabled or requiring confirmation.
3. **OCR and accessibility output are never treated as trusted instructions.** They are data. The server does not interpret them.
4. **Scoped reads.** Window-scoped reads are preferred over full-screen. Full-screen capture is available but not the default path.
5. **Auditability.** All act-tool invocations are logged (to stderr / a log sink) with their parameters.

---

## 10. Platform Support Matrix

| Capability | macOS | Linux X11 | Linux Wayland |
|---|---|---|---|
| Screen capture | ScreenCaptureKit (12.3+) | xcap | Portal ScreenCast + PipeWire (later) |
| Window enumeration | CGWindowList / AX | EWMH / X11 | Limited via portal (later) |
| Window text (a11y) | AXUIElement | AT-SPI2 (`atspi`) | AT-SPI2 (session dependent) |
| OCR | Vision framework | Tesseract (`leptess`) | Tesseract |
| Clipboard | `arboard` | `arboard` | `arboard` (wl protocols) |
| Input control | `enigo` (Input Monitoring perm) | `enigo` | InputCapture portal / libei (later) |

---

## 11. Milestones

### Phase 0: Skeleton
- `rmcp` stdio server boots, registers tools, logging on stderr.
- Trait layer defined in `core`. macOS + X11 backend stubs compile.

### Phase 1: MVP read slice (highest value first)
- `list_windows`, `read_window_text`, `capture_region` (with `output=text` via OCR), `read_clipboard`.
- macOS (Vision + SCK + AX) and Linux X11 (Tesseract + xcap + AT-SPI).
- Verified working against a real local agent.

### Phase 2: Image output + full read surface
- `capture_window`, image output modes with downscaling, `list_displays`.

### Phase 3: Gated act tools
- `write_clipboard`, `focus_window`, `type_text`, `click`, `move_mouse`, `key_press`, plus the policy gate.
- macOS signing / entitlements resolved so input control and capture hold permissions.

### Phase 4: Wayland backend
- Portal-based capture and input for Wayland sessions.

### Phase 5 (candidate)
- Clipboard history store, remote HTTP transport with auth, Windows backend.

---

## 12. Success Metrics

- Agent can read the text content of an arbitrary native window on macOS and Linux/X11 in a single tool round-trip.
- Default text-first path keeps typical read operations under a few thousand tokens (no accidental full-screen image dumps).
- Permission-denied states return actionable errors 100% of the time (never a silent hang or generic failure).
- Act tools cannot fire without passing the policy gate.

---

## 13. Risks & Open Questions

### Risks

| Risk | Impact | Mitigation |
|---|---|---|
| macOS TCC grants do not attach to a spawned stdio binary | Capture/input silently fail | Signed bundle or LaunchAgent daemon + local HTTP (decide early) |
| A dependency writes to stdout | Agent disconnects silently | Audit all deps, force stderr logging, add a startup self-check |
| Full-screen image output blows up context cost | Expensive, slow agent loops | Text-first defaults, enforced downscaling, region scoping |
| Prompt injection via on-screen content driving act tools | Unsafe actions | Read/act separation, policy gate, no interpretation of read output |
| Wayland capture/input restrictions | Feature gaps on modern Linux | Scope Wayland as its own phase, degrade gracefully |

### Open questions

1. Primary transport for v1: pure stdio, or daemon + local HTTP (driven mostly by the macOS permission decision)?
2. Is this a Catalyst Labs product, a Greentic-internal capability, or a standalone open-source tool? Affects naming, licensing, and distribution.
3. Default posture for act tools: shipped disabled, confirmation-required, or policy-file driven?
4. Is accessibility text reliable enough across target apps, or is OCR fallback needed as a standard path?

---

## Appendix A: Candidate crates

- MCP: `rmcp` (official). Alt: `rust-mcp-sdk` (HTTP + OAuth).
- Capture: `xcap` (cross-platform), `screencapturekit` (macOS direct), `ashpd` (Wayland portals).
- Accessibility: `objc2` / `accessibility` (macOS), `atspi` (Linux).
- OCR: Vision via `objc2` (macOS), `leptess` / `rusty-tesseract` (Linux).
- Clipboard: `arboard`.
- Input: `enigo`.
- Runtime & plumbing: `tokio`, `serde`, `serde_json`, `schemars`, `tracing`, `tracing-subscriber`, `anyhow`.
