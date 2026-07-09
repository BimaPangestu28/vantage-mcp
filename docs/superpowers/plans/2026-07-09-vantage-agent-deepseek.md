# vantage-agent (DeepSeek over MCP) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Rust CLI (`vantage-agent`) that spawns `vantage-mcp`, exposes its tools to DeepSeek (`deepseek-chat`), and runs a tool-calling agent loop — REPL + one-shot, read tools free, act tools gated + confirmed.

**Architecture:** New workspace crate `crates/agent`. `mcp.rs` wraps the `rmcp` client (spawn server via `TokioChildProcess`, list/convert/call tools, sanitize results). `deepseek.rs` is a `reqwest` OpenAI-compatible chat client. `agent.rs` runs the loop with act-tool confirmation. `main.rs` parses args and drives REPL/one-shot.

**Tech Stack:** Rust 1.95; `rmcp` (client + transport-child-process); `reqwest` (rustls-tls, json); `tokio`; `serde`/`serde_json`; `rustyline`; `anyhow`; `tracing`.

## Global Constraints

- The agent talks **only MCP** to the built `vantage-mcp` binary; it does NOT depend on `vantage-core`/server crates.
- **Act safety:** act tools (`write_clipboard`, `type_text`, `click`, `move_mouse`, `key_press`, `focus_window`) are available only with `--allow-act` (agent sets `VANTAGE_ALLOW_ACT=1` on the spawned server) AND each call is confirmed (y/N) unless `--yes`; in a non-TTY one-shot, act calls are refused without `--yes`.
- **No new system deps:** `reqwest` uses `rustls-tls` (`default-features = false`), not OpenSSL.
- **Context economy:** tool results feed back as JSON text with base64 `image` fields stripped.
- `DEEPSEEK_API_KEY` required; clear early error if absent. Iteration cap to avoid runaway loops. No secrets logged. Commit after each task's tests pass. Conventional commits.

---

### Task 1: Crate scaffold + DeepSeek client

**Files:**
- Create: `crates/agent/Cargo.toml`, `crates/agent/src/main.rs` (placeholder), `crates/agent/src/deepseek.rs`, `crates/agent/src/config.rs`
- Modify: root `Cargo.toml` (workspace member)
- Test: inline unit tests in `deepseek.rs`

**Interfaces:**
- Produces: `config::AgentConfig`; `deepseek::{DeepSeek, Message, ToolCall, ToolSchema, Role}`; `DeepSeek::chat(&self, messages, tools) -> anyhow::Result<Message>`.

- [ ] **Step 1: Add the crate to the workspace + `crates/agent/Cargo.toml`**

Root `Cargo.toml` members: add `"crates/agent"`.

```toml
[package]
name = "vantage-agent"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "vantage-agent"
path = "src/main.rs"

[dependencies]
rmcp = { version = "2.1.0", features = ["client", "transport-child-process"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "process", "io-std"] }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
rustyline = "14"
```

- [ ] **Step 2: `crates/agent/src/config.rs`**

```rust
/// Runtime configuration, assembled from CLI flags + environment.
pub struct AgentConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub server_bin: String,
    pub allow_act: bool,
    pub auto_yes: bool,
}

impl AgentConfig {
    pub const DEFAULT_BASE_URL: &'static str = "https://api.deepseek.com";
    pub const DEFAULT_MODEL: &'static str = "deepseek-chat";
    pub const DEFAULT_SERVER: &'static str = "./target/release/vantage-mcp";
}
```

- [ ] **Step 3: `crates/agent/src/deepseek.rs` — wire types + client**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(s: impl Into<String>) -> Self { Self::text(Role::System, s) }
    pub fn user(s: impl Into<String>) -> Self { Self::text(Role::User, s) }
    pub fn tool(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self { role: Role::Tool, content: Some(content.into()), tool_calls: vec![], tool_call_id: Some(id.into()) }
    }
    fn text(role: Role, s: impl Into<String>) -> Self {
        Self { role, content: Some(s.into()), tool_calls: vec![], tool_call_id: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "default_function")]
    pub kind: String,
    pub function: FunctionCall,
}
fn default_function() -> String { "function".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    /// OpenAI encodes arguments as a JSON *string*.
    pub arguments: String,
}

