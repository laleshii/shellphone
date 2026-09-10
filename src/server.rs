use crate::auth::{AuthResult, SessionGuard};
use crate::herdr;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::pty_bridge::{CommandTx, EventRx, PtyCommand, PtyEvent};
use crate::tls::SelfSignedCert;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use rust_embed::Embed;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, Notify};

#[derive(Embed)]
#[folder = "frontend/"]
struct FrontendAssets;

pub enum BackendConfig {
    Pty { cmd_tx: CommandTx, event_rx: EventRx },
    Herdr(Arc<herdr::Hub>),
}

#[derive(Clone)]
enum Backend {
    Pty {
        cmd_tx: CommandTx,
        event_tx: broadcast::Sender<PtyEvent>,
    },
    Herdr(Arc<herdr::Hub>),
}

impl Backend {
    fn index_file(&self) -> &'static str {
        match self {
            Backend::Pty { .. } => "index.html",
            Backend::Herdr(_) => "herdr.html",
        }
    }
}

#[derive(Clone)]
struct AppState {
    guard: SessionGuard,
    backend: Backend,
    connected_notify: Arc<Notify>,
}

pub struct ServerConfig {
    pub token: String,
    pub backend: BackendConfig,
    pub bind: String,
    pub tls: Option<SelfSignedCert>,
}

pub async fn start(config: ServerConfig) -> anyhow::Result<(SocketAddr, Arc<Notify>)> {
    let connected_notify = Arc::new(Notify::new());
    let backend = match config.backend {
        BackendConfig::Pty { cmd_tx, event_rx } => Backend::Pty {
            cmd_tx,
            event_tx: relay_events(event_rx),
        },
        BackendConfig::Herdr(hub) => Backend::Herdr(hub),
    };
    let state = AppState {
        guard: SessionGuard::new(config.token),
        backend,
        connected_notify: connected_notify.clone(),
    };

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .route("/assets/{*path}", get(asset_handler))
        .with_state(state);

    let preferred: SocketAddr = config.bind.parse()?;
    let listener = match tokio::net::TcpListener::bind(preferred).await {
        Ok(l) => l,
        Err(_) => {
            let fallback: SocketAddr = format!("{}:0", preferred.ip()).parse()?;
            tokio::net::TcpListener::bind(fallback).await?
        }
    };
    let addr = listener.local_addr()?;

    if let Some(tls) = config.tls {
        let rustls_config = axum_server::tls_rustls::RustlsConfig::from_pem(
            tls.cert_pem.into_bytes(),
            tls.key_pem.into_bytes(),
        )
        .await?;

        let tls_listener = listener.into_std()?;
        tokio::spawn(async move {
            axum_server::from_tcp_rustls(tls_listener, rustls_config)
                .serve(app.into_make_service())
                .await
                .ok();
        });
    } else {
        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
    }

    Ok((addr, connected_notify))
}

fn relay_events(rx: EventRx) -> broadcast::Sender<PtyEvent> {
    let (tx, _) = broadcast::channel(256);
    let tx2 = tx.clone();
    let mut rx = rx;
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let _ = tx2.send(event);
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    tx
}

async fn index_handler(State(state): State<AppState>) -> impl IntoResponse {
    match FrontendAssets::get(state.backend.index_file()) {
        Some(content) => Html(String::from_utf8_lossy(&content.data).to_string()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn asset_handler(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl IntoResponse {
    match FrontendAssets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path)
                .first_or_octet_stream()
                .to_string();
            (
                [(axum::http::header::CONTENT_TYPE, mime)],
                content.data.to_vec(),
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<AppState>,
) -> Response {
    let token = params.get("token").cloned();
    let refresh = params.get("refresh").cloned();
    ws.on_upgrade(move |socket| handle_ws(socket, token, refresh, state))
}

async fn handle_ws(
    socket: WebSocket,
    token: Option<String>,
    refresh: Option<String>,
    state: AppState,
) {
    let auth_result = state
        .guard
        .authenticate(token.as_deref(), refresh.as_deref())
        .await;

    let (mut ws_tx, ws_rx) = socket.split();

    match auth_result {
        AuthResult::NewSession { refresh_token } => {
            tracing::info!("New client authenticated");
            state.connected_notify.notify_one();
            let msg = ServerMessage::RefreshToken {
                token: refresh_token,
            };
            let json = serde_json::to_string(&msg).unwrap();
            if ws_tx.send(Message::Text(json.into())).await.is_err() {
                return;
            }
        }
        AuthResult::Reconnected => {
            tracing::info!("Client reconnected via refresh token");
        }
        AuthResult::Failed => {
            tracing::warn!("WebSocket auth failed");
            return;
        }
    }

    match state.backend {
        Backend::Pty { cmd_tx, event_tx } => bridge_pty(ws_tx, ws_rx, cmd_tx, event_tx).await,
        Backend::Herdr(hub) => herdr::ws::serve(ws_tx, ws_rx, hub).await,
    }

    tracing::info!("Client disconnected");
}

async fn bridge_pty(
    mut ws_tx: SplitSink<WebSocket, Message>,
    mut ws_rx: SplitStream<WebSocket>,
    cmd_tx: CommandTx,
    event_tx: broadcast::Sender<PtyEvent>,
) {
    let mut event_rx = event_tx.subscribe();

    let send_task = tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(PtyEvent::Output(data)) => {
                    if ws_tx.send(Message::Binary(data.into())).await.is_err() {
                        break;
                    }
                }
                Ok(PtyEvent::Exit(code)) => {
                    let msg = ServerMessage::Exit { code };
                    let json = serde_json::to_string(&msg).unwrap();
                    let _ = ws_tx.send(Message::Text(json.into())).await;
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Text(text) => {
                    if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                        let cmd = match client_msg {
                            ClientMessage::Input { data } => PtyCommand::Input(data.into_bytes()),
                            ClientMessage::Resize { cols, rows } => {
                                PtyCommand::Resize { cols, rows }
                            }
                        };
                        if cmd_tx.send(cmd).await.is_err() {
                            break;
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
}
