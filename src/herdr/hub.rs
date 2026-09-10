use super::api::Client;
use super::controller::{Controller, ControllerEvent};
use super::snapshot::Snapshot;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify, broadcast};

const EVENT_DEBOUNCE: Duration = Duration::from_millis(120);
const SNAPSHOT_POLL: Duration = Duration::from_secs(3);
const MAX_CONNECT_FAILURES: u32 = 5;
const MAX_SCROLL_LINES: u32 = 500;

const STRUCTURAL_EVENTS: &[&str] = &[
    "workspace.created",
    "workspace.updated",
    "workspace.renamed",
    "workspace.moved",
    "workspace.reordered",
    "workspace.closed",
    "workspace.focused",
    "tab.created",
    "tab.closed",
    "tab.focused",
    "tab.renamed",
    "tab.moved",
    "pane.created",
    "pane.closed",
    "pane.updated",
    "pane.focused",
    "pane.moved",
    "pane.exited",
    "pane.agent_detected",
];

const PANE_SET_EVENTS: &[&str] = &["pane_created", "pane_closed", "pane_moved", "pane_exited"];

#[derive(Clone)]
pub enum HubEvent {
    Snapshot(Snapshot),
    Attached { pane_id: String },
    Detached { pane_id: String, reason: String },
    Frame(Vec<u8>),
    Closed,
}

struct HubState {
    snapshot: Snapshot,
    controller: Option<Controller>,
    cols: u16,
    rows: u16,
}

pub struct Hub {
    api: Client,
    herdr_bin: PathBuf,
    events: broadcast::Sender<HubEvent>,
    state: Mutex<HubState>,
    closed: Notify,
}

impl Hub {
    pub async fn connect(api: Client, herdr_bin: PathBuf) -> anyhow::Result<Arc<Self>> {
        let snapshot = fetch_snapshot(&api).await?;
        let (events, _) = broadcast::channel(1024);
        let hub = Arc::new(Self {
            api,
            herdr_bin,
            events,
            state: Mutex::new(HubState {
                snapshot,
                controller: None,
                cols: 80,
                rows: 24,
            }),
            closed: Notify::new(),
        });
        tokio::spawn(hub.clone().watch());
        Ok(hub)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HubEvent> {
        self.events.subscribe()
    }

    pub async fn closed(&self) {
        self.closed.notified().await
    }

    pub async fn snapshot(&self) -> Snapshot {
        self.state.lock().await.snapshot.clone()
    }

    pub async fn attached_pane(&self) -> Option<String> {
        self.state
            .lock()
            .await
            .controller
            .as_ref()
            .map(|c| c.pane_id.clone())
    }

    /// Returns the bytes needed to repaint the attached pane on a fresh terminal.
    pub async fn screen(&self) -> Option<Vec<u8>> {
        let mut state = self.state.lock().await;
        let controller = state.controller.as_mut()?;
        if controller.screen_is_stale().await {
            let _ = controller.repaint().await;
            return Some(Vec::new());
        }
        Some(controller.screen().await)
    }

    pub async fn attach(self: &Arc<Self>, pane_id: &str) -> anyhow::Result<()> {
        let mut state = self.state.lock().await;
        if !state.snapshot.has_pane(pane_id) {
            anyhow::bail!("unknown pane {pane_id}");
        }
        if let Some(previous) = state.controller.take() {
            previous.release().await;
        }
        let hub = self.clone();
        let event_pane = pane_id.to_string();
        let controller = Controller::spawn(
            &self.herdr_bin,
            self.api.socket_path(),
            pane_id,
            state.cols,
            state.rows,
            move |event| match event {
                ControllerEvent::Frame(bytes) => {
                    let _ = hub.events.send(HubEvent::Frame(bytes));
                }
                ControllerEvent::Closed { reason } => {
                    let _ = hub.events.send(HubEvent::Detached {
                        pane_id: event_pane.clone(),
                        reason,
                    });
                    let hub = hub.clone();
                    let pane_id = event_pane.clone();
                    tokio::spawn(async move { hub.forget_closed(&pane_id).await });
                }
            },
        )
        .await?;
        let _ = self.events.send(HubEvent::Attached {
            pane_id: pane_id.to_string(),
        });
        state.controller = Some(controller);
        Ok(())
    }

    pub async fn detach(&self) {
        let mut state = self.state.lock().await;
        if let Some(controller) = state.controller.take() {
            let pane_id = controller.pane_id.clone();
            controller.release().await;
            let _ = self.events.send(HubEvent::Detached {
                pane_id,
                reason: "detached".into(),
            });
        }
    }

    pub async fn input(&self, data: &str) -> anyhow::Result<()> {
        let mut state = self.state.lock().await;
        match state.controller.as_mut() {
            Some(controller) => controller.input(data).await,
            None => Ok(()),
        }
    }

    pub async fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        if cols == 0 || rows == 0 {
            return Ok(());
        }
        let mut state = self.state.lock().await;
        state.cols = cols;
        state.rows = rows;
        match state.controller.as_mut() {
            Some(controller) => controller.resize(cols, rows).await,
            None => Ok(()),
        }
    }

