use super::{TunnelResult, parse_url_from_output};
use std::process::Stdio;
use tokio::process::Command;

pub async fn start(local_port: u16, cmd_template: &str) -> TunnelResult {
    let expanded = cmd_template.replace("{port}", &local_port.to_string());
    let parts: Vec<&str> = expanded.split_whitespace().collect();

    if parts.is_empty() {
        return TunnelResult::Unavailable("Empty tunnel command".into());
    }

    let child = Command::new(parts[0])
        .args(&parts[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    match child {
        Ok(child) => {
            parse_url_from_output(child, |_| true, 30, "custom").await
        }
        Err(e) => TunnelResult::Unavailable(format!("custom tunnel command: {e}")),
    }
}
