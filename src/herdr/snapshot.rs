use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub focused_workspace_id: Option<String>,
    pub focused_tab_id: Option<String>,
    pub focused_pane_id: Option<String>,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
    #[serde(default)]
    pub tabs: Vec<Tab>,
    #[serde(default)]
    pub panes: Vec<Pane>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Workspace {
    pub workspace_id: String,
    pub label: String,
    pub number: u64,
    pub agent_status: String,
    pub active_tab_id: String,
    pub focused: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tab {
    pub tab_id: String,
    pub workspace_id: String,
    pub label: String,
    pub number: u64,
    pub agent_status: String,
    pub focused: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pane {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub agent: Option<String>,
    pub display_agent: Option<String>,
    pub agent_status: String,
    pub cwd: Option<String>,
    pub label: Option<String>,
    pub title: Option<String>,
    pub terminal_title_stripped: Option<String>,
    pub focused: bool,
}

impl Snapshot {
    pub fn pane_ids(&self) -> Vec<String> {
        self.panes.iter().map(|p| p.pane_id.clone()).collect()
    }

    pub fn has_pane(&self, pane_id: &str) -> bool {
        self.panes.iter().any(|p| p.pane_id == pane_id)
    }
}
