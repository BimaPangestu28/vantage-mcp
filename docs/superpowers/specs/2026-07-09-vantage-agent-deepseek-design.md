# Design Spec: vantage-agent — DeepSeek agent over MCP

| Field | Value |
|---|---|
| Scope | A Rust CLI agent that drives the `vantage-mcp` tools via DeepSeek (tool-calling loop) |
| Target platform | Linux/macOS (wherever `vantage-mcp` runs) |
| LLM | DeepSeek `deepseek-chat` (OpenAI-compatible API) |
| Status | Approved design, ready for implementation plan |
| Date | 2026-07-09 |

---

## 1. Objective

Ship `vantage-agent`: an interactive (REPL) and one-shot CLI that connects to the
`vantage-mcp` server, exposes its tools to DeepSeek, and runs the standard
tool-calling agent loop so a user can drive the desktop with natural language
("what windows are open?", "read the text of the Chrome window", and — with the
act gate — "focus the terminal and type ls").

### Success criteria

1. Spawns `vantage-mcp` over stdio (MCP), lists its tools, and converts them to
   OpenAI tool schemas that DeepSeek accepts.
2. Runs the loop: DeepSeek → tool_calls → MCP `call_tool` → results appended →
   repeat until a final text answer; prints a compact per-tool-call trace.
3. Read tools run freely; **act tools require confirmation** (interactive y/N,
   `--yes` to skip, refused in a non-TTY one-shot without `--yes`). Act tools are
   only available when `--allow-act` is passed (agent forwards
   `VANTAGE_ALLOW_ACT=1` to the spawned server).
4. Works as `vantage-agent "task"` (one-shot) and `vantage-agent` (REPL).
5. No system dependency beyond what `vantage-mcp` already needs (reqwest uses rustls).

### Non-goals

- Vision (DeepSeek `deepseek-chat` is text-only; capture images are omitted from
  tool results — the agent uses OCR text).
- Streaming token output, multi-agent, persistence, other providers.
- Re-implementing MCP; we use `rmcp`'s client.

---

## 2. Architecture

New workspace crate `crates/agent` (bin `vantage-agent`). It depends on `rmcp`
(client) + `reqwest` + `tokio` + `serde`/`serde_json` + `rustyline` + `anyhow` +
`tracing`. It does **not** depend on `vantage-core`/server — it talks the MCP
protocol to the built binary.

```
crates/agent/src/
  main.rs       # CLI parse (clap-lite via std::env), config, REPL vs one-shot, wiring
  config.rs     # AgentConfig { api_key, model, base_url, server_bin, allow_act, auto_yes }
  mcp.rs        # McpClient: spawn server, list tools -> OpenAI schemas, call_tool -> sanitized text
  deepseek.rs   # DeepSeek chat client (reqwest) + wire types (Message, ToolCall, ToolSchema)
  agent.rs      # Agent loop: messages, tool dispatch, act-tool confirmation, trace output
```

### 2.1 MCP client (`mcp.rs`)

- Connect: `let service = ().serve(TokioChildProcess::new(cmd)?).await?;` where `cmd`
  is `tokio::process::Command::new(server_bin)`, inheriting env plus
  `VANTAGE_ALLOW_ACT=1` when `allow_act`. stderr inherited so server logs show.
- `list_all_tools().await?` → for each `Tool`, build an OpenAI tool:
  `{ "type":"function", "function": { "name": tool.name, "description": tool.description,
  "parameters": tool.input_schema } }` (the MCP `input_schema` is already a JSON Schema object).
- `call_tool(name, args_json)` → `CallToolResult`. Serialize the result for the LLM:
  prefer `structured_content` (JSON), else join text `ContentBlock`s. **Sanitize**:
  recursively replace any string field named `image` whose value looks like base64
  (long, no spaces) with `"[image omitted: N chars base64 PNG]"`, so a screenshot
  doesn't blow the context. Mark `is_error` results with an `"error": true` note.