/// One OpenAI-style tool (function) advertised to the model.
#[derive(Debug, Clone, Serialize)]
pub struct ToolSchema {
    #[serde(rename = "type")]
    pub kind: &'static str, // "function"
    pub function: FunctionSchema,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

pub struct DeepSeek {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl DeepSeek {
    pub fn new(api_key: String, base_url: String, model: String) -> Self {
        Self { http: reqwest::Client::new(), api_key, base_url, model }
    }

    pub async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> anyhow::Result<Message> {
        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            messages: &'a [Message],
            tools: &'a [ToolSchema],
            tool_choice: &'a str,
            temperature: f32,
        }
        #[derive(Deserialize)]
        struct Resp { choices: Vec<Choice> }
        #[derive(Deserialize)]
        struct Choice { message: Message }

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&Req { model: &self.model, messages, tools, tool_choice: "auto", temperature: 0.2 })
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            anyhow::bail!("DeepSeek API error {status}: {body}");
        }
        let parsed: Resp = serde_json::from_str(&body)
            .map_err(|e| anyhow::anyhow!("decode DeepSeek response ({e}): {body}"))?;
        parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message)
            .ok_or_else(|| anyhow::anyhow!("DeepSeek returned no choices"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn message_serializes_openai_shape() {
        let m = Message::tool("call_1", "{\"ok\":true}");
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["role"], "tool");
        assert_eq!(v["tool_call_id"], "call_1");
        assert_eq!(v["content"], "{\"ok\":true}");
        assert!(v.get("tool_calls").is_none(), "empty tool_calls must be omitted");
    }
    #[test]
    fn assistant_tool_call_roundtrips() {
        let json = r#"{"role":"assistant","content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":"list_windows","arguments":"{}"}}]}"#;
        let m: Message = serde_json::from_str(json).unwrap();
        assert_eq!(m.role, Role::Assistant);
        assert_eq!(m.tool_calls[0].function.name, "list_windows");
    }
}
```

- [ ] **Step 4: `crates/agent/src/main.rs` placeholder that compiles**

```rust
mod config;
mod deepseek;

fn main() {
    eprintln!("vantage-agent (scaffold)");
}
```

- [ ] **Step 5: Build + unit tests**

Run: `cargo test -p vantage-agent`
Expected: PASS — `message_serializes_openai_shape`, `assistant_tool_call_roundtrips`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/agent
git commit -m "feat(agent): crate scaffold + DeepSeek chat client"
```

---

### Task 2: MCP client (spawn server, list/convert/call tools, sanitize)

**Files:**
- Create: `crates/agent/src/mcp.rs`
- Modify: `crates/agent/src/main.rs` (add `mod mcp;`)
- Test: inline unit tests (schema conversion, sanitizer, is_act_tool)

**Interfaces:**
- Consumes: `deepseek::{ToolSchema, FunctionSchema}`.
- Produces: `mcp::McpClient` with `async connect(server_bin, allow_act) -> Result<Self>`, `tool_schemas() -> &[ToolSchema]`, `async call(&self, name, args: serde_json::Value) -> Result<String>` (sanitized text), and `is_act_tool(name) -> bool`.

- [ ] **Step 1: Sanitizer + act-set as pure, testable functions**

```rust
use serde_json::Value;

pub const ACT_TOOLS: [&str; 6] = [
    "write_clipboard", "type_text", "click", "move_mouse", "key_press", "focus_window",
];

pub fn is_act_tool(name: &str) -> bool {
    ACT_TOOLS.contains(&name)
}

/// Replace long base64-looking values under any `image` key with a short
/// placeholder so screenshots don't blow the LLM context.
pub fn sanitize(mut v: Value) -> Value {
    fn walk(v: &mut Value) {
        match v {
            Value::Object(map) => {
                for (k, val) in map.iter_mut() {
                    if k == "image" {
                        if let Value::String(s) = val {
                            if s.len() > 256 && !s.contains(' ') {
                                *val = Value::String(format!("[image omitted: {} base64 chars]", s.len()));
                                continue;
                            }
                        }
                    }
                    walk(val);
                }
            }
            Value::Array(arr) => arr.iter_mut().for_each(walk),
            _ => {}
        }
    }
    walk(&mut v);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_base64_image_keeps_text() {
        let big = "A".repeat(500);
        let v = serde_json::json!({ "text": "hello", "image": big });
        let out = sanitize(v);
        assert_eq!(out["text"], "hello");
        assert!(out["image"].as_str().unwrap().starts_with("[image omitted"));
    }
    #[test]
    fn keeps_short_or_spaced_image_values() {
        let v = serde_json::json!({ "image": "not really base64" });
        assert_eq!(sanitize(v)["image"], "not really base64");
    }
    #[test]
    fn act_set_matches_server() {
        assert!(is_act_tool("type_text"));
        assert!(!is_act_tool("list_windows"));
    }
}
```

