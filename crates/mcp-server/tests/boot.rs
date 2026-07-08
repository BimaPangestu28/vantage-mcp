//! Boots the built binary over stdio, performs the MCP `initialize` handshake
//! by writing one JSON-RPC line to stdin, and asserts:
//!   1. a JSON-RPC response comes back on stdout, and
//!   2. stdout contains ONLY JSON-RPC (no stray log/print lines).
//!      Logging must be on stderr.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn initialize_handshake_and_clean_stdout() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_vantage-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn vantage-mcp");

    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "boot-test", "version": "0.0.0" }
        }
    });

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "{init}").expect("write initialize");
        stdin.flush().expect("flush");
    }

    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read response line");

    let parsed: serde_json::Value =
        serde_json::from_str(line.trim()).expect("stdout line 1 must be valid JSON-RPC");
    assert_eq!(
        parsed["jsonrpc"], "2.0",
        "first stdout line must be JSON-RPC"
    );
    assert_eq!(parsed["id"], 1);
    assert!(
        parsed.get("result").is_some(),
        "initialize must return a result"
    );

    child.kill().ok();
    child.wait().ok();
}