- Expose `is_act_tool(name)` via a known set matching the server's `ACT_TOOL_NAMES`
  (`write_clipboard`, `type_text`, `click`, `move_mouse`, `key_press`, `focus_window`).

### 2.2 DeepSeek client (`deepseek.rs`)

- `reqwest::Client` (rustls). POST `${base_url}/chat/completions` (base_url default
  `https://api.deepseek.com`), bearer `api_key`.
- Request: `{ model, messages, tools, tool_choice: "auto", temperature: 0.2 }`.
- Wire types (serde): `Message { role, content: Option<String>, tool_calls?, tool_call_id? }`,
  `ToolCall { id, type:"function", function: { name, arguments: String } }` (arguments
  is a JSON *string* per OpenAI). Response: `choices[0].message`.
- Errors (non-2xx, network) → `anyhow` with the body for diagnosis.

### 2.3 Agent loop (`agent.rs`)

```
messages = [system, user]
loop:
  resp = deepseek.chat(messages, tools)
  msg  = resp.choices[0].message
  messages.push(assistant = msg)
  if msg.tool_calls is empty:
      print(msg.content); return   # final answer
  for tc in msg.tool_calls:
      args = parse(tc.function.arguments)           # JSON string -> Value
      if is_act_tool(tc.name) and not confirmed(tc): # print + y/N (unless --yes)
          result = "{\"refused\": true, \"reason\": \"user declined\"}"
      else:
          print_trace(tc.name, args)
          result = mcp.call_tool(tc.name, args)      # sanitized text
      messages.push(tool = { tool_call_id: tc.id, content: result })
```

- A hard cap on iterations (e.g. 25) to avoid runaway loops; on hit, stop with a note.
- System prompt: explains the desktop tools, that captures are text-first (request
  `output:"text"`; images are not visible to the model), to call `list_windows`
  first to orient, and to be conservative and explicit with act tools.

### 2.4 CLI (`main.rs`)

- `vantage-agent [OPTIONS] [TASK]`. No `TASK` → REPL (`rustyline`, `>` prompt,
  `exit`/Ctrl-D to quit, conversation persists across turns).
- Options: `--allow-act`, `--yes` (skip act confirmation), `--model <m>`
  (default `deepseek-chat`), `--server <path>` (default `./target/release/vantage-mcp`),
  `--base-url <u>`. `DEEPSEEK_API_KEY` required (error early if missing).
- Minimal hand-rolled arg parsing (no new clap dep) — the option set is small.

---

## 3. Testing

| Test | Where |
|---|---|
| tool-schema conversion (MCP `Tool` → OpenAI function) shape | unit |
| result sanitizer strips a base64 `image` field, keeps `text` | unit |
| `is_act_tool` matches the six act names | unit |
| MCP client: spawn `vantage-mcp`, list tools (6 read / 12 with act) | integration (needs built server) |
| Full DeepSeek loop against a read-only task | live (needs `DEEPSEEK_API_KEY`) |

### Verification matrix (this environment)

- ✅ Here: build, unit tests, and the MCP-client integration (spawn server, list +
  call a read tool like `list_windows` — no LLM needed).
- ⚠️ Not here without a key: the DeepSeek round-trip. If `DEEPSEEK_API_KEY` is
  provided, drive a read-only task e2e; otherwise ship the run command + docs and
  leave the live LLM run to the operator. Flagged in the plan.

---

## 4. Risks

1. **LLM-driven side effects.** Act tools can type/click. Mitigated by: act tools
   off unless `--allow-act`, per-call confirmation by default, conservative system
   prompt. The server's own gate is the backstop.
2. **Context blow-up from images.** Sanitizer omits base64 image payloads.
3. **DeepSeek tool-calling quirks.** `arguments` is a JSON string; the model may
   emit malformed JSON — parse defensively and feed an error result back so the
   model can retry rather than crashing the loop.
4. **Server binary path.** Defaulted + `--server` override; clear error if missing
   (suggest `make build`).