- [ ] **Step 2: Run the sanitizer/act tests**

Run: `cargo test -p vantage-agent sanitize::tests 2>&1 | tail -20` (or `-- strips_base64`).
Expected: FAIL first if module not declared → add `mod mcp;` to `main.rs`, then PASS.

- [ ] **Step 3: Implement `McpClient` (spawn + list + convert + call)**

```rust
use anyhow::{Context, Result};
use rmcp::model::CallToolRequestParam;
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;

use crate::deepseek::{FunctionSchema, ToolSchema};

pub struct McpClient {
    service: rmcp::service::RunningService<rmcp::RoleClient, ()>,
    tools: Vec<ToolSchema>,
}

impl McpClient {
    pub async fn connect(server_bin: &str, allow_act: bool) -> Result<Self> {
        let mut cmd = tokio::process::Command::new(server_bin);
        if allow_act {
            cmd.env("VANTAGE_ALLOW_ACT", "1");
        }
        let transport = TokioChildProcess::new(cmd)
            .with_context(|| format!("failed to spawn MCP server at {server_bin:?} (build it: make build)"))?;
        let service = ()
            .serve(transport)
            .await
            .context("MCP initialize handshake failed")?;

        let listed = service.list_all_tools().await.context("list_tools failed")?;
        let tools = listed
            .into_iter()
            .map(|t| ToolSchema {
                kind: "function",
                function: FunctionSchema {
                    name: t.name.to_string(),
                    description: t.description.map(|d| d.to_string()).unwrap_or_default(),
                    parameters: serde_json::to_value(&*t.input_schema).unwrap_or(serde_json::json!({"type":"object"})),
                },
            })
            .collect();
        Ok(Self { service, tools })
    }

    pub fn tool_schemas(&self) -> &[ToolSchema] {
        &self.tools
    }

    pub async fn call(&self, name: &str, args: serde_json::Value) -> Result<String> {
        let arguments = args.as_object().cloned();
        let result = self
            .service
            .call_tool(CallToolRequestParam { name: name.to_string().into(), arguments })
            .await
            .with_context(|| format!("call_tool {name} failed"))?;

        // Prefer structured content; else join text blocks.
        let payload = if let Some(sc) = result.structured_content {
            sanitize(sc)
        } else {
            let text = result
                .content
                .iter()
                .filter_map(|b| b.as_text().map(|t| t.text.clone()))
                .collect::<Vec<_>>()
                .join("\n");
            serde_json::json!({ "text": text })
        };
        let mut out = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
        if result.is_error.unwrap_or(false) {
            out = format!("{{\"error\":true,\"result\":{out}}}");
        }
        Ok(out)
    }
}
```

Note: confirm `RunningService<RoleClient, ()>` path and that `()` implements the
client handler for `().serve(...)`. If rmcp requires an explicit handler type,
use the crate's no-op client handler; the behavior (a working client session) is
the deliverable. Confirm `TextContent` exposes `.text` (String).

- [ ] **Step 4: Build**

Run: `cargo build -p vantage-agent`
Expected: PASS.

- [ ] **Step 5: Integration test — spawn the real server, list tools**

Create `crates/agent/tests/mcp_live.rs`:

```rust
//! Spawns the built vantage-mcp and lists tools. Requires `make build` first.
//! Run: `cargo test -p vantage-agent --test mcp_live -- --ignored`

#[tokio::test]
#[ignore = "requires ./target/release/vantage-mcp (make build)"]
async fn lists_read_tools_and_act_when_allowed() {
    let read = vantage_agent_test::connect_count("./target/release/vantage-mcp", false).await;
    assert_eq!(read, 6, "read-only should expose 6 tools");
    let all = vantage_agent_test::connect_count("./target/release/vantage-mcp", true).await;
    assert_eq!(all, 12, "with act gate, 12 tools");
}
```

