//! Minimal DeepSeek (OpenAI-compatible) chat client + wire types.

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
    pub fn system(s: impl Into<String>) -> Self {
        Self::text(Role::System, s)
    }
    pub fn user(s: impl Into<String>) -> Self {
        Self::text(Role::User, s)
    }
    pub fn tool(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            tool_calls: vec![],
            tool_call_id: Some(id.into()),
        }
    }
    fn text(role: Role, s: impl Into<String>) -> Self {
        Self {
            role,
            content: Some(s.into()),
            tool_calls: vec![],
            tool_call_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "default_function")]
    pub kind: String,
    pub function: FunctionCall,
}
fn default_function() -> String {
    "function".into()
}

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
        Self {
            http: reqwest::Client::new(),
            api_key,
            base_url,
            model,
        }
    }

    /// One chat completion round-trip. Returns the assistant `Message` (which may
    /// carry `tool_calls` instead of `content`).
    pub async fn chat(&self, messages: &[Message], tools: &[ToolSchema]) -> anyhow::Result<Message> {
        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            messages: &'a [Message],
            tools: &'a [ToolSchema],
            tool_choice: &'a str,
            temperature: f32,
        }
        #[derive(Deserialize)]
        struct Resp {
            choices: Vec<Choice>,
        }
        #[derive(Deserialize)]
        struct Choice {
            message: Message,
        }

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&Req {
                model: &self.model,
                messages,
                tools,
                tool_choice: "auto",
                temperature: 0.2,
            })
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
        assert!(
            v.get("tool_calls").is_none(),
            "empty tool_calls must be omitted"
        );
    }

    #[test]
    fn assistant_tool_call_roundtrips() {
        let json = r#"{"role":"assistant","content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":"list_windows","arguments":"{}"}}]}"#;
        let m: Message = serde_json::from_str(json).unwrap();
        assert_eq!(m.role, Role::Assistant);
        assert_eq!(m.tool_calls[0].function.name, "list_windows");
        assert_eq!(m.tool_calls[0].function.arguments, "{}");
    }
}
