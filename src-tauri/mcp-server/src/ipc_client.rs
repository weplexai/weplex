use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

/// Client for communicating with the Tauri IPC socket via JSON Lines protocol.
pub struct IpcClient {
    socket_path: String,
    /// Stream used for writing requests
    writer: Option<UnixStream>,
    /// Buffered reader wrapping a cloned stream for reading responses
    reader: Option<BufReader<UnixStream>>,
}

impl IpcClient {
    pub fn new(socket_path: String) -> Self {
        Self {
            socket_path,
            writer: None,
            reader: None,
        }
    }

    /// Send a JSON-RPC request to the IPC socket and return the response.
    /// Lazily connects on first use.
    pub fn send(&mut self, request: Value) -> Result<Value, String> {
        if self.socket_path.is_empty() {
            return Err("Not in a pipeline context".to_string());
        }

        // Lazy connect
        if self.writer.is_none() {
            let stream = UnixStream::connect(&self.socket_path).map_err(|e| {
                format!(
                    "Cannot connect to Weplex. The pipeline may have ended or the app is not running. ({})",
                    e
                )
            })?;
            stream
                .set_read_timeout(Some(Duration::from_secs(30)))
                .map_err(|e| format!("Failed to set read timeout: {}", e))?;

            // Clone the stream: one for writing, one for reading
            let reader_stream = stream
                .try_clone()
                .map_err(|e| format!("Failed to clone stream: {}", e))?;

            self.writer = Some(stream);
            self.reader = Some(BufReader::new(reader_stream));
        }

        let writer = self.writer.as_mut().unwrap();

        // Write request as a single JSON line
        let mut line = serde_json::to_string(&request)
            .map_err(|e| format!("Failed to serialize request: {}", e))?;
        line.push('\n');
        writer
            .write_all(line.as_bytes())
            .map_err(|e| format!("Failed to write to socket: {}", e))?;
        writer
            .flush()
            .map_err(|e| format!("Failed to flush socket: {}", e))?;

        // Read response (single JSON line)
        let reader = self.reader.as_mut().unwrap();
        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .map_err(|e| format!("Failed to read from socket: {}", e))?;

        if response_line.is_empty() {
            return Err("Socket closed unexpectedly".to_string());
        }

        serde_json::from_str(&response_line)
            .map_err(|e| format!("Failed to parse IPC response: {}", e))
    }
}