To make `McpClient` reachable from the integration test, expose a tiny test
helper. Simplest: add `pub mod` visibility is not available for a bin crate, so
instead add a **library target** to the agent crate (`src/lib.rs` re-exporting
`mcp`, `deepseek`, `config`, `agent`) and have `main.rs` use the lib. Update
`Cargo.toml` with `[lib] name = "vantage_agent"` and keep the `[[bin]]`. The test
then calls `vantage_agent::mcp::McpClient::connect(...).await` and counts
`tool_schemas().len()`. Adjust the test to use the real path:

```rust
use vantage_agent::mcp::McpClient;

#[tokio::test]
#[ignore = "requires ./target/release/vantage-mcp (make build)"]
async fn lists_read_tools_and_act_when_allowed() {
    let read = McpClient::connect("./target/release/vantage-mcp", false).await.unwrap();
    assert_eq!(read.tool_schemas().len(), 6);
    let all = McpClient::connect("./target/release/vantage-mcp", true).await.unwrap();
    assert_eq!(all.tool_schemas().len(), 12);
}
```

- [ ] **Step 6: Refactor to lib + bin, then run the integration test**

Add `crates/agent/src/lib.rs`:

```rust
pub mod agent; // added in Task 3
pub mod config;
pub mod deepseek;
pub mod mcp;
```

`main.rs` becomes `use vantage_agent::{...};` (Task 4 finalizes it). For now, get
the crate compiling as lib+bin, then:

Run: `make build` (server), then
`cargo test -p vantage-agent --test mcp_live -- --ignored`
Expected: PASS — 6 read tools, 12 with act. (This proves the MCP client end to end
without needing DeepSeek.)

- [ ] **Step 7: Commit**

```bash
git add crates/agent Cargo.toml Cargo.lock
git commit -m "feat(agent): MCP client (spawn server, list/convert/call tools, sanitize)"
```

---

### Task 3: Agent loop + act-tool confirmation

**Files:**
- Create: `crates/agent/src/agent.rs`
- Test: inline unit test for the confirmation gate helper

**Interfaces:**
- Consumes: `mcp::McpClient`, `deepseek::{DeepSeek, Message, Role}`.
- Produces: `agent::Agent { deepseek, mcp, auto_yes }` with `async run_turn(&mut self, messages: &mut Vec<Message>) -> Result<String>` (one user turn → final assistant text, executing tool calls), and a `confirm(name, args, auto_yes, is_tty) -> bool` helper.

- [ ] **Step 1: Confirmation helper (pure) + test**

```rust
use crate::deepseek::{DeepSeek, Message, Role, ToolCall};
use crate::mcp::{is_act_tool, McpClient};
use anyhow::Result;

const MAX_ITERS: usize = 25;

/// Decide whether an act tool call may proceed. Read tools always proceed
/// (callers only invoke this for act tools). Returns true to run it.
pub fn confirm(name: &str, args: &serde_json::Value, auto_yes: bool, is_tty: bool, prompt: impl FnOnce(&str) -> bool) -> bool {
    if auto_yes {
        return true;
    }
    if !is_tty {
        return false; // never fire side effects unattended without --yes
    }
    prompt(&format!("Run act tool `{name}` with {args}? [y/N] "))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn act_confirmation_rules() {
        let a = serde_json::json!({});
        assert!(confirm("type_text", &a, true, false, |_| false));      // --yes bypasses
        assert!(!confirm("type_text", &a, false, false, |_| true));     // non-tty, no --yes → refuse
        assert!(confirm("type_text", &a, false, true, |_| true));       // tty + user says y
        assert!(!confirm("type_text", &a, false, true, |_| false));     // tty + user says n
    }
}
```

- [ ] **Step 2: The loop**

