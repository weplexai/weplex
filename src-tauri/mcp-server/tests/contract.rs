//! Integration test that spawns the compiled `weplex-mcp` binary and verifies
//! its `tools/list` response matches the contract in `weplex-mcp-contract`.
//!
//! This is the regression guard against silently shipping a stale binary whose
//! exported tool names drift from what the Tauri hook server requests. If you
//! rename a tool in `mcp-contract`, this test forces a rebuild before the
//! binary can pass CI / release.sh.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use weplex_mcp_contract::TOOL_LOG_ACTIVITY;

/// Read one JSON-RPC frame (single line) from a buffered stdout reader.
fn read_line(reader: &mut BufReader<std::process::ChildStdout>) -> String {
    let mut line = String::new();
    reader.read_line(&mut line).expect("read stdout line");
    line
}

#[test]
fn tools_list_exposes_log_activity() {
    let bin = env!("CARGO_BIN_EXE_weplex-mcp");

    let mut child = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn weplex-mcp");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05","capabilities":{{}},"clientInfo":{{"name":"contract-test","version":"0"}}}}}}"#
        )
        .unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{{}}}}"#
        )
        .unwrap();
    }

    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    let _initialize = read_line(&mut reader);
    let list_response = read_line(&mut reader);

    // Close stdin so the binary exits.
    drop(child.stdin.take());
    let _ = child.wait_timeout(Duration::from_secs(2));

    let parsed: serde_json::Value =
        serde_json::from_str(&list_response).expect("tools/list response is JSON");
    let tools = parsed
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| t.as_array())
        .expect("response has result.tools array");

    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
        .collect();

    assert!(
        names.contains(&TOOL_LOG_ACTIVITY),
        "weplex-mcp must expose `{}` (got {:?}). \
         If you renamed the constant, rebuild the binary before running this test.",
        TOOL_LOG_ACTIVITY,
        names
    );
}

// Minimal `wait_timeout` shim — `std::process::Child` has no built-in timeout.
trait WaitTimeoutExt {
    fn wait_timeout(&mut self, dur: Duration) -> std::io::Result<Option<std::process::ExitStatus>>;
}

impl WaitTimeoutExt for std::process::Child {
    fn wait_timeout(&mut self, dur: Duration) -> std::io::Result<Option<std::process::ExitStatus>> {
        let start = std::time::Instant::now();
        loop {
            match self.try_wait()? {
                Some(status) => return Ok(Some(status)),
                None if start.elapsed() >= dur => {
                    let _ = self.kill();
                    return Ok(None);
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }
}
