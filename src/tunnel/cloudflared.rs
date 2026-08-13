use super::{TunnelResult, parse_url_from_output};
use std::process::Stdio;
use tokio::process::Command;

pub async fn start(local_port: u16) -> TunnelResult {
    let child = Command::new("cloudflared")
        .args(["tunnel", "--url", &format!("http://127.0.0.1:{local_port}")])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    match child {
        Ok(child) => {
            parse_url_from_output(child, |url| url.contains(".trycloudflare.com"), 15, "cloudflared")
                .await
        }
        Err(e) => TunnelResult::Unavailable(format!("cloudflared: {e}")),
    }
}