```rust
pub struct Agent {
    pub deepseek: DeepSeek,
    pub mcp: McpClient,
    pub auto_yes: bool,
    pub is_tty: bool,
}

impl Agent {
    /// Run tool-calling iterations until the model returns a final text answer.
    /// `messages` already contains the system prompt and the new user message.
    pub async fn run_turn(&self, messages: &mut Vec<Message>) -> Result<String> {
        for _ in 0..MAX_ITERS {
            let reply = self.deepseek.chat(messages, self.mcp.tool_schemas()).await?;
            let tool_calls = reply.tool_calls.clone();
            messages.push(reply.clone());

            if tool_calls.is_empty() {
                return Ok(reply.content.unwrap_or_default());
            }
            for tc in tool_calls {
                let result = self.dispatch(&tc).await;
                messages.push(Message::tool(tc.id.clone(), result));
            }
        }
        Ok("(stopped: reached the tool-call iteration limit)".into())
    }

    async fn dispatch(&self, tc: &ToolCall) -> String {
        let name = &tc.function.name;
        let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
            .unwrap_or(serde_json::Value::Object(Default::default()));

        if is_act_tool(name) {
            let ok = confirm(name, &args, self.auto_yes, self.is_tty, |p| read_yes(p));
            if !ok {
                eprintln!("  ↳ refused act tool `{name}`");
                return "{\"refused\":true,\"reason\":\"not confirmed by user\"}".into();
            }
        }
        eprintln!("  ↳ {name}({args})");
        match self.mcp.call(name, args).await {
            Ok(s) => s,
            Err(e) => format!("{{\"error\":true,\"message\":{}}}", serde_json::Value::String(e.to_string())),
        }
    }
}

/// Blocking y/N read from the controlling terminal (used only in TTY mode).
fn read_yes(prompt: &str) -> bool {
    use std::io::Write;
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}
```

- [ ] **Step 3: Build + test**

Run: `cargo test -p vantage-agent agent`
Expected: PASS — `act_confirmation_rules`.

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/agent.rs crates/agent/src/lib.rs
git commit -m "feat(agent): tool-calling loop + act-tool confirmation gate"
```

---

### Task 4: CLI (REPL + one-shot) + system prompt + docs

**Files:**
- Rewrite: `crates/agent/src/main.rs`
- Create: `crates/agent/README.md`
- Modify: `README.md` (link to the agent), `CLAUDE.md` (mention the agent crate)

**Interfaces:**
- Consumes: all of `vantage_agent`.

- [ ] **Step 1: `main.rs` — arg parse, config, REPL/one-shot**

```rust
use std::io::IsTerminal;

use anyhow::{Context, Result};
use vantage_agent::agent::Agent;
use vantage_agent::config::AgentConfig;
use vantage_agent::deepseek::{DeepSeek, Message};
use vantage_agent::mcp::McpClient;

