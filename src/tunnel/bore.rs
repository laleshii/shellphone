use super::{TunnelResult, parse_url_from_output};
use std::process::Stdio;
use tokio::process::Command;

pub async fn start(local_port: u16, server: &str) -> TunnelResult {
    let child = Command::new("bore")
        .args([
            "local",
            &local_port.to_string(),
            "--to",
            server,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    match child {
        Ok(child) => {
            parse_url_from_output(child, |_| true, 15, "bore").await
        }
        Err(e) => TunnelResult::Unavailable(format!("bore: {e}")),
    }
}
