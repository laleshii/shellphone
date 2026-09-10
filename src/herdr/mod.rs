mod api;
mod controller;
mod hub;
mod protocol;
mod snapshot;
pub mod ws;

pub use hub::Hub;

use anyhow::Context;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Options {
    pub session: Option<String>,
    pub socket: Option<PathBuf>,
    pub herdr_bin: Option<PathBuf>,
    pub pane: Option<String>,
}

pub async fn connect(opts: Options) -> anyhow::Result<Arc<Hub>> {
    let herdr_bin = match opts.herdr_bin {
        Some(path) => path,
        None => which::which("herdr").context("'herdr' is not installed or not in PATH")?,
    };
    let socket = resolve_socket(opts.session.as_deref(), opts.socket)?;
    if !socket.exists() {
        anyhow::bail!(
            "no herdr server socket at {}. Start herdr first (`herdr` or `herdr --session <name>`).",
            socket.display()
        );
    }

    let api = api::Client::new(socket);
    let pong = api
        .request("ping", serde_json::json!({}))
        .await
        .context("herdr server did not answer; is it running?")?;
    tracing::info!(
        "Connected to herdr {} (protocol {})",
        pong["version"].as_str().unwrap_or("?"),
        pong["protocol"]
    );

    let hub = Hub::connect(api, herdr_bin).await?;
    if let Some(pane_id) = opts.pane {
        hub.attach(&pane_id).await?;
    }
    Ok(hub)
}

fn resolve_socket(session: Option<&str>, explicit: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    if let Some(name) = session {
        return Ok(herdr_config_dir()?
            .join("sessions")
            .join(name)
            .join("herdr.sock"));
    }
    if let Some(path) = std::env::var_os("HERDR_SOCKET_PATH") {
        return Ok(PathBuf::from(path));
    }
    Ok(herdr_config_dir()?.join("herdr.sock"))
}

fn herdr_config_dir() -> anyhow::Result<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg).join("herdr"));
    }
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config").join("herdr"))
}