const SYSTEM_PROMPT: &str = "You are a desktop assistant driving a macOS/Linux machine through MCP tools \
(list_windows, read_window_text, capture_region, capture_window, list_displays, read_clipboard, and — when enabled — \
act tools: write_clipboard, type_text, click, move_mouse, key_press, focus_window). \
Call list_windows first to orient. Captures are TEXT-FIRST: request output=\"text\" (OCR) — you cannot see images. \
Prefer read_window_text over screenshots. Be explicit and conservative with act tools; they move the real mouse/keyboard. \
When done, answer the user concisely in plain text.";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_writer(std::io::stderr).with_env_filter(
        tracing_subscriber::EnvFilter::try_from_env("VANTAGE_AGENT_LOG").unwrap_or_else(|_| "warn".into()),
    ).init();

    let (cfg, task) = parse_args()?;
    let deepseek = DeepSeek::new(cfg.api_key.clone(), cfg.base_url.clone(), cfg.model.clone());
    let mcp = McpClient::connect(&cfg.server_bin, cfg.allow_act)
        .await
        .context("could not connect to vantage-mcp")?;
    eprintln!(
        "connected: {} tools{}",
        mcp.tool_schemas().len(),
        if cfg.allow_act { " (act enabled)" } else { "" }
    );
    let is_tty = std::io::stdin().is_terminal();
    let agent = Agent { deepseek, mcp, auto_yes: cfg.auto_yes, is_tty };

    let mut messages = vec![Message::system(SYSTEM_PROMPT)];
    match task {
        Some(t) => {
            messages.push(Message::user(t));
            let answer = agent.run_turn(&mut messages).await?;
            println!("{answer}");
        }
        None => repl(&agent, &mut messages).await?,
    }
    Ok(())
}
```

Add `parse_args() -> Result<(AgentConfig, Option<String>)>` (hand-rolled):
`--allow-act`, `--yes`, `--model <m>`, `--server <path>`, `--base-url <u>`,
`-h/--help`; the first non-flag positional is the one-shot task. `api_key` from
`DEEPSEEK_API_KEY` (bail with a clear message if unset). Defaults from
`AgentConfig` consts.

Add `repl(agent, messages)`:

```rust
async fn repl(agent: &Agent, messages: &mut Vec<Message>) -> Result<()> {
    let mut rl = rustyline::DefaultEditor::new()?;
    println!("vantage-agent REPL — type a task, or `exit`/Ctrl-D to quit.");
    loop {
        match rl.readline("> ") {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() { continue; }
                if line == "exit" || line == "quit" { break; }
                let _ = rl.add_history_entry(line);
                messages.push(Message::user(line));
                match agent.run_turn(messages).await {
                    Ok(answer) => println!("{answer}"),
                    Err(e) => eprintln!("error: {e:#}"),
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted)
            | Err(rustyline::error::ReadlineError::Eof) => break,
            Err(e) => { eprintln!("readline error: {e}"); break; }
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Build + full workspace check**

Run: `cargo build -p vantage-agent && cargo clippy -p vantage-agent --all-targets && cargo fmt --all --check`
Expected: PASS/clean.

- [ ] **Step 3: Live smoke (needs DEEPSEEK_API_KEY; else document)**

If `DEEPSEEK_API_KEY` is set:
Run: `make build && DEEPSEEK_API_KEY=… cargo run -p vantage-agent -- "list the open windows and tell me which app is focused"`
Expected: prints a per-tool-call trace (`↳ list_windows({})`) then a plain-text answer.

If no key: run `cargo run -p vantage-agent -- "hi"` and confirm it fails fast with
a clear "DEEPSEEK_API_KEY is not set" message; leave the LLM e2e to the operator.

- [ ] **Step 4: `crates/agent/README.md` + link from root**

Document: what it is, prerequisites (`DEEPSEEK_API_KEY`, `make build` for the
server), usage (one-shot + REPL), flags, the act-tools safety model, and an
example session. Add a short "## Agent" section to the root `README.md` linking
here, and a line in `CLAUDE.md` noting the `crates/agent` crate + that it talks
MCP to the server and uses DeepSeek.

- [ ] **Step 5: Commit**

```bash
git add crates/agent README.md CLAUDE.md Cargo.lock
git commit -m "feat(agent): REPL + one-shot CLI, system prompt, docs"
```

---

## Self-Review

**Spec coverage:**
- §2.1 MCP client (spawn+env gate, list→OpenAI schema, call→sanitized, is_act_tool) → Task 2. ✅
- §2.2 DeepSeek client (reqwest rustls, wire types, chat) → Task 1. ✅
- §2.3 agent loop (iterate, dispatch, confirm, iter cap) → Task 3. ✅
- §2.4 CLI (one-shot + REPL, flags, system prompt, key check) → Task 4. ✅
- §3 testing (schema/sanitizer/act units, confirm unit, MCP live 6/12, DeepSeek live) → Tasks 1–4. ✅
- §4 risks (act confirm + gate, image strip, defensive arg parse, server-path error) → Tasks 2–4. ✅

**Placeholder scan:** No TODO/TBD. The lib+bin split (Task 2 Step 6) is required so
the integration test can reach `McpClient`; stated explicitly. The DeepSeek live
smoke is conditional on a key being available — a stated verification boundary,
with a concrete no-key fallback (fail-fast check), not a gap. rmcp client-handler
type / `TextContent.text` are flagged to confirm at implementation.

**Type consistency:** `Message`/`ToolCall`/`FunctionCall`/`ToolSchema`/`FunctionSchema`
are defined in `deepseek.rs` (Task 1) and consumed by `mcp.rs` (Task 2) and
`agent.rs` (Task 3) unchanged. `McpClient::{connect, tool_schemas, call}` and
`is_act_tool`/`sanitize`/`ACT_TOOLS` (Task 2) are used by `agent.rs` (Task 3) and
`main.rs` (Task 4). `AgentConfig` fields flow from `parse_args` → `DeepSeek::new`
/ `McpClient::connect` / `Agent`. The act-tool name set matches the server's
`ACT_TOOL_NAMES` (six names).
