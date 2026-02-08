mod actions;
mod analysis;
mod insights;
mod keymap;
mod navigation;

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, TryRecvError};

use crate::domain::graph::{Provenance, ProvenanceSource};
use crate::domain::{ArchitectureGraph, NodeKind, QueryAnswer, Repository};
use crate::inference::{InferenceEngine, InferenceProgress};
use crate::infrastructure::{CoverageMap, GitSignals, PersistenceStore};
use chrono::Utc;

pub use analysis::{node_code_path, render_edit_history, require_graph};
pub use keymap::{map_key_to_action, KeyAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    Search,
    Query,
    EditSummary,
    EditReason,
    Help,
    History,
    ConfirmRegenerate,
    ConfirmExport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    Tree,
    Semantic,
    Right,
}

impl FocusPane {
    fn next(self) -> Self {
        match self {
            Self::Tree => Self::Semantic,
            Self::Semantic => Self::Right,
            Self::Right => Self::Tree,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Tree => Self::Right,
            Self::Semantic => Self::Tree,
            Self::Right => Self::Semantic,
        }
    }
}

pub struct AppState {
    pub repository: Repository,
    pub persistence: PersistenceStore,
    pub graph: ArchitectureGraph,
    pub visible: Vec<(String, usize)>,
    pub selected_index: usize,
    pub collapsed: HashSet<String>,
    pub mode: AppMode,
    pub input_buffer: String,
    pub edit_buffer: String,
    pub reason_buffer: String,
    pub last_query: Option<QueryAnswer>,
    pub query_history: Vec<QueryAnswer>,
    pub status_line: String,
    pub show_dependency_graph: bool,
    pub show_impact: bool,
    pub impact_depth: usize,
    pub focus: FocusPane,
    pub right_scroll: usize,
    pub should_quit: bool,
    pub pending_g: bool,
    pub coverage: CoverageMap,
    pub git_signals: GitSignals,
    pub inference: InferenceEngine,
    pub inference_events: Option<Receiver<InferenceProgress>>,
    pub inference_running: bool,
    pub author: String,
    pub query_history_cursor: Option<usize>,
}

impl AppState {
    pub fn new(
        repository: Repository,
        persistence: PersistenceStore,
        graph: ArchitectureGraph,
        inference: InferenceEngine,
        author: String,
    ) -> Self {
        let root_id = graph.root_id.clone();
        let mut collapsed = HashSet::new();
        for node in graph.nodes.values() {
            if node.id != root_id
                && !node.children.is_empty()
                && matches!(node.kind, NodeKind::Container | NodeKind::Component)
            {
                collapsed.insert(node.id.clone());
            }
        }

        let mut state = Self {
            repository,
            persistence,
            graph,
            visible: Vec::new(),
            selected_index: 0,
            collapsed,
            mode: AppMode::Normal,
            input_buffer: String::new(),
            edit_buffer: String::new(),
            reason_buffer: String::new(),
            last_query: None,
            query_history: Vec::new(),
            status_line: String::from("Ready"),
            show_dependency_graph: false,
            show_impact: false,
            impact_depth: 2,
            focus: FocusPane::Tree,
            right_scroll: 0,
            should_quit: false,
            pending_g: false,
            coverage: CoverageMap::default(),
            git_signals: GitSignals::default(),
            inference,
            inference_events: None,
            inference_running: false,
            author,
            query_history_cursor: None,
        };
        state.refresh_visible();
        state
    }

    pub fn selected_node_id(&self) -> Option<&str> {
        self.visible
            .get(self.selected_index)
            .map(|(id, _)| id.as_str())
    }

    pub fn selected_node(&self) -> Option<&crate::domain::ArchitectureNode> {
        self.selected_node_id().and_then(|id| self.graph.node(id))
    }

    pub fn selected_node_mut(&mut self) -> Option<&mut crate::domain::ArchitectureNode> {
        let id = self.selected_node_id()?.to_string();
        self.graph.node_mut(&id)
    }

    pub fn attach_inference_events(&mut self, rx: Receiver<InferenceProgress>) {
        self.inference_events = Some(rx);
        self.inference_running = true;
    }

    pub fn poll_inference_events(&mut self) {
        let Some(rx) = &self.inference_events else {
            return;
        };

        loop {
            match rx.try_recv() {
                Ok(event) => match event {
                    InferenceProgress::Started { total_nodes } => {
                        self.inference_running = true;
                        self.status_line = format!("Inference started: 0/{total_nodes}");
                    }
                    InferenceProgress::NodeDone {
                        processed,
                        total,
                        node_id,
                        node_name,
                        summary,
                        confidence,
                        source,
                    } => {
                        if let Some(node) = self.graph.node_mut(&node_id) {
                            if node.provenance.source != ProvenanceSource::Human {
                                node.summary = summary;
                                node.confidence = confidence;
                                node.provenance = Provenance::ai();
                                node.last_analyzed = Some(Utc::now());
                            }
                        }
                        self.status_line =
                            format!("Inference {processed}/{total}: {node_name} ({source})");
                    }
                    InferenceProgress::ProviderDisabled { reason } => {
                        self.status_line = format!(
                            "AI provider disabled during inference; local fallback active: {reason}"
                        );
                    }
                    InferenceProgress::Completed { stats } => {
                        self.inference_running = false;
                        self.status_line = format!(
                            "Inference complete: cache_hits={}, cache_misses={}, prompt_tokens={}, completion_tokens={}",
                            stats.cache_hits,
                            stats.cache_misses,
                            stats.prompt_tokens,
                            stats.completion_tokens
                        );
                        if let Err(err) = self.persistence.save_graph(&self.graph) {
                            self.status_line =
                                format!("Inference completed but save failed: {err}");
                        }
                    }
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.inference_running = false;
                    self.inference_events = None;
                    break;
                }
            }
        }
    }

