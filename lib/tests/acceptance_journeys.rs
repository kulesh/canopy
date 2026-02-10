use std::fs;
use std::path::PathBuf;

use canopy_lib::application::{AppMode, AppState, KeyAction};
use canopy_lib::domain::{ArchitectureGraph, ArchitectureNode, NodeKind, Repository};
use canopy_lib::inference::{InferenceEngine, InferenceExecutionMode};
use canopy_lib::infrastructure::{
    collect_source_tree_snapshot, discover_repository, map_repository_architecture_with_policy,
    C4MappingPolicy, ComponentContribution, EvidenceSpan, FileMappingRule, InferenceCache,
    PersistenceStore,
};
use tempfile::TempDir;

fn init_git(path: &std::path::Path) {
    std::process::Command::new("git")
        .arg("init")
        .current_dir(path)
        .output()
        .expect("git init");
}

fn make_repo() -> TempDir {
    let dir = TempDir::new().expect("temp");
    fs::create_dir_all(dir.path().join("src")).expect("src");
    fs::write(dir.path().join("src/main.rs"), "fn main() {}").expect("main");
    init_git(dir.path());
    dir
}

fn strict_smoke_policy(repository: &Repository) -> C4MappingPolicy {
    let snapshot = collect_source_tree_snapshot(repository).expect("snapshot");
    let mappings = snapshot
        .files
        .iter()
        .map(|file| FileMappingRule {
            file: file.clone(),
            include: true,
            container: Some("src".to_string()),
            component: Some("main".to_string()),
            confidence: 0.8,
            rationale: "test scaffold".to_string(),
        })
        .collect::<Vec<_>>();
    let contributions = snapshot
        .files
        .iter()
        .map(|file| ComponentContribution {
            file: file.clone(),
            container: "src".to_string(),
            component: "main".to_string(),
            confidence: 0.8,
            rationale: "test scaffold".to_string(),
            evidence: vec![EvidenceSpan {
                file: file.clone(),
                start_line: 1,
                end_line: 1,
                excerpt: None,
                reason: "test scaffold evidence".to_string(),
            }],
        })
        .collect::<Vec<_>>();
    C4MappingPolicy {
        purpose: "test scaffold".to_string(),
        generated_at: chrono::Utc::now(),
        provider: Some("test".to_string()),
        model: Some("test-model".to_string()),
        notes: None,
        mappings,
        contributions,
        dependencies: vec![],
        semantic_asts: vec![],
    }
}

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
    if let Some(node) = graph.node_mut("component:src:main") {
        node.summary = "old summary".to_string();
    }

    let cache = InferenceCache::open(&repo.canopy_dir().join("cache.db")).expect("cache");
    let inference = InferenceEngine::new_with_mode(
        None,
        cache,
        "Understand repository architecture".to_string(),
        InferenceExecutionMode::HybridFallback,
    );
    AppState::new(repo, persistence, graph, inference, "tester".to_string())
}

#[test]
fn given_repository_when_mapped_then_c4_layers_exist() {
    let repo = make_repo();
    let discovered = discover_repository(repo.path()).expect("discover");
    let policy = strict_smoke_policy(&discovered);
    let graph = map_repository_architecture_with_policy(&discovered, &policy).expect("map");

    assert!(graph.nodes.values().any(|n| n.kind == NodeKind::System));
    assert!(graph.nodes.values().any(|n| n.kind == NodeKind::Container));
    assert!(graph.nodes.values().any(|n| n.kind == NodeKind::Component));
    assert!(graph.nodes.values().any(|n| n.kind == NodeKind::CodeUnit));
}

#[test]
fn given_summary_edit_flow_when_saved_then_edit_log_records_provenance() {
    let mut state = sample_state();
    state.jump_to("component:src:main");
    state.apply(KeyAction::Edit).expect("enter edit");

    state.edit_buffer = "new behavior summary".to_string();
    state.apply(KeyAction::Accept).expect("to reason mode");
    for ch in "clarify intent".chars() {
        state.apply(KeyAction::Input(ch)).expect("reason");
    }
    state.apply(KeyAction::Accept).expect("save");

    let edits = state.persistence.read_edits().expect("edits");
    assert_eq!(state.mode, AppMode::Normal);
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].after, "new behavior summary");
    assert_eq!(edits[0].author, "tester");
}

#[test]
fn given_export_confirmation_when_accept_then_export_file_created() {
    let mut state = sample_state();
    state
        .apply(KeyAction::Export)
        .expect("enter export confirm");
    assert_eq!(state.mode, AppMode::ConfirmExport);
    state.apply(KeyAction::Accept).expect("export");

    let export_path = state.persistence.canopy_dir.join("edit_log_export.json");
    assert!(export_path.exists());
}

#[test]
fn given_non_human_regenerate_when_triggered_then_persists_and_audits() {
    let mut state = sample_state();
    state.jump_to("component:src:main");

    let before = state
        .graph
        .node("component:src:main")
        .map(|node| node.summary.clone())
        .expect("component");
    assert_eq!(before, "old summary");

    state.apply(KeyAction::Regenerate).expect("regenerate");

    let edits = state.persistence.read_edits().expect("edits");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].component_path, "component:src:main");
    assert!(edits[0]
        .reason
        .as_deref()
        .unwrap_or_default()
        .contains("Regenerate summary"));

    let persisted = state
        .persistence
        .load_graph()
        .expect("load graph")
        .expect("graph");
    let current_summary = state
        .graph
        .node("component:src:main")
        .map(|node| node.summary.clone())
        .expect("component");
    let persisted_summary = persisted
        .node("component:src:main")
        .map(|node| node.summary.clone())
        .expect("component");

    assert_ne!(current_summary, before);
    assert_eq!(persisted_summary, current_summary);
}
