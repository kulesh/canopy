use std::fs;

use canopy_lib::domain::{ArchitectureGraph, ArchitectureNode, NodeKind};
use canopy_lib::infrastructure::{merge_workspace_graphs, parse_lcov};
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
