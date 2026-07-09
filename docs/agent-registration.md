# Registering `vantage-mcp` with a local MCP agent

`vantage-mcp` is a stdio MCP server: the agent spawns it as a subprocess and
talks JSON-RPC over its stdin/stdout. This doc covers building the binary,
registering it with Claude Code or Claude Desktop, and granting the OS
permissions it needs (macOS and Linux).

## 1. Build the binary

```bash
cd /path/to/vantage-mcp
cargo build --release
```

This produces `target/release/vantage-mcp`. Note the absolute path — you'll
point the agent's config at it directly.

**Linux build prerequisites.** A full Linux build compiles screen-capture
(xcap) and OCR (Tesseract), which need system libraries:

```bash
sudo apt-get install -y \
  libwayland-dev libxcb1-dev libxcb-randr0-dev libxcb-shm0-dev \
  libdbus-1-dev libpipewire-0.3-dev libxkbcommon-dev \
  libtesseract-dev libleptonica-dev tesseract-ocr-eng clang libclang-dev
```

If you don't need capture/OCR (e.g. only window enumeration, accessibility text,
and clipboard), build without those system deps:

```bash
cargo build --release --no-default-features
```

In that build `capture_region` and OCR return an actionable `Unsupported`
error; the other tools work normally. (macOS uses system frameworks and needs
no extra packages.)

## 2. Register with Claude Code

Claude Code reads MCP server definitions from its settings (project-level
`.mcp.json` or global config, depending on how you invoke it). Add an entry
like:

```json
{
  "mcpServers": {
    "vantage": {
      "command": "/path/to/vantage-mcp/target/release/vantage-mcp",
      "args": []
    }
  }
}
```

Or via the CLI:

```bash
claude mcp add vantage /path/to/vantage-mcp/target/release/vantage-mcp
```

## 3. Register with Claude Desktop

Edit Claude Desktop's config file (macOS:
`~/Library/Application Support/Claude/claude_desktop_config.json`) and add:

```json
{
  "mcpServers": {
    "vantage": {
      "command": "/path/to/vantage-mcp/target/release/vantage-mcp",
      "args": []
    }
  }
}
```

Restart Claude Desktop for the change to take effect.

## 3b. Enabling act tools (optional, off by default)

By default `vantage-mcp` serves only read tools. The six act tools
(`write_clipboard`, `type_text`, `click`, `move_mouse`, `key_press`,
`focus_window`) cause side effects and are **not mounted** unless you explicitly
opt in — they won't even appear in `tools/list`. Enable **all** of them with the
`--allow-act` flag **or** the `VANTAGE_ALLOW_ACT=1` environment variable, or mount
only a **subset** with `VANTAGE_ACT_TOOLS=write_clipboard,click` (or
`--act-tools=write_clipboard,click`):

```json
{
  "mcpServers": {
    "vantage": {
      "command": "/path/to/vantage-mcp/target/release/vantage-mcp",
      "args": ["--allow-act"]
    }
  }
}
```

or

```json
{
  "mcpServers": {
    "vantage": {
      "command": "/path/to/vantage-mcp/target/release/vantage-mcp",
      "env": { "VANTAGE_ALLOW_ACT": "1" }
    }
  }
}
```

⚠️ **Only enable this if you trust the agent and its context.** With act tools
mounted, the agent can type, click, focus windows, and write your clipboard.
The gate is deliberately operator-controlled at launch (not agent-controlled at
runtime); every act call is logged to stderr. `type_text`/`click` require X11 or
macOS — on native Wayland the compositor restricts synthetic input and the tools
return an actionable error.

## 4. Grant OS permissions

### macOS

`vantage-mcp` needs two permissions granted to **whatever process launches
it** — that's the agent app itself if it spawns the binary directly (e.g.
Claude Desktop.app), or your terminal app if you're driving it from a
terminal-launched agent (e.g. Claude Code in Terminal.app / iTerm).

Open **System Settings → Privacy & Security**, then:

- **Screen Recording** — add the launching app (Claude Desktop / Terminal /
  iTerm / etc). Needed for `capture_region` and for window *titles* in
  `list_windows`.
- **Accessibility** — add the same launching app. Needed for
  `read_window_text`.

After granting either permission for the first time, **fully quit and
restart the launching app** (macOS does not apply newly granted TCC
permissions to an already-running process). If the agent spawns
`vantage-mcp` as a fresh subprocess each session, restarting the parent app
is normally enough; a long-lived subprocess started before the grant will
also need to be restarted.

### Linux (X11 and Wayland)

Linux has no per-app TCC prompt like macOS, but two capabilities depend on the
session:

- **Accessibility bus (AT-SPI)** — required for `read_window_text` and for
  window enumeration in `list_windows`. GNOME and KDE enable the accessibility
  bus by default. If it is off, enable assistive technologies in your desktop's
  accessibility settings (or ensure `at-spi2-registryd` is running). When the
  bus is unavailable the affected tools return an actionable
  `AccessibilityPermissionDenied` error.
- **Screen capture** — required for `capture_region`. On **Wayland**, the first
  capture triggers an `xdg-desktop-portal` permission prompt; approve it for the
  launching application. On **X11** no prompt is shown. A denied/cancelled
  capture returns an actionable `ScreenRecordingPermissionDenied` error.

The backend auto-detects X11 vs Wayland at runtime; no configuration is needed.
Note that on Wayland some compositors (e.g. GNOME/Mutter) do not expose
on-screen coordinates for native Wayland windows, so `bounds` may read as zero
there; on X11 the real geometry is reported.

## 5. What permission-denied looks like

If a permission is missing, the affected tool call returns a normal MCP
tool error — not a hang, not a generic internal error. For example, calling
`read_window_text` without Accessibility permission granted returns:

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "error": {
    "code": -32600,
    "message": "Accessibility permission not granted to this process. Grant it in System Settings > Privacy & Security > Accessibility, then restart the agent."
  }
}
```

This is `invalid_request` (`-32600`), distinct from an internal error, and
the message names the exact permission and the Settings path to fix it. The
equivalent applies to `capture_region` when Screen Recording is missing
(`ScreenRecordingPermissionDenied`, same error shape, message names "Screen
Recording"). `list_windows` degrades more gracefully: without Screen
Recording it still returns windows, just with empty titles, rather than
erroring.

On **Linux** the error code and shape are identical, but the message text is
platform-appropriate: `AccessibilityPermissionDenied` explains the AT-SPI
accessibility bus, and `ScreenRecordingPermissionDenied` explains the Wayland
screen-capture portal / X11 capture rather than macOS System Settings.

## 6. Sanity-checking the registration

Once registered, from the agent:

1. Call `list_windows` — expect a non-empty list of windows with ids.
2. Call `read_window_text` with a `window_id` from step 1 — expect either
   real text back, or (if Accessibility isn't yet granted) the actionable
   error above, in a single round trip.

If you get the actionable error, grant Accessibility per step 4, restart the
launching app, and retry — it should now return real text.