    pub async fn scroll(&self, direction: &str, lines: u32) -> anyhow::Result<()> {
        if !matches!(direction, "up" | "down") || lines == 0 {
            anyhow::bail!("invalid scroll");
        }
        let mut state = self.state.lock().await;
        match state.controller.as_mut() {
            Some(controller) => controller.scroll(direction, lines.min(MAX_SCROLL_LINES)).await,
            None => Ok(()),
        }
    }

    pub async fn focus_on_desktop(&self, pane_id: &str) -> anyhow::Result<()> {
        self.api
            .request("pane.focus", json!({ "pane_id": pane_id }))
            .await?;
        Ok(())
    }

    async fn forget_closed(&self, pane_id: &str) {
        let mut state = self.state.lock().await;
        if state
            .controller
            .as_ref()
            .is_some_and(|c| c.pane_id == pane_id)
            && let Some(controller) = state.controller.take()
        {
            controller.release().await;
        }
    }

    async fn refresh(&self) -> anyhow::Result<()> {
        let snapshot = fetch_snapshot(&self.api).await?;
        let mut state = self.state.lock().await;
        state.snapshot = snapshot.clone();
        drop(state);
        let _ = self.events.send(HubEvent::Snapshot(snapshot));
        Ok(())
    }

    async fn watch(self: Arc<Self>) {
        let mut failures = 0u32;
        loop {
            let pane_ids = self.state.lock().await.snapshot.pane_ids();
            let mut rx = match self.api.subscribe(build_subscriptions(&pane_ids)).await {
                Ok(rx) => {
                    failures = 0;
                    rx
                }
                Err(e) => {
                    failures += 1;
                    tracing::warn!("herdr event subscription failed ({failures}): {e}");
                    if failures >= MAX_CONNECT_FAILURES {
                        eprintln!("  herdr server is gone; shutting down.");
                        let _ = self.events.send(HubEvent::Closed);
                        self.closed.notify_waiters();
                        return;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };

            let _ = self.refresh().await;
            let mut dirty_since: Option<Instant> = None;
            let mut resubscribe = false;
            loop {
                let debounce = async {
                    match dirty_since {
                        Some(t) => tokio::time::sleep_until((t + EVENT_DEBOUNCE).into()).await,
                        None => std::future::pending::<()>().await,
                    }
                };
                tokio::select! {
                    event = rx.recv() => match event {
                        Some(event) => {
                            dirty_since.get_or_insert_with(Instant::now);
                            let kind = event["event"].as_str().unwrap_or("");
                            if PANE_SET_EVENTS.contains(&kind) {
                                resubscribe = true;
                            }
                        }
                        None => break,
                    },
                    _ = debounce => {
                        dirty_since = None;
                        if self.refresh().await.is_err() {
                            break;
                        }
                        if resubscribe {
                            break;
                        }
                    }
                    _ = tokio::time::sleep(SNAPSHOT_POLL) => {
                        if self.refresh().await.is_err() {
                            break;
                        }
                    }
                }
            }
            if !resubscribe {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

fn build_subscriptions(pane_ids: &[String]) -> Vec<Value> {
    let mut subs: Vec<Value> = STRUCTURAL_EVENTS
        .iter()
        .map(|t| json!({ "type": t }))
        .collect();
    for pane_id in pane_ids {
        subs.push(json!({ "type": "pane.agent_status_changed", "pane_id": pane_id }));
    }
    subs
}

async fn fetch_snapshot(api: &Client) -> anyhow::Result<Snapshot> {
    let mut result = api.request("session.snapshot", json!({})).await?;
    Ok(serde_json::from_value(result["snapshot"].take())?)
}
