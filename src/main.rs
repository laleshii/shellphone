mod attach;
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

#[derive(clap::Args, Clone)]
struct NetworkOpts {
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
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Run a command and expose it via a secure web interface
    Run {
        /// The command to execute (passed to the shell)
        #[arg(trailing_var_arg = true, required = true)]
        cmd: Vec<String>,

        #[command(flatten)]
        net: NetworkOpts,
    },

    /// Attach to a running shellphone session from another terminal
    Attach {
        /// The shellphone URL (e.g. https://host:3845?token=...)
        url: String,

        /// Accept self-signed TLS certificates
        #[arg(long, short = 'k')]
        insecure: bool,
    },

    /// Resume an AI coding agent session
    Agent {
        /// Agent name (claude, codex). Omit for interactive selection.
        agent: Option<String>,

        /// Session ID to resume. Omit for the agent's interactive picker.
        session: Option<String>,

        #[command(flatten)]
        net: NetworkOpts,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run { cmd, net } => run_command(&cmd.join(" "), net).await,
        Commands::Attach { url, insecure } => attach::run(&url, insecure).await,
        Commands::Agent { agent, session, net } => {
            let cmd = resolve_agent_command(agent, session)?;
            run_command(&cmd, net).await
        }
    }
}

struct AgentDef {
    name: &'static str,
    binary: &'static str,
    resume_cmd: fn(Option<&str>) -> String,
}

const AGENTS: &[AgentDef] = &[
    AgentDef {
        name: "claude",
        binary: "claude",
        resume_cmd: |session| match session {
            Some(id) => format!("claude --resume {id}"),
            None => "claude --resume".to_string(),
        },
    },
    AgentDef {
        name: "codex",
        binary: "codex",
        resume_cmd: |session| match session {
            Some(id) => format!("codex resume {id}"),
            None => "codex resume".to_string(),
        },
    },
    AgentDef {
        name: "opencode",
        binary: "opencode",
        resume_cmd: |session| match session {
            Some(id) => format!("opencode --session {id}"),
            None => "opencode --continue".to_string(),
        },
    },
];

fn resolve_agent_command(
    agent: Option<String>,
    session: Option<String>,
) -> anyhow::Result<String> {
    let installed: Vec<&AgentDef> = AGENTS
        .iter()
        .filter(|a| which::which(a.binary).is_ok())
        .collect();

    let agent_def = match agent {
        Some(name) => {
            let name_lower = name.to_lowercase();
            AGENTS
                .iter()
                .find(|a| a.name == name_lower)
                .ok_or_else(|| {
                    let known: Vec<_> = AGENTS.iter().map(|a| a.name).collect();
                    anyhow::anyhow!("Unknown agent '{name}'. Known agents: {}", known.join(", "))
                })?
        }
        None => {
            if installed.is_empty() {
                anyhow::bail!(
                    "No supported agents found. Install one of: {}",
                    AGENTS.iter().map(|a| a.name).collect::<Vec<_>>().join(", ")
                );
            }
            if installed.len() == 1 {
                installed[0]
            } else {
                eprintln!("  Available agents:");
                for (i, a) in installed.iter().enumerate() {
                    eprintln!("    {}. {}", i + 1, a.name);
                }
                eprint!("  Select agent [1]: ");
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                let choice: usize = input.trim().parse().unwrap_or(1);
                installed
                    .get(choice.saturating_sub(1))
                    .copied()
                    .ok_or_else(|| anyhow::anyhow!("Invalid selection"))?
            }
        }
    };

    if which::which(agent_def.binary).is_err() {
        anyhow::bail!("'{}' is not installed or not in PATH", agent_def.binary);
    }

    Ok((agent_def.resume_cmd)(session.as_deref()))
}

async fn run_command(command_str: &str, net: NetworkOpts) -> anyhow::Result<()> {
    let token = auth::generate_token();
    let (cmd_tx, event_rx, exit_rx) = pty_bridge::spawn(command_str)?;

    let default_bind = if net.tls {
        "0.0.0.0:3845"
    } else {
        "127.0.0.1:3845"
    };
    let bind_addr = net.bind.unwrap_or_else(|| default_bind.to_string());

    let tls_cert = if net.tls {
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
    let scheme = if net.tls { "https" } else { "http" };
    tracing::info!("Server listening on {scheme}://{addr}");

    let provider = parse_tunnel_provider(&net.tunnel, net.tunnel_cmd, net.bore_server);

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
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
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
