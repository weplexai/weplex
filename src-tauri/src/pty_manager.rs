use base64::Engine as _;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;

/// A single PTY session with its writer, master handle, and liveness flag.
pub struct PtySession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    alive: Arc<AtomicBool>,
}

/// Manages all PTY sessions, keyed by session ID.
pub struct PtyManager {
    sessions: HashMap<u32, PtySession>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn create(
        &mut self,
        session_id: u32,
        cols: u16,
        rows: u16,
        command: Option<String>,
        cwd: Option<String>,
        env_vars: Option<HashMap<String, String>>,
        app: tauri::AppHandle,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let pty_system = native_pty_system();

        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());

        let mut cmd = CommandBuilder::new(&shell);
        cmd.arg("-l");

        // Essential terminal identity vars — Tauri is a GUI app so these are
        // not inherited from any parent terminal. Without them TUI apps like
        // Claude Code use wrong escape sequences and leave ghost text on screen.
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        // Apply custom environment variables (e.g. CLAUDE_CONFIG_DIR for profiles)
        if let Some(vars) = env_vars {
            for (key, value) in vars {
                cmd.env(key, value);
            }
        }

        // Set working directory (proper tilde expansion)
        let work_dir = cwd
            .map(|d| {
                if d == "~" {
                    home.clone()
                } else if let Some(rest) = d.strip_prefix("~/") {
                    format!("{}/{}", home, rest)
                } else if let Some(rest) = d.strip_prefix("~") {
                    // Handle ~Documents/LLM → /home/user/Documents/LLM
                    format!("{}/{}", home, rest)
                } else {
                    d
                }
            })
            .unwrap_or_else(|| home.clone());
        cmd.cwd(&work_dir);

        // Spawn the shell process in the PTY
        let _child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave); // Release slave side

        let mut writer = pair.master.take_writer()?;

        // Explicit cd to ensure correct directory (macOS zsh session restore can override cmd.cwd)
        // Escape single quotes to prevent shell injection: ' → '\''
        let escaped_dir = work_dir.replace("'", "'\\''");
        let cd_cmd = format!("cd '{}'\n", escaped_dir);
        writer.write_all(cd_cmd.as_bytes())?;
        writer.flush()?;

        // If a command was specified, send it to the shell
        if let Some(ref cmd_str) = command
            && !cmd_str.is_empty()
        {
            let cmd_line = format!("{}\n", cmd_str);
            writer.write_all(cmd_line.as_bytes())?;
            writer.flush()?;
        }

        // Spawn a reader thread that forwards PTY output to frontend
        // Uses buffering + throttle to avoid flooding the WebView IPC queue
        let mut reader = pair.master.try_clone_reader()?;
        let event_name = format!("pty-output-{}", session_id);
        let alive = Arc::new(AtomicBool::new(true));
        let alive_clone = alive.clone();

        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            let throttle = std::time::Duration::from_millis(8);
            let burst_threshold = 32 * 1024; // 32 KB = high-throughput burst

            loop {
                if !alive_clone.load(Ordering::Relaxed) {
                    break;
                }

                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        // Send raw bytes as base64. The JS side decodes this into
                        // a Uint8Array and calls term.write(bytes), which lets
                        // xterm.js handle UTF-8 decoding with its own stateful
                        // parser — the same approach VS Code uses. This avoids
                        // any corruption of multibyte sequences split at buffer
                        // boundaries (which from_utf8_lossy would mangle).
                        let encoded = base64::engine::general_purpose::STANDARD.encode(&buf[..n]);
                        if app.emit(&event_name, encoded).is_err() {
                            break;
                        }
                        // Throttle only on large bursts to prevent IPC queue flood
                        if n >= burst_threshold {
                            std::thread::sleep(throttle);
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        self.sessions.insert(
            session_id,
            PtySession {
                writer,
                master: pair.master,
                alive,
            },
        );

        Ok(())
    }

    pub fn write(&mut self, session_id: u32, data: &str) -> Result<(), Box<dyn std::error::Error>> {
        let session = self
            .sessions
            .get_mut(&session_id)
            .ok_or("Session not found")?;
        session.writer.write_all(data.as_bytes())?;
        session.writer.flush()?;
        Ok(())
    }

    pub fn resize(
        &mut self,
        session_id: u32,
        cols: u16,
        rows: u16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let session = self
            .sessions
            .get_mut(&session_id)
            .ok_or("Session not found")?;
        session.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    pub fn kill(&mut self, session_id: u32) -> Result<(), Box<dyn std::error::Error>> {
        let session = self
            .sessions
            .remove(&session_id)
            .ok_or("Session not found")?;
        // Signal the reader thread to stop before dropping
        session.alive.store(false, Ordering::Relaxed);
        // Dropping the session closes the PTY, which kills the child process
        Ok(())
    }
}
