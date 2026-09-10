use super::hub::{Hub, HubEvent};
use super::protocol::{ClientMessage, ServerMessage};
use axum::extract::ws::{Message, WebSocket};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

const CLEAR_SCREEN: &[u8] = b"\x1b[3J\x1b[2J\x1b[H";

fn text(msg: &ServerMessage) -> Message {
    Message::Text(serde_json::to_string(msg).unwrap().into())
}

pub async fn serve(
    mut ws_tx: SplitSink<WebSocket, Message>,
    mut ws_rx: SplitStream<WebSocket>,
    hub: Arc<Hub>,
) {
    let mut events = hub.subscribe();
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(256);

    let snapshot = hub.snapshot().await;
    let attached_pane_id = hub.attached_pane().await;
    let _ = out_tx
        .send(text(&ServerMessage::Snapshot {
            snapshot,
            attached_pane_id: attached_pane_id.clone(),
        }))
        .await;
    if let Some(pane_id) = attached_pane_id {
        let _ = out_tx.send(text(&ServerMessage::Attached { pane_id })).await;
        if let Some(screen) = hub.screen().await {
            let mut repaint = CLEAR_SCREEN.to_vec();
            repaint.extend_from_slice(&screen);
            let _ = out_tx.send(Message::Binary(repaint.into())).await;
        }
    }

    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if ws_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    let hub_events = hub.clone();
    let events_tx = out_tx.clone();
    let event_task = tokio::spawn(async move {
        loop {
            let msg = match events.recv().await {
                Ok(HubEvent::Frame(bytes)) => Message::Binary(bytes.into()),
                Ok(HubEvent::Snapshot(snapshot)) => text(&ServerMessage::Snapshot {
                    snapshot,
                    attached_pane_id: hub_events.attached_pane().await,
                }),
                Ok(HubEvent::Attached { pane_id }) => {
                    if events_tx
                        .send(Message::Binary(CLEAR_SCREEN.to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                    text(&ServerMessage::Attached { pane_id })
                }
                Ok(HubEvent::Detached { pane_id, reason }) => {
                    text(&ServerMessage::Detached { pane_id, reason })
                }
                Ok(HubEvent::Closed) => {
                    let _ = events_tx.send(text(&ServerMessage::Exit { code: None })).await;
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let Some(screen) = hub_events.screen().await else { continue };
                    let mut repaint = CLEAR_SCREEN.to_vec();
                    repaint.extend_from_slice(&screen);
                    Message::Binary(repaint.into())
                }
                Err(broadcast::error::RecvError::Closed) => break,
            };
            if events_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Text(raw) => {
                    let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&raw) else {
                        continue;
                    };
                    let result = match client_msg {
                        ClientMessage::Input { data } => hub.input(&data).await,
                        ClientMessage::Resize { cols, rows } => hub.resize(cols, rows).await,
                        ClientMessage::Attach { pane_id } => hub.attach(&pane_id).await,
                        ClientMessage::Detach => {
                            hub.detach().await;
                            Ok(())
                        }
                        ClientMessage::Scroll { direction, lines } => {
                            hub.scroll(&direction, lines).await
                        }
                        ClientMessage::Focus { pane_id } => hub.focus_on_desktop(&pane_id).await,
                    };
                    if let Err(e) = result {
                        let _ = out_tx
                            .send(text(&ServerMessage::Error {
                                message: e.to_string(),
                            }))
                            .await;
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = event_task => {},
        _ = recv_task => {},
    }
    writer.abort();
}
