use std::fs;

use canopy_lib::application::query::answer_query;
use canopy_lib::domain::graph::{Provenance, ProvenanceSource};
use canopy_lib::domain::{ArchitectureGraph, ArchitectureNode, EditLogEntry, NodeKind};
use canopy_lib::infrastructure::{
    discover_repository, map_repository_architecture, map_repository_architecture_with_policy,
    C4MappingPolicy, FileMappingRule, PersistenceStore,
};
use chrono::Utc;
use tempfile::TempDir;

fn make_repo() -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    fs::create_dir_all(dir.path().join("src")).expect("src dir");
    fs::write(
        dir.path().join("src/main.rs"),
        "mod api; use api::handler; fn main() { handler(); }",
    )
    .expect("main");
    fs::write(dir.path().join("src/api.rs"), "pub fn handler() {}").expect("api");

    std::process::Command::new("git")
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("git init");

    dir
}

#[test]
fn discovers_repository_and_maps_c4_tree() {
    let repo = make_repo();
    let repository = discover_repository(repo.path()).expect("discover");
    let graph = map_repository_architecture(&repository).expect("map");

    assert!(graph.nodes.values().any(|n| n.kind == NodeKind::System));
    assert!(graph.nodes.values().any(|n| n.kind == NodeKind::Container));
    assert!(graph.nodes.values().any(|n| n.kind == NodeKind::Component));
    assert!(graph.nodes.values().any(|n| n.kind == NodeKind::CodeUnit));
    graph.validate().expect("graph validates");
}

#[test]
fn query_mentions_resolve_and_return_references() {
    let mut graph = ArchitectureGraph::new(
        "system:test".to_string(),
        ArchitectureNode::new(
            "system:test".to_string(),
            "test".to_string(),
            NodeKind::System,
            ".".into(),
            None,
        ),
    );
    graph.add_node(ArchitectureNode::new(
        "component:auth".to_string(),
        "AuthService".to_string(),
        NodeKind::Component,
        "src/auth.rs".into(),
        Some("system:test".to_string()),
    ));

    let answer = answer_query(&graph, "How does @AuthService work?");
    assert!(!answer.references.is_empty());
}

#[test]
fn persists_graph_and_edit_log() {
    let repo = make_repo();
    let repository = discover_repository(repo.path()).expect("discover");
    let graph = map_repository_architecture(&repository).expect("map");
    let store = PersistenceStore::new(&repository.root).expect("store");

    store.save_graph(&graph).expect("save graph");
    let restored = store.load_graph().expect("load").expect("some graph");
    assert_eq!(restored.root_id, graph.root_id);

    let edit = EditLogEntry {
        timestamp: Utc::now(),
        author: "tester".to_string(),
        component_path: "component:auth".to_string(),
        field: "summary".to_string(),
        before: "before".to_string(),
        after: "after".to_string(),
        reason: Some("clarify".to_string()),
        provenance: Provenance {
            source: ProvenanceSource::Human,
            author: Some("tester".to_string()),
            reason: Some("clarify".to_string()),
            edited_at: Some(Utc::now()),
        },
    };
    store.append_edit(&edit).expect("append edit");
    let edits = store.read_edits().expect("read edits");
    assert_eq!(edits.len(), 1);
}

#[test]
fn python_init_files_do_not_become_components() {
    let dir = TempDir::new().expect("temp dir");
    fs::create_dir_all(dir.path().join("src/pkg")).expect("pkg dir");
    fs::write(
        dir.path().join("src/pkg/__init__.py"),
        "from .service import run",
    )
    .expect("init");
    fs::write(
        dir.path().join("src/pkg/service.py"),
        "def run():\n    pass\n",
    )
    .expect("service");

    std::process::Command::new("git")
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("git init");

    let repository = discover_repository(dir.path()).expect("discover");
    let graph = map_repository_architecture(&repository).expect("map");
    let component_names: Vec<String> = graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Component)
        .map(|n| n.name.clone())
        .collect();

    assert!(!component_names.iter().any(|name| name == "__init__"));
    assert!(component_names.iter().any(|name| name == "pkg"));
    assert!(component_names.iter().any(|name| name == "service"));
}

#[test]
fn applies_llm_policy_mapping_for_component_grouping() {
    let dir = TempDir::new().expect("temp dir");
    fs::create_dir_all(dir.path().join("src/pkg")).expect("pkg dir");
    fs::write(
        dir.path().join("src/pkg/__init__.py"),
        "from .service import run",
    )
    .expect("init");
    fs::write(
        dir.path().join("src/pkg/service.py"),
        "def run():\n    pass\n",
    )
    .expect("service");

    std::process::Command::new("git")
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("git init");

    let repository = discover_repository(dir.path()).expect("discover");
    let policy = C4MappingPolicy {
        purpose: "Group package entrypoints with service behavior".to_string(),
        generated_at: Utc::now(),
        provider: Some("test".to_string()),
        model: Some("test-model".to_string()),
        notes: None,
        mappings: vec![
            FileMappingRule {
                file: "src/pkg/__init__.py".to_string(),
                include: true,
                container: Some("app".to_string()),
                component: Some("auth_core".to_string()),
                confidence: 0.91,
                rationale: "package export".to_string(),
            },
            FileMappingRule {
                file: "src/pkg/service.py".to_string(),
                include: true,
                container: Some("app".to_string()),
                component: Some("auth_core".to_string()),
                confidence: 0.94,
                rationale: "service behavior".to_string(),
            },
        ],
    };
    let graph = map_repository_architecture_with_policy(&repository, &policy).expect("map");
    let component_names: Vec<String> = graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Component)
        .map(|n| n.name.clone())
        .collect();

    assert!(component_names.iter().any(|name| name == "auth_core"));
    assert!(!component_names.iter().any(|name| name == "__init__"));
}

#[test]
fn policy_mapping_rejects_missing_file_entries() {
    let dir = TempDir::new().expect("temp dir");
    fs::create_dir_all(dir.path().join("src/pkg")).expect("pkg dir");
    fs::write(
        dir.path().join("src/pkg/__init__.py"),
        "from .service import run",
    )
    .expect("init");
    fs::write(
        dir.path().join("src/pkg/service.py"),
        "def run():\n    pass\n",
    )
    .expect("service");

    std::process::Command::new("git")
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("git init");

    let repository = discover_repository(dir.path()).expect("discover");
    let incomplete_policy = C4MappingPolicy {
        purpose: "Incomplete test policy".to_string(),
        generated_at: Utc::now(),
        provider: Some("test".to_string()),
        model: Some("test-model".to_string()),
        notes: None,
        mappings: vec![FileMappingRule {
            file: "src/pkg/__init__.py".to_string(),
            include: true,
            container: Some("app".to_string()),
            component: Some("auth_core".to_string()),
            confidence: 0.91,
            rationale: "package export".to_string(),
        }],
    };

    let result = map_repository_architecture_with_policy(&repository, &incomplete_policy);
    assert!(result.is_err());
}
