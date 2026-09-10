use super::TunnelResult;
use std::process::Stdio;
use tokio::process::Command;

/// The node's own tailnet IPv4, used to bind the server so only tailnet peers can reach it.
pub async fn bind_address(port: u16) -> Option<String> {
    let output = Command::new("tailscale")
        .args(["ip", "-4"])
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || ip.is_empty() {
        return None;
    }
    Some(format!("{ip}:{port}"))
}

/// Direct tailnet access: no relay, plain HTTP inside WireGuard, MagicDNS name when available.
pub async fn start(local_port: u16) -> TunnelResult {
    let output = Command::new("tailscale")
        .args(["status", "--json"])
        .stderr(Stdio::null())
        .output()
        .await;
    let output = match output {
        Ok(o) if o.status.success() => o,
        Ok(_) => return TunnelResult::Unavailable("tailscale is not connected".into()),
        Err(e) => return TunnelResult::Unavailable(format!("tailscale: {e}")),
    };
    let status: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(v) => v,
        Err(e) => return TunnelResult::Unavailable(format!("tailscale status: {e}")),
    };
    let dns_name = status["Self"]["DNSName"]
        .as_str()
        .map(|n| n.trim_end_matches('.').to_string())
        .filter(|n| !n.is_empty());
    let ip = status["Self"]["TailscaleIPs"]
        .as_array()
        .and_then(|ips| ips.iter().find_map(|v| v.as_str().filter(|s| s.contains('.'))))
        .map(str::to_string);
    match dns_name.or(ip) {
        Some(host) => TunnelResult::Url(format!("http://{host}:{local_port}")),
        None => TunnelResult::Unavailable("tailscale: no tailnet address for this node".into()),
    }
}

/// Public exposure through Tailscale Funnel; requires Funnel enabled on the tailnet.
pub async fn start_funnel(local_port: u16) -> TunnelResult {
    let child = Command::new("tailscale")
        .args(["funnel", &local_port.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let child = match child {
        Ok(c) => c,
        Err(e) => return TunnelResult::Unavailable(format!("tailscale: {e}")),
    };

    super::parse_url_from_output(child, |url| url.contains(".ts.net"), 15, "tailscale funnel").await
}
