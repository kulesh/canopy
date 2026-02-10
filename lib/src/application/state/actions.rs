use chrono::Utc;

use crate::application::query::answer_query;
use crate::domain::graph::{Provenance, ProvenanceSource};
use crate::domain::{EditLogEntry, ExportFormat};
use crate::error::Result;

use super::{AppMode, AppState, FocusPane, KeyAction};

impl AppState {
    pub fn apply(&mut self, action: KeyAction) -> Result<()> {
        match self.mode {
            AppMode::Normal => self.apply_normal(action),
            AppMode::Project => self.apply_project(action),
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
            KeyAction::Regenerate => self.start_regeneration()?,
            KeyAction::ToggleHelp => {
                self.mode = AppMode::Help;
            }
            KeyAction::ToggleProjectView => {
                self.mode = AppMode::Project;
                self.status_line =
                    "Project view: o queue | c cancel | R retry | s switch".to_string();
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

    fn apply_project(&mut self, action: KeyAction) -> Result<()> {
        match action {
            KeyAction::Up => self.move_project_selection(-1),
            KeyAction::Down => self.move_project_selection(1),
            KeyAction::PageUp => self.move_project_selection(-8),
            KeyAction::PageDown => self.move_project_selection(8),
            KeyAction::QueueOnboarding => self.queue_selected_project_repository(),
            KeyAction::CancelOnboarding => self.cancel_selected_project_repository(),
            KeyAction::RetryOnboarding => self.retry_selected_project_repository(),
            KeyAction::SwitchRepository | KeyAction::Accept => {
                self.switch_to_selected_project_repository()?;
            }
            KeyAction::ToggleProjectView | KeyAction::Cancel | KeyAction::Back => {
                self.mode = AppMode::Normal;
            }
            _ => {}
        }
        Ok(())
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
                if let Some(source) = self.regenerate_selected_node(&node_id)? {
                    self.status_line = format!("Summary regenerated ({source})");
                } else {
                    self.status_line = "Summary regeneration skipped".to_string();
                }
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

    fn start_regeneration(&mut self) -> Result<()> {
        let Some(node) = self.selected_node() else {
            return Ok(());
        };

        if node.provenance.source == ProvenanceSource::Human {
            self.mode = AppMode::ConfirmRegenerate;
            self.status_line =
                "Node was human-edited. Press Enter to confirm regeneration".to_string();
            return Ok(());
        }

        if let Some(id) = self.selected_node_id().map(ToString::to_string) {
            if let Some(source) = self.regenerate_selected_node(&id)? {
                self.status_line = format!("Summary regenerated ({source})");
            } else {
                self.status_line = "Summary regeneration skipped".to_string();
            }
        }
        Ok(())
    }

    fn regenerate_selected_node(&mut self, node_id: &str) -> Result<Option<&'static str>> {
        let Some((summary, confidence, source)) =
            self.inference.regenerate_node(&self.graph, node_id)
        else {
            return Ok(None);
        };
        let Some(node) = self.graph.node_mut(node_id) else {
            return Ok(None);
        };

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
            component_path: node_id.to_string(),
            field: "summary".to_string(),
            before,
            after: summary,
            reason: Some(format!("Regenerate summary ({source})")),
            provenance: node.provenance.clone(),
        };
        self.persistence.append_edit(&entry)?;
        self.persistence.save_graph(&self.graph)?;

        Ok(Some(source))
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
