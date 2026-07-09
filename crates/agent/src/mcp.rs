//! MCP client: spawns the `vantage-mcp` server, lists its tools (converted to
//! OpenAI tool schemas), and calls them, sanitizing results for the LLM.

use anyhow::{Context, Result};
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use rmcp::{RoleClient, ServiceExt};
use serde_json::Value;

use crate::deepseek::{FunctionSchema, ToolSchema};

/// The server's act (mutating) tool names — kept in sync with the server's
/// `ACT_TOOL_NAMES`. Used to gate/confirm side-effecting calls.
pub const ACT_TOOLS: [&str; 6] = [
    "write_clipboard",
    "type_text",
    "click",
    "move_mouse",
    "key_press",
    "focus_window",
];

pub fn is_act_tool(name: &str) -> bool {
    ACT_TOOLS.contains(&name)
}

/// Replace long base64-looking values under any `image` key with a short
/// placeholder so screenshots don't blow the LLM context (DeepSeek is text-only).
pub fn sanitize(mut v: Value) -> Value {
    fn walk(v: &mut Value) {
        match v {
            Value::Object(map) => {
                for (k, val) in map.iter_mut() {
                    if k == "image" {
                        if let Value::String(s) = val {
                            if s.len() > 256 && !s.contains(' ') {
                                *val = Value::String(format!(
                                    "[image omitted: {} base64 chars]",
                                    s.len()
                                ));
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

pub struct McpClient {
    service: RunningService<RoleClient, ()>,
    tools: Vec<ToolSchema>,
}

impl McpClient {
    /// Spawn the server binary over stdio and complete the MCP handshake, then
    /// snapshot its tool list. `allow_act` forwards `VANTAGE_ALLOW_ACT=1` so the
    /// server mounts the act tools.
    pub async fn connect(server_bin: &str, allow_act: bool) -> Result<Self> {
        let mut cmd = tokio::process::Command::new(server_bin);
        if allow_act {
            cmd.env("VANTAGE_ALLOW_ACT", "1");
        }
        let transport = TokioChildProcess::new(cmd).with_context(|| {
            format!("failed to spawn MCP server at {server_bin:?} (build it: `make build`)")
        })?;
        let service = ().serve(transport).await.context("MCP initialize handshake failed")?;

        let listed = service
            .list_all_tools()
            .await
            .context("list_tools failed")?;
        let tools = listed
            .into_iter()
            .map(|t| ToolSchema {
                kind: "function",
                function: FunctionSchema {
                    name: t.name.to_string(),
                    description: t.description.map(|d| d.to_string()).unwrap_or_default(),
                    parameters: serde_json::to_value(&*t.input_schema)
                        .unwrap_or_else(|_| serde_json::json!({ "type": "object" })),
                },
            })
            .collect();
        Ok(Self { service, tools })
    }

    pub fn tool_schemas(&self) -> &[ToolSchema] {
        &self.tools
    }

    /// Cancel the MCP session and reap the server subprocess cleanly (avoids a
    /// noisy teardown panic when the transport is dropped during runtime shutdown).
    pub async fn shutdown(self) {
        let _ = self.service.cancel().await;
    }

    /// Call a tool and return a compact JSON string for the LLM (structured
    /// content preferred, text blocks otherwise; base64 images stripped).
    pub async fn call(&self, name: &str, args: Value) -> Result<String> {
        let mut params = CallToolRequestParams::new(name.to_string());
        params.arguments = args.as_object().cloned();
        let result = self
            .service
            .call_tool(params)
            .await
            .with_context(|| format!("call_tool {name} failed"))?;

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
        let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
        if result.is_error.unwrap_or(false) {
            Ok(format!("{{\"error\":true,\"result\":{body}}}"))
        } else {
            Ok(body)
        }
    }
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
        assert!(is_act_tool("focus_window"));
        assert!(!is_act_tool("list_windows"));
    }
}
