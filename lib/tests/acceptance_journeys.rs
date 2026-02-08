use std::fs;
use std::path::PathBuf;

use canopy_lib::application::{AppMode, AppState, KeyAction};
use canopy_lib::domain::{ArchitectureGraph, ArchitectureNode, NodeKind, Repository};
use canopy_lib::inference::InferenceEngine;
use canopy_lib::infrastructure::{
    discover_repository, map_repository_architecture, InferenceCache, PersistenceStore,
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
    let inference = InferenceEngine::new(
        None,
        cache,
        "Understand repository architecture".to_string(),
    );
    AppState::new(repo, persistence, graph, inference, "tester".to_string())
}

#[test]
fn given_repository_when_mapped_then_c4_layers_exist() {
    let repo = make_repo();
    let discovered = discover_repository(repo.path()).expect("discover");
    let graph = map_repository_architecture(&discovered).expect("map");

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
