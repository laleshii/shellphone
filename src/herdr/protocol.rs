use super::snapshot::Snapshot;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
    Attach { pane_id: String },
    Detach,
    Scroll { direction: String, lines: u32 },
    Focus { pane_id: String },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Snapshot {
        #[serde(flatten)]
        snapshot: Snapshot,
        attached_pane_id: Option<String>,
    },
    Attached {
        pane_id: String,
    },
    Detached {
        pane_id: String,
        reason: String,
    },
    Error {
        message: String,
    },
    Exit {
        code: Option<u32>,
    },
}
