use std::fs;

use canopy_lib::application::query::answer_query;
use canopy_lib::domain::graph::{Provenance, ProvenanceSource};
use canopy_lib::domain::{ArchitectureGraph, ArchitectureNode, EditLogEntry, NodeKind, Repository};
use canopy_lib::infrastructure::{
    collect_source_tree_snapshot, discover_repository, map_repository_architecture_with_policy,
    C4MappingPolicy, ComponentContribution, EvidenceSpan, FileMappingRule, PersistenceStore,
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

fn strict_smoke_policy(repository: &Repository) -> C4MappingPolicy {
    let snapshot = collect_source_tree_snapshot(repository).expect("snapshot");
    let mut mappings = Vec::new();
    let mut contributions = Vec::new();
    for file in snapshot.files {
        let path = std::path::Path::new(&file);
        let container = path
            .components()
            .next()
            .and_then(|component| component.as_os_str().to_str())
            .unwrap_or("app")
            .to_string();
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("unit");
        let component = if stem == "__init__" || stem == "mod" || stem == "index" {
            path.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("app")
                .to_string()
        } else {
            stem.to_string()
        };
        mappings.push(FileMappingRule {
            file: file.clone(),
            include: true,
            container: Some(container.clone()),
            component: Some(component.clone()),
            confidence: 0.75,
            rationale: "test scaffold policy".to_string(),
        });
        contributions.push(ComponentContribution {
            file: file.clone(),
            container,
            component,
            confidence: 0.75,
            rationale: "test scaffold contribution".to_string(),
            evidence: vec![EvidenceSpan {
                file,
                start_line: 1,
                end_line: 1,
                excerpt: None,
                reason: "test scaffold evidence".to_string(),
            }],
        });
    }
    C4MappingPolicy {
        purpose: "test scaffold".to_string(),
        generated_at: Utc::now(),
        provider: Some("test".to_string()),
        model: Some("test-model".to_string()),
        notes: None,
        mappings,
        contributions,
        dependencies: vec![],
        semantic_asts: vec![],
    }
}

#[test]
fn discovers_repository_and_maps_c4_tree() {
    let repo = make_repo();
    let repository = discover_repository(repo.path()).expect("discover");
    let policy = strict_smoke_policy(&repository);
    let graph = map_repository_architecture_with_policy(&repository, &policy).expect("map");

    assert!(graph.nodes.values().any(|n| n.kind == NodeKind::System));
    assert!(graph.nodes.values().any(|n| n.kind == NodeKind::Container));
    assert!(graph.nodes.values().any(|n| n.kind == NodeKind::Component));
    assert!(graph.nodes.values().any(|n| n.kind == NodeKind::CodeUnit));
    graph.validate().expect("graph validates");
}

#[test]
fn mapping_fails_on_non_utf8_source_file() {
    let dir = TempDir::new().expect("temp dir");
    fs::create_dir_all(dir.path().join("src")).expect("src dir");
    fs::write(
        dir.path().join("src/main.rs"),
        [0_u8, 159_u8, 32_u8, 240_u8],
    )
    .expect("main");

    std::process::Command::new("git")
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("git init");

    let repository = discover_repository(dir.path()).expect("discover");
    let policy = strict_smoke_policy(&repository);
    let err =
        map_repository_architecture_with_policy(&repository, &policy).expect_err("map should fail");
    let message = err.to_string();
    assert!(message.contains("src/main.rs"));
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
    let policy = strict_smoke_policy(&repository);
    let graph = map_repository_architecture_with_policy(&repository, &policy).expect("map");
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
    let policy = strict_smoke_policy(&repository);
    let graph = map_repository_architecture_with_policy(&repository, &policy).expect("map");
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
        contributions: vec![
            ComponentContribution {
                file: "src/pkg/__init__.py".to_string(),
                container: "app".to_string(),
                component: "auth_core".to_string(),
                confidence: 0.9,
                rationale: "Exports service entrypoints".to_string(),
                evidence: vec![EvidenceSpan {
                    file: "src/pkg/__init__.py".to_string(),
                    start_line: 1,
                    end_line: 1,
                    excerpt: Some("from .service import run".to_string()),
                    reason: "Connects package boundary to behavior".to_string(),
                }],
            },
            ComponentContribution {
                file: "src/pkg/service.py".to_string(),
                container: "app".to_string(),
                component: "auth_core".to_string(),
                confidence: 0.94,
                rationale: "Implements service behavior".to_string(),
                evidence: vec![EvidenceSpan {
                    file: "src/pkg/service.py".to_string(),
                    start_line: 1,
                    end_line: 2,
                    excerpt: Some("def run():".to_string()),
                    reason: "Defines runtime behavior".to_string(),
                }],
            },
        ],
        dependencies: vec![],
        semantic_asts: vec![],
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
        contributions: vec![],
        dependencies: vec![],
        semantic_asts: vec![],
    };

    let result = map_repository_architecture_with_policy(&repository, &incomplete_policy);
    assert!(result.is_err());
}

