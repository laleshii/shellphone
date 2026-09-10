use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

const SCREEN_REPLAY_CAP: usize = 4 * 1024 * 1024;

pub enum ControllerEvent {
    Frame(Vec<u8>),
    Closed { reason: String },
}

pub struct Controller {
    pub pane_id: String,
    pub cols: u16,
    pub rows: u16,
    child: Child,
    stdin: ChildStdin,
    reader: Option<JoinHandle<()>>,
    releasing: Arc<AtomicBool>,
    screen: Arc<Mutex<Vec<u8>>>,
}

impl Controller {
    pub async fn spawn(
        herdr_bin: &Path,
        socket: &Path,
        pane_id: &str,
        cols: u16,
        rows: u16,
        on_event: impl Fn(ControllerEvent) + Send + Sync + 'static,
    ) -> anyhow::Result<Self> {
        let mut child = Command::new(herdr_bin)
            .args(["terminal", "session", "control", pane_id, "--takeover"])
            .arg("--cols")
            .arg(cols.to_string())
            .arg("--rows")
            .arg(rows.to_string())
            .env("HERDR_SOCKET_PATH", socket)
            .env_remove("HERDR_ENV")
            .env_remove("HERDR_PANE_ID")
            .env_remove("HERDR_TAB_ID")
            .env_remove("HERDR_WORKSPACE_ID")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        let screen = Arc::new(Mutex::new(Vec::new()));
        let releasing = Arc::new(AtomicBool::new(false));

        let stderr_pane = pane_id.to_string();
        let stderr_lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let stderr_lines2 = stderr_lines.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!("herdr control {stderr_pane}: {line}");
                stderr_lines2.lock().await.push(line);
            }
        });

        let screen2 = screen.clone();
        let releasing2 = releasing.clone();
        let reader = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            let mut reason = String::from("stream ended");
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(record) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                match record["type"].as_str() {
                    Some("terminal.frame") => {
                        let Some(b64) = record["bytes"].as_str() else { continue };
                        let Ok(bytes) = STANDARD.decode(b64) else { continue };
                        let full = record["full"].as_bool().unwrap_or(false);
                        {
                            let mut screen = screen2.lock().await;
                            if full {
                                screen.clear();
                            }
                            if screen.len() + bytes.len() <= SCREEN_REPLAY_CAP {
                                screen.extend_from_slice(&bytes);
                            }
                        }
                        on_event(ControllerEvent::Frame(bytes));
                    }
                    Some("terminal.closed") => {
                        reason = record["reason"]
                            .as_str()
                            .unwrap_or("closed")
                            .to_string();
                        break;
                    }
                    _ => {}
                }
            }
            if !releasing2.load(Ordering::SeqCst) {
                let stderr_lines = stderr_lines.lock().await;
                if let Some(last) = stderr_lines.last() {
                    reason = last.clone();
                }
                on_event(ControllerEvent::Closed { reason });
            }
        });

        Ok(Self {
            pane_id: pane_id.to_string(),
            cols,
            rows,
            child,
            stdin,
            reader: Some(reader),
            releasing,
            screen,
        })
    }

    pub async fn screen(&self) -> Vec<u8> {
        self.screen.lock().await.clone()
    }

    pub async fn screen_is_stale(&self) -> bool {
        self.screen.lock().await.len() >= SCREEN_REPLAY_CAP
    }

    pub async fn input(&mut self, data: &str) -> anyhow::Result<()> {
        self.send(json!({
            "type": "terminal.input",
            "bytes": STANDARD.encode(data.as_bytes()),
        }))
        .await
    }

    pub async fn resize(&mut self, cols: u16, rows: u16) -> anyhow::Result<()> {
        self.cols = cols;
        self.rows = rows;
        self.send(json!({ "type": "terminal.resize", "cols": cols, "rows": rows }))
            .await
    }

    pub async fn repaint(&mut self) -> anyhow::Result<()> {
        let (cols, rows) = (self.cols, self.rows);
        self.resize(cols, rows).await
    }

    // herdr applies `lines` exactly in the normal buffer but turns each
    // command into a single wheel tick on the alternate screen (verified on
    // 0.9.0), so send one command per line to move the same distance in both.
    pub async fn scroll(&mut self, direction: &str, lines: u32) -> anyhow::Result<()> {
        let command = serde_json::to_string(
            &json!({ "type": "terminal.scroll", "direction": direction, "lines": 1 }),
        )? + "\n";
        let batch = command.repeat(lines as usize);
        self.stdin.write_all(batch.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    pub async fn release(mut self) {
        self.releasing.store(true, Ordering::SeqCst);
        let _ = self.send(json!({ "type": "terminal.release" })).await;
        let exited = tokio::time::timeout(std::time::Duration::from_secs(2), self.child.wait()).await;
        if exited.is_err() {
            let _ = self.child.kill().await;
        }
        if let Some(reader) = self.reader.take() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), reader).await;
        }
    }

    async fn send(&mut self, command: Value) -> anyhow::Result<()> {
        let line = serde_json::to_string(&command)? + "\n";
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }
}
