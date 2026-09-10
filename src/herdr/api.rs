use anyhow::Context;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct Client {
    socket: PathBuf,
}

impl Client {
    pub fn new(socket: PathBuf) -> Self {
        Self { socket }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket
    }

    pub async fn request(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let stream = UnixStream::connect(&self.socket)
            .await
            .with_context(|| format!("cannot connect to herdr socket {}", self.socket.display()))?;
        let (rd, mut wr) = stream.into_split();
        let line = serde_json::to_string(&json!({
            "id": "shellphone",
            "method": method,
            "params": params,
        }))? + "\n";
        wr.write_all(line.as_bytes()).await?;

        let mut reader = BufReader::new(rd);
        let mut response = String::new();
        reader.read_line(&mut response).await?;
        if response.is_empty() {
            anyhow::bail!("herdr closed the connection during {method}");
        }
        let mut value: Value = serde_json::from_str(&response)?;
        if let Some(err) = value.get("error") {
            anyhow::bail!(
                "herdr {method}: {} ({})",
                err["message"].as_str().unwrap_or("unknown error"),
                err["code"].as_str().unwrap_or("error")
            );
        }
        Ok(value["result"].take())
    }

    pub async fn subscribe(&self, subscriptions: Vec<Value>) -> anyhow::Result<mpsc::Receiver<Value>> {
        let stream = UnixStream::connect(&self.socket)
            .await
            .with_context(|| format!("cannot connect to herdr socket {}", self.socket.display()))?;
        let (rd, mut wr) = stream.into_split();
        let line = serde_json::to_string(&json!({
            "id": "shellphone-events",
            "method": "events.subscribe",
            "params": { "subscriptions": subscriptions },
        }))? + "\n";
        wr.write_all(line.as_bytes()).await?;

        let mut reader = BufReader::new(rd);
        let mut ack = String::new();
        reader.read_line(&mut ack).await?;
        let ack: Value = serde_json::from_str(&ack).context("invalid subscription ack")?;
        if let Some(err) = ack.get("error") {
            anyhow::bail!("herdr events.subscribe: {}", err["message"]);
        }

        let (tx, rx) = mpsc::channel(256);
        tokio::spawn(async move {
            let _keep_writer_open = wr;
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if let Ok(event) = serde_json::from_str::<Value>(&line)
                            && tx.send(event).await.is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });
        Ok(rx)
    }
}
