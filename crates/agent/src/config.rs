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
