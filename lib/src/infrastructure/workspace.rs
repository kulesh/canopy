use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::domain::{ArchitectureGraph, ArchitectureNode, NodeKind, WorkspaceConfig};
use crate::error::{CanopyError, Result};

pub fn load_workspace(path: &Path) -> Result<WorkspaceConfig> {
    let body = fs::read_to_string(path).map_err(|source| CanopyError::io(path, source))?;
    let config = toml::from_str::<WorkspaceConfig>(&body)?;
    Ok(config)
}

pub fn merge_workspace_graphs(graphs: &[(String, ArchitectureGraph)]) -> ArchitectureGraph {
    let workspace_root = ArchitectureNode::new(
        "system:workspace".to_string(),
        "workspace".to_string(),
        NodeKind::System,
        Path::new(".").to_path_buf(),
        None,
    );
    let mut out = ArchitectureGraph::new("system:workspace".to_string(), workspace_root);

    for (repo_name, graph) in graphs {
        let container_id = format!("container:workspace:{repo_name}");
        let mut container = ArchitectureNode::new(
            container_id.clone(),
            repo_name.clone(),
            NodeKind::Container,
            Path::new(repo_name).to_path_buf(),
            Some("system:workspace".to_string()),
        );
        container.summary = "Repository container in workspace".to_string();
        container.confidence = 1.0;
        out.add_node(container);

        let mut map: BTreeMap<String, String> = BTreeMap::new();
        for node in graph.nodes.values() {
            if node.id == graph.root_id {
                continue;
            }
            let new_id = format!("{}::{}", repo_name, node.id);
            map.insert(node.id.clone(), new_id);
        }

        for node in graph.nodes.values() {
            if node.id == graph.root_id {
                continue;
            }
            let mut clone = node.clone();
            clone.id = map
                .get(&node.id)
                .cloned()
                .unwrap_or_else(|| format!("{}::{}", repo_name, node.id));
            clone.parent_id = node
                .parent_id
                .as_ref()
                .and_then(|p| map.get(p).cloned())
                .or_else(|| Some(container_id.clone()));
            clone.children = node
                .children
                .iter()
                .filter_map(|c| map.get(c).cloned())
                .collect();
            clone.dependencies = node
                .dependencies
                .iter()
                .filter_map(|d| map.get(d).cloned())
                .collect();
            clone.dependents = node
                .dependents
                .iter()
                .filter_map(|d| map.get(d).cloned())
                .collect();
            out.add_node(clone);
        }
    }

    out.rebuild_dependents();
    out
}

pub fn upsert_workspace_repository_graph(
    graphs: &mut BTreeMap<String, ArchitectureGraph>,
    repository_name: impl Into<String>,
    graph: ArchitectureGraph,
) -> ArchitectureGraph {
    graphs.insert(repository_name.into(), graph);
    let ordered: Vec<(String, ArchitectureGraph)> = graphs
        .iter()
        .map(|(name, graph)| (name.clone(), graph.clone()))
        .collect();
    merge_workspace_graphs(&ordered)
}

pub fn merged_graph_for_active_repository(
    graphs: &BTreeMap<String, ArchitectureGraph>,
    active_repository: &str,
) -> ArchitectureGraph {
    if graphs.contains_key(active_repository) {
        let mut ordered = Vec::with_capacity(graphs.len());
        if let Some(graph) = graphs.get(active_repository) {
            ordered.push((active_repository.to_string(), graph.clone()));
        }
        for (name, graph) in graphs {
            if name == active_repository {
                continue;
            }
            ordered.push((name.clone(), graph.clone()));
        }
        return merge_workspace_graphs(&ordered);
    }

    let ordered: Vec<(String, ArchitectureGraph)> = graphs
        .iter()
        .map(|(name, graph)| (name.clone(), graph.clone()))
        .collect();
    merge_workspace_graphs(&ordered)
}
