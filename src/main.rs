mod auth;
mod protocol;
mod pty_bridge;
mod server;
mod tls;
mod tunnel;

use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "shellphone", about = "Pipe CLI output to a secure mobile web interface")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Run a command and expose it via a secure web interface
    Run {
        /// The command to execute (passed to the shell)
        #[arg(trailing_var_arg = true, required = true)]
        cmd: Vec<String>,

        /// Tunnel provider: auto, cloudflared, ngrok, bore, tailscale, custom, none
        #[arg(long, default_value = "auto")]
        tunnel: String,

        /// Custom tunnel command (use {port} as placeholder)
        #[arg(long)]
        tunnel_cmd: Option<String>,

        /// Bore server address (default: bore.pub)
        #[arg(long, default_value = "bore.pub")]
        bore_server: String,

        /// Enable built-in TLS with self-signed certificate
        #[arg(long)]
        tls: bool,

        /// Bind address (default: 127.0.0.1:3845, or 0.0.0.0:3845 with --tls)
        #[arg(long)]
        bind: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            cmd,
            tunnel: tunnel_name,
            tunnel_cmd,
            bore_server,
            tls: use_tls,
            bind,
        } => {
            let command_str = cmd.join(" ");
            let token = auth::generate_token();
            let (cmd_tx, event_rx, exit_rx) = pty_bridge::spawn(&command_str)?;

            let default_bind = if use_tls {
                "0.0.0.0:3845"
            } else {
                "127.0.0.1:3845"
            };
            let bind_addr = bind.unwrap_or_else(|| default_bind.to_string());

            let tls_cert = if use_tls {
                let cert = tls::generate()?;
                eprintln!("  TLS certificate fingerprint:");
                eprintln!("  {}", cert.fingerprint);
                eprintln!();
                Some(cert)
            } else {
                None
            };

            let server_config = server::ServerConfig {
                token: token.clone(),
                cmd_tx,
                event_rx,
                bind: bind_addr.clone(),
                tls: tls_cert,
            };

            let (addr, connected_notify) = server::start(server_config).await?;
            let port = addr.port();
            let scheme = if use_tls { "https" } else { "http" };
            tracing::info!("Server listening on {scheme}://{addr}");

            let provider = parse_tunnel_provider(&tunnel_name, tunnel_cmd, bore_server);

            let base_url = match provider {
                Some(provider) => match provider.start(port).await {
                    tunnel::TunnelResult::Url(url) => {
                        tracing::info!("Tunnel active: {url}");
                        url
                    }
                    tunnel::TunnelResult::Unavailable(reason) => {
                        eprintln!("  ({reason})");
                        eprintln!();
                        format!("{scheme}://{addr}")
                    }
                },
                None => format!("{scheme}://{addr}"),
            };

            let qr_lines = print_launch_info(&base_url, &token);

            let connected_notify2 = connected_notify.clone();
            tokio::spawn(async move {
                connected_notify2.notified().await;
                clear_lines(qr_lines);
                eprintln!("  Client connected. Waiting for process to exit...");
                eprintln!();
            });

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Interrupted");
                }
                exit_code = exit_rx => {
                    let code = exit_code.ok().flatten();
                    tracing::info!("Process exited with code {}", code.unwrap_or(0));
                    // Brief delay so the WebSocket exit message reaches the client
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        }
    }

    Ok(())
}

fn parse_tunnel_provider(
    name: &str,
    custom_cmd: Option<String>,
    bore_server: String,
) -> Option<tunnel::Provider> {
    match name {
        "none" => None,
        "auto" => Some(tunnel::Provider::Auto),
        "cloudflared" => Some(tunnel::Provider::Cloudflared),
        "ngrok" => Some(tunnel::Provider::Ngrok),
        "bore" => Some(tunnel::Provider::Bore {
            server: bore_server,
        }),
        "tailscale" => Some(tunnel::Provider::Tailscale),
        "custom" => {
            let cmd = custom_cmd.expect("--tunnel-cmd is required when --tunnel=custom");
            Some(tunnel::Provider::Custom { cmd })
        }
        other => {
            eprintln!("Unknown tunnel provider: {other}. Using auto-detect.");
            Some(tunnel::Provider::Auto)
        }
    }
}

fn print_launch_info(base_url: &str, token: &str) -> usize {
    let url = format!("{base_url}?token={token}");

    let qr = qrcode::QrCode::new(&url).expect("failed to generate QR code");
    let qr_string = render_qr_compact(&qr);

    let mut lines = 0;

    eprintln!();
    lines += 1;
    eprintln!("  Scan to connect:");
    lines += 1;
    eprintln!();
    lines += 1;
    for line in qr_string.lines() {
        eprintln!("  {line}");
        lines += 1;
    }
    eprintln!();
    lines += 1;
    eprintln!("  {url}");
    lines += 1;
    eprintln!();
    lines += 1;

    lines
}

fn clear_lines(n: usize) {
    // Move cursor up n lines, clearing each one
    for _ in 0..n {
        eprint!("\x1b[A\x1b[2K");
    }
}

fn render_qr_compact(qr: &qrcode::QrCode) -> String {
    let width = qr.width();
    let margin = 1;
    let total_w = width + margin * 2;
    let total_h = width + margin * 2;
    let mut out = String::new();

    let is_dark = |x: i32, y: i32| -> bool {
        if x < 0 || y < 0 || x >= width as i32 || y >= width as i32 {
            return false;
        }
        qr[(x as usize, y as usize)] == qrcode::types::Color::Dark
    };

    // Each output row encodes 2 QR rows using half-block characters
    let mut y: usize = 0;
    while y < total_h {
        for x in 0..total_w {
            let qx = x as i32 - margin as i32;
            let qy_top = y as i32 - margin as i32;
            let qy_bot = qy_top + 1;
            let top = is_dark(qx, qy_top);
            let bot = is_dark(qx, qy_bot);
            out.push(match (top, bot) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
        }
        out.push('\n');
        y += 2;
    }
    out
}