    pub fn breadcrumb(&self) -> String {
        let Some(id) = self.selected_node_id() else {
            return String::new();
        };
        self.graph
            .lineage(id)
            .iter()
            .filter_map(|node_id| self.graph.node(node_id))
            .map(|node| node.name.clone())
            .collect::<Vec<_>>()
            .join(" / ")
    }

    pub fn refresh_visible(&mut self) {
        self.visible.clear();
        let root_id = self.graph.root_id.clone();
        self.push_visible(&root_id, 0);
        if self.selected_index >= self.visible.len() {
            self.selected_index = self.visible.len().saturating_sub(1);
        }
    }

    fn push_visible(&mut self, id: &str, depth: usize) {
        self.visible.push((id.to_string(), depth));
        if self.collapsed.contains(id) {
            return;
        }

        let children = self
            .graph
            .node(id)
            .map(|node| node.children.clone())
            .unwrap_or_default();

        for child in children {
            self.push_visible(&child, depth + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::domain::{ArchitectureNode, Repository};
    use crate::inference::InferenceEngine;
    use crate::infrastructure::InferenceCache;

    use super::*;

    fn sample_state() -> AppState {
        let temp = TempDir::new().expect("temp");
        let root = temp.keep();
        let repo = Repository::new("repo", root.clone(), root.clone(), None);
        let persistence = PersistenceStore::new(&root).expect("store");

        let mut graph = ArchitectureGraph::new(
            "system:repo".to_string(),
            ArchitectureNode::new(
                "system:repo".to_string(),
                "repo".to_string(),
                NodeKind::System,
                ".".into(),
                None,
            ),
        );
        let container = ArchitectureNode::new(
            "container:src".to_string(),
            "src".to_string(),
            NodeKind::Container,
            "src".into(),
            Some("system:repo".to_string()),
        );
        graph.add_node(container);
        let component = ArchitectureNode::new(
            "component:src:main".to_string(),
            "main".to_string(),
            NodeKind::Component,
            "src/main.rs".into(),
            Some("container:src".to_string()),
        );
        graph.add_node(component);
        let code = ArchitectureNode::new(
            "code:src:main".to_string(),
            "src/main.rs".to_string(),
            NodeKind::CodeUnit,
            "src/main.rs".into(),
            Some("component:src:main".to_string()),
        );
        graph.add_node(code);

        let cache = InferenceCache::open(&repo.canopy_dir().join("cache.db")).expect("cache");
        let inference = InferenceEngine::new(
            None,
            cache,
            "Understand repository architecture".to_string(),
        );

        AppState::new(repo, persistence, graph, inference, "tester".to_string())
    }

    #[test]
    fn navigates_down_and_up() {
        let mut state = sample_state();
        assert_eq!(
            state.selected_node().map(|n| n.kind),
            Some(NodeKind::System)
        );

        state.apply(KeyAction::Drill).expect("drill 1");
        assert_eq!(
            state.selected_node().map(|n| n.kind),
            Some(NodeKind::Container)
        );
        state.apply(KeyAction::Drill).expect("drill 2");
        assert_eq!(
            state.selected_node().map(|n| n.kind),
            Some(NodeKind::Component)
        );
        state.apply(KeyAction::Drill).expect("drill 3");
        assert_eq!(
            state.selected_node().map(|n| n.kind),
            Some(NodeKind::CodeUnit)
        );

        state.apply(KeyAction::Back).expect("up");
        assert_eq!(
            state.selected_node().map(|n| n.kind),
            Some(NodeKind::Component)
        );
    }

    #[test]
    fn supports_collapse_expand() {
        let mut state = sample_state();
        let baseline = state.visible.len();
        state.toggle_collapse();
        assert!(state.visible.len() < baseline);
        state.toggle_collapse();
        assert_eq!(state.visible.len(), baseline);
    }

    #[test]
    fn handles_query_with_mentions() {
        let mut state = sample_state();
        state.mode = AppMode::Query;
        for ch in "show @main".chars() {
            state.apply(KeyAction::Input(ch)).expect("input");
        }
        state.apply(KeyAction::Accept).expect("submit");
        assert!(state.last_query.is_some());
        assert!(state
            .last_query
            .as_ref()
            .map(|q| !q.references.is_empty())
            .unwrap_or(false));
    }

    #[test]
    fn query_mode_recalls_history_with_arrows() {
        let mut state = sample_state();
        state.mode = AppMode::Query;
        for ch in "first query".chars() {
            state.apply(KeyAction::Input(ch)).expect("input");
        }
        state.apply(KeyAction::Accept).expect("submit");

        state.mode = AppMode::Query;
        state.apply(KeyAction::Up).expect("history up");
        assert_eq!(state.input_buffer, "first query");

        state.apply(KeyAction::Down).expect("history down");
        assert_eq!(state.input_buffer, "");
    }
}
