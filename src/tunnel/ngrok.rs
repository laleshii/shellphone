use super::{TunnelResult, parse_url_from_output};
use std::process::Stdio;
use tokio::process::Command;

pub async fn start(local_port: u16) -> TunnelResult {
    let child = Command::new("ngrok")
        .args([
            "http",
            &local_port.to_string(),
            "--log", "stdout",
            "--log-format", "term",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    match child {
        Ok(child) => {
            parse_url_from_output(child, |url| url.contains(".ngrok"), 15, "ngrok").await
        }
        Err(e) => TunnelResult::Unavailable(format!("ngrok: {e}")),
    }
}
