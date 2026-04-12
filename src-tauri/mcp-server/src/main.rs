mod ipc_client;
mod mcp_protocol;
mod tools;

use ipc_client::IpcClient;
use mcp_protocol::{JsonRpcRequest, JsonRpcResponse};
use std::io::{self, BufRead, Write};

fn main() {
    // Read session context from environment
    let session_id = std::env::var("WEPLEX_SESSION_ID").unwrap_or_default();

    // Global socket path for MCP v2 cross-session tools
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let global_socket_path = format!("{}/.weplex/ipc/global.sock", home);
    let global_exists = std::path::Path::new(&global_socket_path).exists();

    eprintln!(
        "[weplex-mcp] starting (session={}, global={})",
        if session_id.is_empty() { "<none>" } else { &session_id },
        if global_exists { &global_socket_path } else { "<not found>" },
    );

    let mut global_ipc = if global_exists {
        Some(IpcClient::new(global_socket_path.clone()))
    } else {
        None
    };

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[weplex-mcp] stdin read error: {}", e);
                break;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[weplex-mcp] invalid JSON-RPC: {}", e);
                let resp = JsonRpcResponse::error(
                    None,
                    -32700,
                    format!("Parse error: {}", e),
                );
                write_response(&mut stdout_lock, &resp);
                continue;
            }
        };

        let response = handle_request(
            &request,
            &session_id,
            if global_exists { &global_socket_path } else { "" },
            &mut global_ipc,
        );

        // Notifications (no id) don't get a response
        if let Some(resp) = response {
            write_response(&mut stdout_lock, &resp);
        }
    }

    eprintln!("[weplex-mcp] shutting down");
}

fn handle_request(
    request: &JsonRpcRequest,
    session_id: &str,
    global_socket_path: &str,
    global_ipc: &mut Option<IpcClient>,
) -> Option<JsonRpcResponse> {
    match request.method.as_str() {
        "initialize" => Some(JsonRpcResponse::success(
            request.id.clone(),
            mcp_protocol::initialize_result(),
        )),

        // Notification — no response expected
        "notifications/initialized" => None,

        "tools/list" => {
            let tools = tools::list_tools(global_socket_path);
            Some(JsonRpcResponse::success(request.id.clone(), tools))
        }

        "tools/call" => {
            let params = request.params.as_ref();
            let tool_name = params
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let arguments = params
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(serde_json::json!({}));

            match tools::call_tool(
                tool_name, &arguments, session_id, global_ipc,
            ) {
                Ok(result) => Some(JsonRpcResponse::success(request.id.clone(), result)),
                Err(msg) => {
                    // Return error as tool result content (not JSON-RPC error)
                    // so the agent sees the message
                    let result = serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": msg
                        }],
                        "isError": true
                    });
                    Some(JsonRpcResponse::success(request.id.clone(), result))
                }
            }
        }

        _ => {
            eprintln!("[weplex-mcp] unknown method: {}", request.method);
            Some(JsonRpcResponse::error(
                request.id.clone(),
                -32601,
                format!("Method not found: {}", request.method),
            ))
        }
    }
}

fn write_response(writer: &mut impl Write, response: &JsonRpcResponse) {
    match serde_json::to_string(response) {
        Ok(json) => {
            let _ = writeln!(writer, "{}", json);
            let _ = writer.flush();
        }
        Err(e) => {
            eprintln!("[weplex-mcp] failed to serialize response: {}", e);
        }
    }
}
