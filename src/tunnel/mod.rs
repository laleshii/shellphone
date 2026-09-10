mod bore;
mod cloudflared;
mod custom;
mod ngrok;
pub mod tailscale;

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub enum TunnelResult {
    Url(String),
    Unavailable(String),
}

#[derive(Clone, Debug)]
pub enum Provider {
    Cloudflared,
    Ngrok,
    Bore { server: String },
    Tailscale,
    TailscaleFunnel,
    Custom { cmd: String },
    Auto,
}

impl Provider {
    pub async fn start(&self, local_port: u16) -> TunnelResult {
        match self {
            Provider::Cloudflared => cloudflared::start(local_port).await,
            Provider::Ngrok => ngrok::start(local_port).await,
            Provider::Bore { server } => bore::start(local_port, server).await,
            Provider::Tailscale => tailscale::start(local_port).await,
            Provider::TailscaleFunnel => tailscale::start_funnel(local_port).await,
            Provider::Custom { cmd } => custom::start(local_port, cmd).await,
            Provider::Auto => auto_detect(local_port).await,
        }
    }
}

async fn auto_detect(local_port: u16) -> TunnelResult {
    let candidates: &[(&str, fn() -> Provider)] = &[
        ("cloudflared", || Provider::Cloudflared),
        ("ngrok", || Provider::Ngrok),
        ("bore", || Provider::Bore { server: "bore.pub".into() }),
        ("tailscale", || Provider::TailscaleFunnel),
    ];
    for &(name, make_provider) in candidates {
        if which(name).await {
            tracing::info!("Auto-detected tunnel provider: {name}");
            return match make_provider() {
                Provider::Cloudflared => cloudflared::start(local_port).await,
                Provider::Ngrok => ngrok::start(local_port).await,
                Provider::Bore { server } => bore::start(local_port, &server).await,
                Provider::TailscaleFunnel => tailscale::start_funnel(local_port).await,
                _ => unreachable!(),
            };
        }
    }
    TunnelResult::Unavailable("No tunnel provider found. Install cloudflared, ngrok, bore, or tailscale, or use --tunnel-cmd.".into())
}

async fn which(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok_and(|s| s.success())
}

async fn parse_url_from_output(
    mut child: tokio::process::Child,
    url_pattern: fn(&str) -> bool,
    timeout_secs: u64,
    provider_name: &str,
) -> TunnelResult {
    let stderr = child.stderr.take();
    let stdout = child.stdout.take();

    let (url_tx, url_rx) = tokio::sync::oneshot::channel::<String>();
    let url_tx = std::sync::Arc::new(std::sync::Mutex::new(Some(url_tx)));

    fn scan_stream(
        reader: impl tokio::io::AsyncRead + Unpin + Send + 'static,
        url_pattern: fn(&str) -> bool,
        url_tx: std::sync::Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
        label: String,
    ) {
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader);
            let mut buf = String::new();
            loop {
                buf.clear();
                match lines.read_line(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let line = buf.trim();
                        tracing::debug!("[{label}] {line}");
                        if let Some(url) = extract_url(line, url_pattern) {
                            if let Some(tx) = url_tx.lock().unwrap().take() {
                                let _ = tx.send(url);
                            }
                        }
                    }
                }
            }
        });
    }

    let label = provider_name.to_string();
    if let Some(stderr) = stderr {
        scan_stream(stderr, url_pattern, url_tx.clone(), format!("{label}/err"));
    }
    if let Some(stdout) = stdout {
        scan_stream(stdout, url_pattern, url_tx.clone(), format!("{label}/out"));
    }

    tokio::spawn(async move {
        let _ = child.wait().await;
    });

    match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), url_rx).await {
        Ok(Ok(url)) => TunnelResult::Url(url),
        _ => TunnelResult::Unavailable(format!(
            "Timed out waiting for {provider_name} tunnel URL"
        )),
    }
}

fn extract_url(line: &str, pattern: fn(&str) -> bool) -> Option<String> {
    let start = line.find("https://")?;
    let url_part = &line[start..];
    let url = url_part
        .split(|c: char| c.is_whitespace() || c == '|' || c == '"' || c == '\'')
        .next()?
        .trim_end_matches(|c: char| c == '/' || c == '.' || c == ',');

    if pattern(url) {
        Some(url.to_string())
    } else {
        None
    }
}