#[test]
fn supports_multiple_component_contributions_from_single_file() {
    let dir = TempDir::new().expect("temp dir");
    fs::create_dir_all(dir.path().join("src")).expect("src dir");
    fs::write(
        dir.path().join("src/bridge.py"),
        "def auth_gate(user):\n    return user.is_active\n\ndef billing_gate(invoice):\n    return invoice.total > 0\n",
    )
    .expect("bridge");

    std::process::Command::new("git")
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("git init");

    let repository = discover_repository(dir.path()).expect("discover");
    let policy = C4MappingPolicy {
        purpose: "Model mixed responsibilities".to_string(),
        generated_at: Utc::now(),
        provider: Some("test".to_string()),
        model: Some("test-model".to_string()),
        notes: None,
        mappings: vec![FileMappingRule {
            file: "src/bridge.py".to_string(),
            include: true,
            container: Some("app".to_string()),
            component: Some("auth_flow".to_string()),
            confidence: 0.9,
            rationale: "primary placement for rendering".to_string(),
        }],
        contributions: vec![
            ComponentContribution {
                file: "src/bridge.py".to_string(),
                container: "app".to_string(),
                component: "auth_flow".to_string(),
                confidence: 0.94,
                rationale: "Defines authentication gate behavior".to_string(),
                evidence: vec![EvidenceSpan {
                    file: "src/bridge.py".to_string(),
                    start_line: 1,
                    end_line: 2,
                    excerpt: Some("def auth_gate".to_string()),
                    reason: "Auth boundary logic".to_string(),
                }],
            },
            ComponentContribution {
                file: "src/bridge.py".to_string(),
                container: "app".to_string(),
                component: "billing_flow".to_string(),
                confidence: 0.94,
                rationale: "Defines billing gate behavior".to_string(),
                evidence: vec![EvidenceSpan {
                    file: "src/bridge.py".to_string(),
                    start_line: 4,
                    end_line: 5,
                    excerpt: Some("def billing_gate".to_string()),
                    reason: "Billing boundary logic".to_string(),
                }],
            },
        ],
        dependencies: vec![],
        semantic_asts: vec![],
    };

    let graph = map_repository_architecture_with_policy(&repository, &policy).expect("map");
    let components = graph
        .nodes
        .values()
        .filter(|node| node.kind == NodeKind::Component)
        .map(|node| node.name.clone())
        .collect::<Vec<_>>();
    assert!(components.iter().any(|name| name == "auth_flow"));
    assert!(components.iter().any(|name| name == "billing_flow"));

    let bridge_code_units = graph
        .nodes
        .values()
        .filter(|node| node.kind == NodeKind::CodeUnit)
        .filter(|node| node.path.to_string_lossy() == "src/bridge.py")
        .count();
    assert_eq!(bridge_code_units, 2);
}
