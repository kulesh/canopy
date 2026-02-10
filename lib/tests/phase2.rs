use std::fs;

use canopy_lib::domain::{ArchitectureGraph, ArchitectureNode, NodeKind};
use canopy_lib::infrastructure::{
    merge_workspace_graphs, merged_graph_for_active_repository, parse_lcov,
    upsert_workspace_repository_graph,
};
use tempfile::TempDir;

#[test]
fn parses_lcov_coverage() {
    let dir = TempDir::new().expect("temp");
    let path = dir.path().join("lcov.info");
    fs::write(
        &path,
        "SF:src/main.rs\nDA:1,1\nDA:2,0\nDA:3,1\nend_of_record\n",
    )
    .expect("write");

    let coverage = parse_lcov(&path).expect("parse");
    let stats = coverage.get("src/main.rs").expect("stats");
    assert_eq!(stats.total_lines, 3);
    assert_eq!(stats.hit_lines, 2);
}

#[test]
fn merges_workspace_graphs() {
    let mut graph = ArchitectureGraph::new(
        "system:a".to_string(),
        ArchitectureNode::new(
            "system:a".to_string(),
            "a".to_string(),
            NodeKind::System,
            ".".into(),
            None,
        ),
    );
    let component = ArchitectureNode::new(
        "component:a".to_string(),
        "Auth".to_string(),
        NodeKind::Component,
        "src/auth.rs".into(),
        Some("system:a".to_string()),
    );
    graph.add_node(component);

    let merged = merge_workspace_graphs(&[("repo-a".to_string(), graph)]);
    assert!(merged.nodes.contains_key("system:workspace"));
    assert!(merged
        .nodes
        .keys()
        .any(|id| id.starts_with("repo-a::component:a")));
}

#[test]
fn workspace_graph_upsert_and_active_priority_work() {
    let mut graphs = std::collections::BTreeMap::new();
    let mut graph_a = ArchitectureGraph::new(
        "system:a".to_string(),
        ArchitectureNode::new(
            "system:a".to_string(),
            "a".to_string(),
            NodeKind::System,
            ".".into(),
            None,
        ),
    );
    graph_a.add_node(ArchitectureNode::new(
        "component:a".to_string(),
        "Auth".to_string(),
        NodeKind::Component,
        "src/auth.rs".into(),
        Some("system:a".to_string()),
    ));

    let graph_b = ArchitectureGraph::new(
        "system:b".to_string(),
        ArchitectureNode::new(
            "system:b".to_string(),
            "b".to_string(),
            NodeKind::System,
            ".".into(),
            None,
        ),
    );

    let merged_a = upsert_workspace_repository_graph(&mut graphs, "repo-a", graph_a);
    assert!(merged_a
        .nodes
        .keys()
        .any(|id| id.starts_with("repo-a::component:a")));

    let merged_b = upsert_workspace_repository_graph(&mut graphs, "repo-b", graph_b);
    assert!(merged_b
        .nodes
        .keys()
        .any(|id| id.starts_with("repo-a::component:a")));
    assert!(merged_b
        .nodes
        .keys()
        .any(|id| id.starts_with("container:workspace:repo-b")));

    let active_first = merged_graph_for_active_repository(&graphs, "repo-b");
    let root_children = active_first
        .node("system:workspace")
        .map(|node| node.children.clone())
        .expect("workspace root");
    assert_eq!(
        root_children.first().map(String::as_str),
        Some("container:workspace:repo-b")
    );
}
