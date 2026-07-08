# Registering `vantage-mcp` with a local MCP agent

`vantage-mcp` is a stdio MCP server: the agent spawns it as a subprocess and
talks JSON-RPC over its stdin/stdout. This doc covers building the binary,
registering it with Claude Code or Claude Desktop, and granting the macOS
permissions it needs.

## 1. Build the binary

```bash
cd /path/to/vantage-mcp
cargo build --release
```

This produces `target/release/vantage-mcp`. Note the absolute path — you'll
point the agent's config at it directly.

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

## 4. Grant macOS permissions

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

## 6. Sanity-checking the registration

Once registered, from the agent:

1. Call `list_windows` — expect a non-empty list of windows with ids.
2. Call `read_window_text` with a `window_id` from step 1 — expect either
   real text back, or (if Accessibility isn't yet granted) the actionable
   error above, in a single round trip.

If you get the actionable error, grant Accessibility per step 4, restart the
launching app, and retry — it should now return real text.
