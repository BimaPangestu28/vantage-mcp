//! Spawns the built vantage-mcp and lists tools — proves the MCP client end to
//! end without needing DeepSeek. Requires `make build` first.
//! Run: `cargo test -p vantage-agent --test mcp_live -- --ignored`

use vantage_agent::mcp::McpClient;

/// The workspace-root server binary. Integration tests run with CWD at the crate
/// dir, so resolve it relative to this crate's manifest (../../target/...).
fn server() -> String {
    format!(
        "{}/../../target/release/vantage-mcp",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires ./target/release/vantage-mcp (make build)"]
async fn lists_read_tools_and_act_when_allowed() {
    let read = McpClient::connect(&server(), false)
        .await
        .expect("connect (read-only)");
    assert_eq!(
        read.tool_schemas().len(),
        6,
        "read-only should expose 6 tools"
    );

    let all = McpClient::connect(&server(), true)
        .await
        .expect("connect (act enabled)");
    assert_eq!(all.tool_schemas().len(), 12, "with the act gate, 12 tools");
    read.shutdown().await;
    all.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires ./target/release/vantage-mcp + a desktop session"]
async fn calls_list_windows() {
    let client = McpClient::connect(&server(), false).await.expect("connect");
    let out = client
        .call(
            "list_windows",
            serde_json::json!({ "on_screen_only": true }),
        )
        .await
        .expect("call list_windows");
    // Result is a JSON string with a `windows` array.
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert!(
        v.get("windows").is_some(),
        "expected a windows field: {out}"
    );
    client.shutdown().await;
}
