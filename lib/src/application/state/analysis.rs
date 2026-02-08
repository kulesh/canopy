use std::collections::BTreeMap;
use std::path::Path;

use crate::domain::{ArchitectureGraph, EditLogEntry, NodeKind};
use crate::error::{CanopyError, Result};

pub(crate) fn detect_cycles(graph: &ArchitectureGraph) -> Vec<Vec<String>> {
    use petgraph::algo::tarjan_scc;
    use petgraph::graphmap::DiGraphMap;

    let mut g = DiGraphMap::<&str, ()>::new();
    for node in graph.nodes.values() {
        g.add_node(&node.id);
        for dep in &node.dependencies {
            g.add_edge(&node.id, dep, ());
        }
    }

    tarjan_scc(&g)
        .into_iter()
        .filter(|component| component.len() > 1)
        .map(|component| component.into_iter().map(ToString::to_string).collect())
        .collect()
}

pub fn render_edit_history(
    edits: &[EditLogEntry],
    selected: Option<&str>,
) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let selected = selected.unwrap_or_default();
    for edit in edits {
        if !selected.is_empty() && edit.component_path != selected {
            continue;
        }
        out.entry(edit.component_path.clone())
            .or_default()
            .push(format!(
                "{} | {} -> {}",
                edit.timestamp.format("%Y-%m-%d %H:%M"),
                edit.before,
                edit.after
            ));
    }
    out
}

pub fn node_code_path(
    repository_root: &Path,
    graph: &ArchitectureGraph,
    node_id: &str,
) -> Option<std::path::PathBuf> {
    let node = graph.node(node_id)?;
    match node.kind {
        NodeKind::CodeUnit => Some(repository_root.join(&node.path)),
        NodeKind::Component => {
            let code_child = node
                .children
                .iter()
                .find_map(|child| graph.node(child).filter(|n| n.kind == NodeKind::CodeUnit));
            code_child.map(|n| repository_root.join(&n.path))
        }
        _ => None,
    }
}

pub fn require_graph(graph: &ArchitectureGraph) -> Result<()> {
    graph.validate().map_err(|err| match err {
        CanopyError::Validation(msg) => {
            CanopyError::Validation(format!("graph validation failed: {msg}"))
        }
        other => other,
    })
}
