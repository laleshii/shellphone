use crate::protocol::{ClientMessage, ServerMessage};
use futures_util::{SinkExt, StreamExt};
use std::io::{Read, Write};
use tokio_tungstenite::tungstenite::Message;

pub async fn run(url_str: &str, insecure: bool) -> anyhow::Result<()> {
    let parsed = url::Url::parse(url_str)?;
    let token = parsed
        .query_pairs()
        .find(|(k, _)| k == "token")
        .map(|(_, v)| v.to_string())
        .ok_or_else(|| anyhow::anyhow!("URL must contain a ?token= parameter"))?;

    let ws_scheme = match parsed.scheme() {
        "https" => "wss",
        _ => "ws",
    };
    let host = parsed.host_str().unwrap_or("127.0.0.1");
    let port = parsed.port().unwrap_or(if ws_scheme == "wss" { 443 } else { 80 });
    let ws_base = format!("{ws_scheme}://{host}:{port}/ws");

    let connector = if insecure {
        let tls = tokio_rustls::rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(std::sync::Arc::new(InsecureVerifier))
            .with_no_client_auth();
        Some(tokio_tungstenite::Connector::Rustls(std::sync::Arc::new(tls)))
    } else {
        None
    };

    crossterm::terminal::enable_raw_mode()?;
    let _raw_guard = RawModeGuard;

    let (stdin_tx, mut stdin_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        let stdin = std::io::stdin();
        let mut handle = stdin.lock();
        loop {
            match handle.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if stdin_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut sigwinch = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())?;
    let mut refresh_token: Option<String> = None;
    let mut first_connect = true;

    loop {
        let ws_url = if first_connect {
            format!("{}?token={}", ws_base, urlencoding::encode(&token))
        } else if let Some(ref rt) = refresh_token {
            format!("{}?refresh={}", ws_base, urlencoding::encode(rt))
        } else {
            eprintln!("\r\nNo refresh token — cannot reconnect.");
            break;
        };

        let connect_result = tokio_tungstenite::connect_async_tls_with_config(
            &ws_url,
            None,
            false,
            connector.clone(),
        )
        .await;

        let ws = match connect_result {
            Ok((ws, _)) => ws,
            Err(e) => {
                if first_connect {
                    return Err(e.into());
                }
                eprintln!("\r\nReconnect failed: {e}. Retrying in 2s...");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        if first_connect {
            eprintln!("Connected. Press Ctrl+C to detach.\r");
        } else {
            eprintln!("Reconnected.\r");
        }
        first_connect = false;

        let (mut ws_tx, mut ws_rx) = ws.split();

        if let Ok((cols, rows)) = crossterm::terminal::size() {
            let msg = serde_json::to_string(&ClientMessage::Resize { cols, rows })?;
            let _ = ws_tx.send(Message::Text(msg.into())).await;
        }

        let mut session_ended = false;

        loop {
            tokio::select! {
                Some(data) = stdin_rx.recv() => {
                    let input = String::from_utf8_lossy(&data).to_string();
                    let msg = serde_json::to_string(&ClientMessage::Input { data: input })?;
                    if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                },
                msg = ws_rx.next() => {
                    match msg {
                        Some(Ok(Message::Binary(data))) => {
                            let mut stdout = std::io::stdout();
                            stdout.write_all(&data)?;
                            stdout.flush()?;
                        }
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(&text) {
                                match server_msg {
                                    ServerMessage::RefreshToken { token: rt } => {
                                        refresh_token = Some(rt);
                                    }
                                    ServerMessage::Exit { code } => {
                                        eprintln!("\r\n[process exited with code {}]\r", code.unwrap_or(0));
                                        session_ended = true;
                                        break;
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => break,
                        _ => {}
                    }
                },
                _ = sigwinch.recv() => {
                    if let Ok((cols, rows)) = crossterm::terminal::size() {
                        let msg = serde_json::to_string(&ClientMessage::Resize { cols, rows })?;
                        let _ = ws_tx.send(Message::Text(msg.into())).await;
                    }
                },
                _ = tokio::signal::ctrl_c() => {
                    eprintln!("\r\nDetached.\r");
                    return Ok(());
                },
            }
        }

        if session_ended {
            break;
        }

        eprintln!("Connection lost. Reconnecting in 2s...\r");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    Ok(())
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

#[derive(Debug)]
struct InsecureVerifier;

impl tokio_rustls::rustls::client::danger::ServerCertVerifier for InsecureVerifier {
    fn verify_server_cert(
        &self,
        _: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _: &[tokio_rustls::rustls::pki_types::CertificateDer<'_>],
        _: &tokio_rustls::rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: tokio_rustls::rustls::pki_types::UnixTime,
    ) -> Result<tokio_rustls::rustls::client::danger::ServerCertVerified, tokio_rustls::rustls::Error> {
        Ok(tokio_rustls::rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<tokio_rustls::rustls::client::danger::HandshakeSignatureValid, tokio_rustls::rustls::Error> {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<tokio_rustls::rustls::client::danger::HandshakeSignatureValid, tokio_rustls::rustls::Error> {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<tokio_rustls::rustls::SignatureScheme> {
        tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
