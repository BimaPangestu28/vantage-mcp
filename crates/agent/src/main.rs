//! `vantage-agent`: a DeepSeek-driven agent that uses the vantage-mcp desktop
//! tools over MCP. One-shot (`vantage-agent "task"`) or interactive REPL.

use std::io::IsTerminal;

use anyhow::{bail, Context, Result};
use vantage_agent::agent::Agent;
use vantage_agent::config::AgentConfig;
use vantage_agent::deepseek::{DeepSeek, Message};
use vantage_agent::mcp::McpClient;

const SYSTEM_PROMPT: &str = "You are a desktop assistant driving a macOS/Linux machine through MCP tools \
(list_windows, read_window_text, capture_region, capture_window, list_displays, read_clipboard, and — when \
enabled — the act tools write_clipboard, type_text, click, move_mouse, key_press, focus_window). \
Call list_windows first to orient. Captures are TEXT-FIRST: request output=\"text\" (OCR) because you cannot \
see images. Prefer read_window_text over screenshots. Be explicit and conservative with act tools — they move \
the real mouse and keyboard. When you are done, answer the user concisely in plain text.";

const HELP: &str = "vantage-agent — DeepSeek agent over the vantage-mcp desktop tools

USAGE:
    vantage-agent [OPTIONS] [TASK]

    With TASK: run it once and print the answer. Without: interactive REPL.

OPTIONS:
    --allow-act           Enable act tools (forwards VANTAGE_ALLOW_ACT=1 to the server)
    --yes                 Skip the per-call confirmation for act tools
    --model <MODEL>       DeepSeek model (default: deepseek-chat)
    --server <PATH>       Path to the vantage-mcp binary (default: ./target/release/vantage-mcp)
    --base-url <URL>      API base URL (default: https://api.deepseek.com)
    -h, --help            Show this help

ENV:
    DEEPSEEK_API_KEY      Required.
    VANTAGE_AGENT_LOG     Log filter (default: warn).";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("VANTAGE_AGENT_LOG")
                .unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let (cfg, task) = match parse_args()? {
        Some(v) => v,
        None => {
            println!("{HELP}");
            return Ok(());
        }
    };

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
    let agent = Agent {
        deepseek,
        mcp,
        auto_yes: cfg.auto_yes,
        is_tty,
    };

    let mut messages = vec![Message::system(SYSTEM_PROMPT)];
    match task {
        Some(t) => {
            messages.push(Message::user(t));
            let answer = agent.run_turn(&mut messages).await?;
            println!("{answer}");
        }
        None => repl(&agent, &mut messages).await?,
    }

    agent.mcp.shutdown().await;
    Ok(())
}

/// Parse CLI args. Returns `Ok(None)` when `--help` was requested.
fn parse_args() -> Result<Option<(AgentConfig, Option<String>)>> {
    let mut allow_act = false;
    let mut auto_yes = false;
    let mut model = AgentConfig::DEFAULT_MODEL.to_string();
    let mut server_bin = AgentConfig::DEFAULT_SERVER.to_string();
    let mut base_url = AgentConfig::DEFAULT_BASE_URL.to_string();
    let mut task: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--allow-act" => allow_act = true,
            "--yes" | "-y" => auto_yes = true,
            "-h" | "--help" => return Ok(None),
            "--model" => model = next_value(&mut args, "--model")?,
            "--server" => server_bin = next_value(&mut args, "--server")?,
            "--base-url" => base_url = next_value(&mut args, "--base-url")?,
            other if other.starts_with('-') => bail!("unknown option: {other} (see --help)"),
            other => {
                // First positional is the one-shot task; join the rest as one string.
                let mut t = other.to_string();
                for extra in args.by_ref() {
                    t.push(' ');
                    t.push_str(&extra);
                }
                task = Some(t);
            }
        }
    }

    let api_key = std::env::var("DEEPSEEK_API_KEY")
        .map_err(|_| anyhow::anyhow!("DEEPSEEK_API_KEY is not set (export it and retry)"))?;

    Ok(Some((
        AgentConfig {
            api_key,
            base_url,
            model,
            server_bin,
            allow_act,
            auto_yes,
        },
        task,
    )))
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .ok_or_else(|| anyhow::anyhow!("{flag} requires a value"))
}

async fn repl(agent: &Agent, messages: &mut Vec<Message>) -> Result<()> {
    let mut rl = rustyline::DefaultEditor::new()?;
    println!("vantage-agent REPL — type a task, or `exit` / Ctrl-D to quit.");
    loop {
        match rl.readline("> ") {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if line == "exit" || line == "quit" {
                    break;
                }
                let _ = rl.add_history_entry(line);
                messages.push(Message::user(line.to_string()));
                match agent.run_turn(messages).await {
                    Ok(answer) => println!("{answer}"),
                    Err(e) => eprintln!("error: {e:#}"),
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted)
            | Err(rustyline::error::ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("readline error: {e}");
                break;
            }
        }
    }
    Ok(())
}
