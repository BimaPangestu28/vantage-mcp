# vantage-agent

A small Rust CLI agent that drives the [`vantage-mcp`](../..) desktop tools with
**DeepSeek**. It spawns the `vantage-mcp` server over MCP (stdio), exposes its
tools to `deepseek-chat`, and runs the standard tool-calling loop — so you can
ask, in natural language, things like "what windows are open?" or (with act
tools enabled) "focus the terminal and type ls".

## Prerequisites

- `DEEPSEEK_API_KEY` in the environment.
- The server binary built: `make build` (from the repo root) → `target/release/vantage-mcp`.
- For window/text/capture tools: the same desktop-session permissions the server
  needs (see the root README).

## Usage

```bash
export DEEPSEEK_API_KEY=sk-...

# One-shot: run a task and print the answer
cargo run -p vantage-agent -- "list the open windows and tell me which app is focused"

# Interactive REPL (conversation persists across turns)
cargo run -p vantage-agent
```

A compact trace of each tool call is printed to stderr (`↳ list_windows({...})`),
and the final answer to stdout.

### Options

| flag | meaning |
|---|---|
| `--allow-act` | Enable act tools — forwards `VANTAGE_ALLOW_ACT=1` to the server so it mounts `write_clipboard`/`type_text`/`click`/`move_mouse`/`key_press`/`focus_window` |
| `--yes` / `-y` | Skip the per-call confirmation for act tools |
| `--model <m>` | DeepSeek model (default `deepseek-chat`) |
| `--server <path>` | Path to the `vantage-mcp` binary (default `./target/release/vantage-mcp`) |
| `--base-url <url>` | API base URL (default `https://api.deepseek.com`) |

`VANTAGE_AGENT_LOG` sets the log filter (default `warn`).

## Safety (act tools)

Read tools run freely. Act tools cause real side effects, so:

- They are **only available with `--allow-act`** (otherwise not even mounted — the
  server's own gate).
- Every act call is **confirmed** (`y/N`) before it runs. `--yes` skips the
  prompt; a **non-interactive** session (piped stdin) **refuses** act tools unless
  `--yes` is given, so an unattended run can't silently type or click.

```bash
# Enable only clipboard writes, and auto-confirm:
cargo run -p vantage-agent -- --allow-act --yes "put 'hello world' on my clipboard"
```

DeepSeek `deepseek-chat` is text-only, so captured **images are omitted** from
tool results — the agent works from OCR text (the capture tools default to
`output=text`).

## How it works

1. `mcp.rs` — spawns the server, `list_all_tools()` → OpenAI tool schemas,
   `call_tool()` → sanitized JSON (base64 images stripped).
2. `deepseek.rs` — OpenAI-compatible `chat/completions` client (reqwest/rustls).
3. `agent.rs` — the loop: DeepSeek → `tool_calls` → MCP → results → repeat until a
   final text answer (with act-tool confirmation).
4. `main.rs` — arg parsing, one-shot vs REPL, system prompt.

## Testing

```bash
cargo test -p vantage-agent                                   # unit tests
make build && cargo test -p vantage-agent --test mcp_live -- --ignored   # live MCP client (6/12 tools)
```

The full DeepSeek loop needs a valid `DEEPSEEK_API_KEY` — run a one-shot task to
exercise it end to end.
