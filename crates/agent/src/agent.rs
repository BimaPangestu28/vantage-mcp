//! The tool-calling agent loop: DeepSeek decides tool calls, we execute them via
//! MCP and feed results back until the model returns a final text answer. Act
//! tools require confirmation before they run.

use anyhow::Result;
use serde_json::Value;

use crate::deepseek::{DeepSeek, Message, ToolCall};
use crate::mcp::{is_act_tool, McpClient};

/// Hard cap on tool-calling iterations per user turn (runaway-loop backstop).
const MAX_ITERS: usize = 25;

pub struct Agent {
    pub deepseek: DeepSeek,
    pub mcp: McpClient,
    pub auto_yes: bool,
    pub is_tty: bool,
}

impl Agent {
    /// Run tool-calling iterations until the model returns a final text answer.
    /// `messages` must already contain the system prompt and the new user message;
    /// it is extended in place with the assistant/tool turns.
    pub async fn run_turn(&self, messages: &mut Vec<Message>) -> Result<String> {
        for _ in 0..MAX_ITERS {
            let reply = self
                .deepseek
                .chat(messages, self.mcp.tool_schemas())
                .await?;
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

    /// Execute one tool call, returning the JSON string to feed back to the model.
    async fn dispatch(&self, tc: &ToolCall) -> String {
        let name = &tc.function.name;
        let args: Value = serde_json::from_str(&tc.function.arguments)
            .unwrap_or(Value::Object(Default::default()));

        if is_act_tool(name) && !confirm(name, &args, self.auto_yes, self.is_tty, |p| read_yes(p)) {
            eprintln!("  \u{21b3} refused act tool `{name}`");
            return "{\"refused\":true,\"reason\":\"not confirmed by user\"}".into();
        }

        eprintln!("  \u{21b3} {name}({args})");
        match self.mcp.call(name, args).await {
            Ok(s) => s,
            Err(e) => format!(
                "{{\"error\":true,\"message\":{}}}",
                Value::String(e.to_string())
            ),
        }
    }
}

/// Decide whether an act tool call may proceed. `--yes` bypasses; a non-TTY
/// session never fires side effects without `--yes`; otherwise prompt the user.
pub fn confirm(
    name: &str,
    args: &Value,
    auto_yes: bool,
    is_tty: bool,
    prompt: impl FnOnce(&str) -> bool,
) -> bool {
    if auto_yes {
        return true;
    }
    if !is_tty {
        return false;
    }
    prompt(&format!("Run act tool `{name}` with {args}? [y/N] "))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn act_confirmation_rules() {
        let a = serde_json::json!({});
        // --yes bypasses the prompt entirely.
        assert!(confirm("type_text", &a, true, false, |_| false));
        // Non-TTY without --yes never fires side effects.
        assert!(!confirm("type_text", &a, false, false, |_| true));
        // TTY: follow the user's answer.
        assert!(confirm("type_text", &a, false, true, |_| true));
        assert!(!confirm("type_text", &a, false, true, |_| false));
    }
}
