use ratatui::layout::{Constraint, Direction, Layout};

use crate::application::state::{AppMode, AppState, FocusPane};

mod detail_panel;
mod overlays;
mod project_panel;
mod right_panel;
mod syntax;
mod theme;
mod tree_panel;

use detail_panel::render_details;
use overlays::{render_help_modal, render_status};
use project_panel::render_project_panel;
use right_panel::render_right_panel;
use theme::Theme;
use tree_panel::render_tree;

pub(super) fn render(frame: &mut ratatui::Frame<'_>, state: &AppState) {
    let theme = Theme::flight_deck();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(6)])
        .split(frame.area());

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(chunks[0]);

    if state.mode == AppMode::Project {
        render_project_panel(frame, chunks[0], state, theme);
    } else {
        render_tree(frame, body[0], state, theme, state.focus == FocusPane::Tree);
        render_details(
            frame,
            body[1],
            state,
            theme,
            state.focus == FocusPane::Semantic,
        );
        render_right_panel(
            frame,
            body[2],
            state,
            theme,
            state.focus == FocusPane::Right,
        );
    }
    render_status(frame, chunks[1], state, theme);
    if state.mode == AppMode::Help {
        render_help_modal(frame, theme);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tempfile::TempDir;

    use crate::application::state::{AppMode, AppState};
    use crate::domain::{
        ArchitectureGraph, ArchitectureNode, NodeKind, OnboardingPhase, Project, ProjectRepository,
        ProjectRepositoryState, ProjectRuntimeState, ProjectSettings, Repository,
    };
    use crate::inference::InferenceEngine;
    use crate::infrastructure::{InferenceCache, PersistenceStore};

    use super::render;

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
                PathBuf::from("."),
                None,
            ),
        );
        graph.add_node(ArchitectureNode::new(
            "container:src".to_string(),
            "src".to_string(),
            NodeKind::Container,
            PathBuf::from("src"),
            Some("system:repo".to_string()),
        ));
        graph.add_node(ArchitectureNode::new(
            "component:src:main".to_string(),
            "main".to_string(),
            NodeKind::Component,
            PathBuf::from("src/main.rs"),
            Some("container:src".to_string()),
        ));
        graph.add_node(ArchitectureNode::new(
            "code:src:main".to_string(),
            "src/main.rs".to_string(),
            NodeKind::CodeUnit,
            PathBuf::from("src/main.rs"),
            Some("component:src:main".to_string()),
        ));

        let cache = InferenceCache::open(&repo.canopy_dir().join("cache.db")).expect("cache");
        let inference = InferenceEngine::new(
            None,
            cache,
            "Understand repository architecture".to_string(),
        );

        AppState::new(repo, persistence, graph, inference, "tester".to_string())
    }

    fn rendered_text(state: &AppState) -> String {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(frame, state))
            .expect("render draw");
        let buffer = terminal.backend().buffer().clone();
        buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn snapshot_contains_core_panes() {
        let state = sample_state();
        let output = rendered_text(&state);
        assert!(output.contains("Architecture [Tab]"));
        assert!(output.contains("Semantic Layer [Tab]"));
        assert!(output.contains("Code [Tab, j/k, PgUp/PgDn]"));
        assert!(output.contains("Key Bindings"));
        assert!(output.contains("STATUS"));
    }

    #[test]
    fn snapshot_contains_help_overlay() {
        let mut state = sample_state();
        state.mode = AppMode::Help;
        let output = rendered_text(&state);
        assert!(output.contains("Help"));
        assert!(output.contains("Navigation"));
        assert!(output.contains("Query & Editing"));
    }

    #[test]
    fn snapshot_contains_project_view() {
        let mut state = sample_state();
        let project = Project {
            name: "demo".to_string(),
            repositories: vec![ProjectRepository {
                id: "repo-a".to_string(),
                name: "repo-a".to_string(),
                path: state.repository.root.clone(),
                enabled: true,
            }],
            active_repository_id: Some("repo-a".to_string()),
            settings: ProjectSettings::default(),
        };
        let mut runtime = ProjectRuntimeState::default();
        runtime.repositories.insert(
            "repo-a".to_string(),
            ProjectRepositoryState {
                repository_id: "repo-a".to_string(),
                phase: OnboardingPhase::Ready,
                progress_percent: Some(100),
                message: Some("onboarding completed".to_string()),
                last_error: None,
                last_ready_at: None,
                updated_at: chrono::Utc::now(),
            },
        );
        state.configure_project(
            project,
            runtime,
            state.repository.root.join("project.toml"),
            "repo-a".to_string(),
            "test purpose".to_string(),
        );
        state.mode = AppMode::Project;

        let output = rendered_text(&state);
        assert!(output.contains("Project View"));
        assert!(output.contains("Repositories"));
        assert!(output.contains("ready"));
    }
}
