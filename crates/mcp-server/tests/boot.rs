//! Boots the built binary over stdio, performs the MCP `initialize` handshake
//! followed by a `tools/list` call, then closes stdin to trigger shutdown and
//! asserts:
//!   1. both JSON-RPC responses come back correctly on stdout, and
//!   2. EVERY non-empty line ever read from stdout (across both round-trips
//!      and whatever is emitted while shutting down) parses as a JSON-RPC
//!      object with `"jsonrpc":"2.0"` -- i.e. stdout never carries a stray
//!      log/plain-text line. Logging must be on stderr.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

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

    let tools_list = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "{init}").expect("write initialize");
        stdin.flush().expect("flush");
    }

    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("read initialize response line");

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

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "{tools_list}").expect("write tools/list");
        stdin.flush().expect("flush");
    }

    let mut line2 = String::new();
    reader
        .read_line(&mut line2)
        .expect("read tools/list response line");

    let parsed2: serde_json::Value =
        serde_json::from_str(line2.trim()).expect("stdout line 2 must be valid JSON-RPC");
    assert_eq!(
        parsed2["jsonrpc"], "2.0",
        "second stdout line must be JSON-RPC"
    );
    assert_eq!(parsed2["id"], 2);

    // Close stdin so the server observes EOF and begins shutting down.
    drop(child.stdin.take());

    // Drain whatever is left on stdout on a background thread so a server
    // that never exits after stdin EOF can't hang this test forever: we cap
    // the wait with a timeout instead of blocking indefinitely on EOF.
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut remaining = Vec::new();
        for line in reader.lines() {
            match line {
                Ok(l) => remaining.push(l),
                Err(_) => break,
            }
        }
        let _ = tx.send(remaining);
    });

    let remaining_lines = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("stdout must drain to EOF (server should exit on stdin EOF) within 5s");

    for l in &remaining_lines {
        let trimmed = l.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed: serde_json::Value = serde_json::from_str(trimmed).unwrap_or_else(|e| {
            panic!("stray non-JSON-RPC line found on stdout: {trimmed:?} ({e})")
        });
        assert_eq!(
            parsed["jsonrpc"], "2.0",
            "every stdout line must be JSON-RPC, got: {trimmed:?}"
        );
    }

    child.kill().ok();
    child.wait().ok();
}
