use super::TunnelResult;
use std::process::Stdio;
use tokio::process::Command;

pub async fn start(local_port: u16) -> TunnelResult {
    // tailscale funnel doesn't stay in foreground the same way — it configures
    // the funnel and prints the URL, then the serve command handles traffic.
    // We use `tailscale funnel <port>` which sets up both serve and funnel.
    let child = Command::new("tailscale")
        .args(["funnel", &local_port.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let child = match child {
        Ok(c) => c,
        Err(e) => return TunnelResult::Unavailable(format!("tailscale: {e}")),
    };

    // tailscale funnel prints the URL to stdout, e.g.:
    // https://myhost.ts.net/
    // Available on the internet:
    // https://myhost.ts.net:443/
    // We also try `tailscale status` to get the funnel URL if the above doesn't work.
    super::parse_url_from_output(child, |url| url.contains(".ts.net"), 15, "tailscale").await
}
