mod analysis;
mod keymap;

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, TryRecvError};

use chrono::Utc;

use crate::application::query::answer_query;
use crate::domain::graph::{Provenance, ProvenanceSource};
use crate::domain::{
    ArchitectureGraph, EditLogEntry, ExportFormat, NodeKind, QueryAnswer, Repository,
};
use crate::error::Result;
use crate::inference::{InferenceEngine, InferenceProgress};
use crate::infrastructure::{CoverageMap, GitSignals, PersistenceStore};

use self::analysis::detect_cycles;
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

    pub fn apply(&mut self, action: KeyAction) -> Result<()> {
        match self.mode {
            AppMode::Normal => self.apply_normal(action),
            AppMode::Search => self.apply_search(action),
            AppMode::Query => self.apply_query(action),
            AppMode::EditSummary => self.apply_edit_summary(action),
            AppMode::EditReason => self.apply_edit_reason(action),
            AppMode::ConfirmRegenerate => self.apply_regenerate_confirm(action),
            AppMode::ConfirmExport => self.apply_export_confirm(action),
            AppMode::Help | AppMode::History => self.apply_overlay(action),
        }
    }

    fn apply_normal(&mut self, action: KeyAction) -> Result<()> {
        match action {
            KeyAction::Up => self.scroll_or_move(-1),
            KeyAction::Down => self.scroll_or_move(1),
            KeyAction::Top => {
                if self.focus == FocusPane::Right {
                    self.right_scroll = 0;
                } else {
                    self.selected_index = 0;
                }
            }
            KeyAction::Bottom => {
                if self.focus == FocusPane::Right {
                    self.right_scroll = self.right_scroll.saturating_add(200);
                } else {
                    self.selected_index = self.visible.len().saturating_sub(1);
                }
            }
            KeyAction::Left | KeyAction::Back => {
                if self.focus == FocusPane::Tree {
                    self.navigate_up();
                } else {
                    self.right_scroll = self.right_scroll.saturating_sub(5);
                }
            }
            KeyAction::Right => {
                if self.focus == FocusPane::Tree {
                    self.toggle_collapse();
                } else {
                    self.right_scroll = self.right_scroll.saturating_add(5);
                }
            }
            KeyAction::Drill => {
                if self.focus == FocusPane::Tree {
                    self.navigate_down();
                } else {
                    self.right_scroll = self.right_scroll.saturating_add(5);
                }
            }
            KeyAction::PageUp => {
                if self.focus == FocusPane::Right {
                    self.right_scroll = self.right_scroll.saturating_sub(20);
                } else {
                    self.move_selection(-10);
                }
            }
            KeyAction::PageDown => {
                if self.focus == FocusPane::Right {
                    self.right_scroll = self.right_scroll.saturating_add(20);
                } else {
                    self.move_selection(10);
                }
            }
            KeyAction::NextPane => {
                self.focus = self.focus.next();
                self.status_line = format!("Focus: {:?}", self.focus);
            }
            KeyAction::PrevPane => {
                self.focus = self.focus.prev();
                self.status_line = format!("Focus: {:?}", self.focus);
            }
            KeyAction::Search => {
                self.mode = AppMode::Search;
                self.input_buffer.clear();
            }
            KeyAction::Query => {
                self.mode = AppMode::Query;
                self.input_buffer.clear();
                self.query_history_cursor = None;
            }
            KeyAction::Edit => {
                if let Some(summary) = self.selected_node().map(|n| n.summary.clone()) {
                    self.edit_buffer = summary;
                    self.mode = AppMode::EditSummary;
                }
            }
            KeyAction::Regenerate => self.start_regeneration(),
            KeyAction::ToggleHelp => {
                self.mode = AppMode::Help;
            }
            KeyAction::ShowHistory => {
                self.mode = AppMode::History;
            }
            KeyAction::ToggleGraph => {
                self.show_dependency_graph = !self.show_dependency_graph;
            }
            KeyAction::ToggleImpact => {
                self.show_impact = !self.show_impact;
            }
            KeyAction::Export => {
                self.mode = AppMode::ConfirmExport;
                self.status_line =
                    "Export edit log? Press Enter to confirm or Esc to cancel".to_string();
            }
            KeyAction::Cancel => {
                self.should_quit = true;
            }
            _ => {}
        }
        Ok(())
    }

    fn scroll_or_move(&mut self, delta: isize) {
        if self.focus == FocusPane::Right {
            if delta.is_negative() {
                self.right_scroll = self.right_scroll.saturating_sub(delta.unsigned_abs());
            } else {
                self.right_scroll = self.right_scroll.saturating_add(delta as usize);
            }
        } else {
            self.move_selection(delta);
        }
    }

    fn apply_search(&mut self, action: KeyAction) -> Result<()> {
        match action {
            KeyAction::Input(ch) => {
                self.input_buffer.push(ch);
                self.query_history_cursor = None;
            }
            KeyAction::Backspace => {
                self.input_buffer.pop();
            }
            KeyAction::Accept => {
                let term = self.input_buffer.to_lowercase();
                if !term.is_empty() {
                    if let Some((idx, _)) = self.visible.iter().enumerate().find(|(_, (id, _))| {
                        self.graph
                            .node(id)
                            .map(|n| {
                                n.name.to_lowercase().contains(&term)
                                    || n.path.to_string_lossy().to_lowercase().contains(&term)
                            })
                            .unwrap_or(false)
                    }) {
                        self.selected_index = idx;
                        self.status_line = format!("Search matched '{term}'");
                    } else {
                        self.status_line = format!("No matches for '{term}'");
                    }
                }
                self.mode = AppMode::Normal;
            }
            KeyAction::Cancel => {
                self.mode = AppMode::Normal;
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_query(&mut self, action: KeyAction) -> Result<()> {
        match action {
            KeyAction::Input(ch) => {
                self.input_buffer.push(ch);
                self.query_history_cursor = None;
            }
            KeyAction::Up => self.query_history_step_back(),
            KeyAction::Down => self.query_history_step_forward(),
            KeyAction::Backspace => {
                self.input_buffer.pop();
                self.query_history_cursor = None;
            }
            KeyAction::Accept => {
                let query = self.input_buffer.trim().to_string();
                if !query.is_empty() {
                    let answer = answer_query(&self.graph, &query);
                    self.persistence.append_query(&answer)?;
                    self.last_query = Some(answer.clone());
                    self.query_history.push(answer.clone());
                    self.status_line = answer.response.clone();
                    if let Some(reference) = answer.references.first() {
                        self.jump_to(&reference.node_id);
                    }
                }
                self.input_buffer.clear();
                self.query_history_cursor = None;
                self.mode = AppMode::Normal;
            }
            KeyAction::Cancel => {
                self.mode = AppMode::Normal;
                self.input_buffer.clear();
                self.query_history_cursor = None;
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_edit_summary(&mut self, action: KeyAction) -> Result<()> {
        match action {
            KeyAction::Input(ch) => self.edit_buffer.push(ch),
            KeyAction::Backspace => {
                self.edit_buffer.pop();
            }
            KeyAction::Accept => {
                self.mode = AppMode::EditReason;
                self.reason_buffer.clear();
                self.status_line = "Optional reason for edit (Enter to save)".to_string();
            }
            KeyAction::Cancel => {
                self.mode = AppMode::Normal;
                self.edit_buffer.clear();
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_edit_reason(&mut self, action: KeyAction) -> Result<()> {
        match action {
            KeyAction::Input(ch) => self.reason_buffer.push(ch),
            KeyAction::Backspace => {
                self.reason_buffer.pop();
            }
            KeyAction::Accept => {
                let node_id = match self.selected_node_id() {
                    Some(id) => id.to_string(),
                    None => return Ok(()),
                };
                let before = self
                    .graph
                    .node(&node_id)
                    .map(|n| n.summary.clone())
                    .unwrap_or_default();
                let after = self.edit_buffer.clone();
                let reason = if self.reason_buffer.trim().is_empty() {
                    None
                } else {
                    Some(self.reason_buffer.trim().to_string())
                };

                if let Some(node) = self.graph.node_mut(&node_id) {
                    node.summary = after.clone();
                    node.provenance = Provenance::human(Some(self.author.clone()), reason.clone());
                    node.confidence = 1.0;
                    node.last_analyzed = Some(Utc::now());
                }

                let entry = EditLogEntry {
                    timestamp: Utc::now(),
                    author: self.author.clone(),
                    component_path: node_id.clone(),
                    field: "summary".to_string(),
                    before,
                    after,
                    reason,
                    provenance: Provenance::human(
                        Some(self.author.clone()),
                        self.reason_buffer_to_option(),
                    ),
                };
                self.persistence.append_edit(&entry)?;
                self.persistence.save_graph(&self.graph)?;

                self.edit_buffer.clear();
                self.reason_buffer.clear();
                self.status_line = "Summary updated".to_string();
                self.mode = AppMode::Normal;
            }
            KeyAction::Cancel => {
                self.reason_buffer.clear();
                self.mode = AppMode::Normal;
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_regenerate_confirm(&mut self, action: KeyAction) -> Result<()> {
        match action {
            KeyAction::Accept => {
                let Some(node_id) = self.selected_node_id().map(ToString::to_string) else {
                    self.mode = AppMode::Normal;
                    return Ok(());
                };
                if let Some((summary, confidence, source)) =
                    self.inference.regenerate_node(&self.graph, &node_id)
                {
                    if let Some(node) = self.graph.node_mut(&node_id) {
                        let before = node.summary.clone();
                        node.summary = summary.clone();
                        node.provenance = Provenance {
                            source: ProvenanceSource::Ai,
                            author: None,
                            reason: Some(format!("Regenerated via {source}")),
                            edited_at: Some(Utc::now()),
                        };
                        node.confidence = confidence;
                        node.last_analyzed = Some(Utc::now());

                        let entry = EditLogEntry {
                            timestamp: Utc::now(),
                            author: self.author.clone(),
                            component_path: node_id.clone(),
                            field: "summary".to_string(),
                            before,
                            after: summary,
                            reason: Some(format!("Regenerate summary ({source})")),
                            provenance: node.provenance.clone(),
                        };
                        self.persistence.append_edit(&entry)?;
                    }
                }
                self.persistence.save_graph(&self.graph)?;
                self.status_line = "Summary regenerated".to_string();
                self.mode = AppMode::Normal;
            }
            KeyAction::Cancel => {
                self.mode = AppMode::Normal;
                self.status_line = "Regenerate canceled".to_string();
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_export_confirm(&mut self, action: KeyAction) -> Result<()> {
        match action {
            KeyAction::Accept => {
                let path = self.persistence.export_edit_log(ExportFormat::Json)?;
                self.status_line = format!("Exported edit log to {}", path.display());
                self.mode = AppMode::Normal;
            }
            KeyAction::Cancel => {
                self.mode = AppMode::Normal;
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_overlay(&mut self, action: KeyAction) -> Result<()> {
        if matches!(
            action,
            KeyAction::Cancel | KeyAction::Accept | KeyAction::ToggleHelp
        ) {
            self.mode = AppMode::Normal;
        }
        Ok(())
    }

    fn move_selection(&mut self, delta: isize) {
        if self.visible.is_empty() {
            self.selected_index = 0;
            return;
        }
        let max = self.visible.len() - 1;
        if delta.is_negative() {
            self.selected_index = self.selected_index.saturating_sub(delta.unsigned_abs());
        } else {
            self.selected_index = (self.selected_index + delta as usize).min(max);
        }
    }

    fn navigate_up(&mut self) {
        let Some(selected_id) = self.selected_node_id().map(ToString::to_string) else {
            return;
        };
        let Some(parent_id) = self
            .graph
            .node(&selected_id)
            .and_then(|node| node.parent_id.clone())
        else {
            return;
        };
        self.jump_to(&parent_id);
    }

    fn navigate_down(&mut self) {
        let Some(selected_id) = self.selected_node_id().map(ToString::to_string) else {
            return;
        };

        let child = self
            .graph
            .node(&selected_id)
            .and_then(|node| node.children.first().cloned());

        if let Some(next) = child {
            self.jump_to(&next);
        }
    }

    fn start_regeneration(&mut self) {
        let Some(node) = self.selected_node() else {
            return;
        };

        if node.provenance.source == ProvenanceSource::Human {
            self.mode = AppMode::ConfirmRegenerate;
            self.status_line =
                "Node was human-edited. Press Enter to confirm regeneration".to_string();
            return;
        }

        if let Some(id) = self.selected_node_id().map(ToString::to_string) {
            if let Some((summary, confidence, source)) =
                self.inference.regenerate_node(&self.graph, &id)
            {
                if let Some(node) = self.graph.node_mut(&id) {
                    node.summary = summary;
                    node.provenance = Provenance::ai();
                    node.confidence = confidence;
                    node.last_analyzed = Some(Utc::now());
                    self.status_line = format!("Summary regenerated ({source})");
                }
            }
        }
    }

    pub fn toggle_collapse(&mut self) {
        let Some(id) = self.selected_node_id().map(ToString::to_string) else {
            return;
        };
        if self.collapsed.contains(&id) {
            self.collapsed.remove(&id);
        } else {
            self.collapsed.insert(id);
        }
        self.refresh_visible();
    }

    pub fn jump_to(&mut self, node_id: &str) {
        if let Some(target) = self.visible.iter().position(|(id, _)| id == node_id) {
            self.selected_index = target;
            return;
        }

        let lineage = self.graph.lineage(node_id);
        for ancestor in lineage.iter().take(lineage.len().saturating_sub(1)) {
            self.collapsed.remove(ancestor);
        }
        self.refresh_visible();

        if let Some(target) = self.visible.iter().position(|(id, _)| id == node_id) {
            self.selected_index = target;
        }
    }

    pub fn dependency_graph_ascii(&self) -> String {
        let Some(node) = self.selected_node() else {
            return String::new();
        };

        let mut out = Vec::new();
        out.push(node.name.clone());
        for dep in &node.dependencies {
            let label = self
                .graph
                .node(dep)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| dep.clone());
            out.push(format!("  ├─ uses -> {label}"));
        }

        let cycles = detect_cycles(&self.graph);
        if !cycles.is_empty() {
            out.push("Cycles:".to_string());
            for cycle in cycles {
                out.push(format!("  • {}", cycle.join(" -> ")));
            }
        }

        out.join("\n")
    }

    pub fn impact_report(&self) -> String {
        let Some(id) = self.selected_node_id() else {
            return String::new();
        };
        let impacted = self.graph.impact_set(id, self.impact_depth);
        if impacted.is_empty() {
            return "No downstream impact detected at current depth".to_string();
        }
        let mut lines = vec![format!("Potential impact ({})", impacted.len())];
        for dep in impacted {
            if let Some(node) = self.graph.node(&dep) {
                lines.push(format!("  - {}", node.name));
            }
        }
        lines.join("\n")
    }

    pub fn coverage_for_selected(&self) -> Option<f32> {
        let node = self.selected_node()?;
        let key = node.path.to_string_lossy().to_string();
        self.coverage.get(&key).map(|stats| stats.percent())
    }

    pub fn recent_change_note(&self) -> Option<String> {
        let node = self.selected_node()?;
        let key = node.path.to_string_lossy().to_string();
        let changed = self.git_signals.recent_changes.get(&key)?;
        Some(format!(
            "Last changed: {}",
            changed.format("%Y-%m-%d %H:%M UTC")
        ))
    }

    pub fn blame_note(&self) -> Option<String> {
        let node = self.selected_node()?;
        let key = node.path.to_string_lossy().to_string();
        self.git_signals
            .blame_author
            .get(&key)
            .map(|author| format!("Last author: {author}"))
    }

    fn reason_buffer_to_option(&self) -> Option<String> {
        let reason = self.reason_buffer.trim();
        if reason.is_empty() {
            None
        } else {
            Some(reason.to_string())
        }
    }

    fn query_history_step_back(&mut self) {
        if self.query_history.is_empty() {
            return;
        }
        let next = match self.query_history_cursor {
            Some(cursor) => cursor.saturating_sub(1),
            None => self.query_history.len() - 1,
        };
        self.query_history_cursor = Some(next);
        if let Some(entry) = self.query_history.get(next) {
            self.input_buffer = entry.query.clone();
        }
    }

    fn query_history_step_forward(&mut self) {
        if self.query_history.is_empty() {
            return;
        }
        let Some(cursor) = self.query_history_cursor else {
            return;
        };
        if cursor + 1 >= self.query_history.len() {
            self.query_history_cursor = None;
            self.input_buffer.clear();
            return;
        }
        let next = cursor + 1;
        self.query_history_cursor = Some(next);
        if let Some(entry) = self.query_history.get(next) {
            self.input_buffer = entry.query.clone();
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
        let inference = InferenceEngine::new(None, cache);

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
