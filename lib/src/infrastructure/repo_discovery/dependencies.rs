use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;

use crate::domain::{ArchitectureGraph, NodeKind, Repository};
use crate::error::{CanopyError, Result};

use super::io::read_source_file;

pub(super) fn infer_dependencies(
    graph: &mut ArchitectureGraph,
    repository: &Repository,
    names: &BTreeMap<String, Vec<String>>,
    aliases: &BTreeMap<String, Vec<String>>,
) -> Result<()> {
    let mut edge_map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let import_re = Regex::new(r"(?i)\b(use|import|from|require|mod)\b\s+([A-Za-z0-9_:\./-]+)")
        .map_err(|err| CanopyError::Validation(format!("invalid import regex: {err}")))?;

    let component_ids: Vec<String> = graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Component)
        .map(|n| n.id.clone())
        .collect();

    for component_id in &component_ids {
        let mut deps = BTreeSet::new();
        let code_child_ids: Vec<String> = graph
            .node(component_id)
            .map(|component| {
                component
                    .children
                    .iter()
                    .filter_map(|child| {
                        graph
                            .node(child)
                            .filter(|node| node.kind == NodeKind::CodeUnit)
                            .map(|node| node.id.clone())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        for code_child_id in code_child_ids {
            let Some(code_node) = graph.node(&code_child_id) else {
                continue;
            };
            let full_path = repository.root.join(&code_node.path);
            let contents = read_source_file(&full_path)?;

            for capture in import_re.captures_iter(&contents) {
                let token = capture
                    .get(2)
                    .map(|m| normalize_token(m.as_str()))
                    .unwrap_or_default();

                if let Some(targets) = aliases.get(&token) {
                    for target in targets {
                        if target != component_id {
                            deps.insert(target.clone());
                        }
                    }
                }

                for segment in token
                    .split(['.', ':', '/', '\\'])
                    .filter(|seg| !seg.is_empty())
                {
                    if segment.len() < 3 {
                        continue;
                    }
                    if let Some(targets) = names.get(segment) {
                        for target in targets {
                            if target != component_id {
                                deps.insert(target.clone());
                            }
                        }
                    }
                }
            }
        }

        edge_map.insert(component_id.clone(), deps);
    }

    for (id, deps) in edge_map {
        if let Some(node) = graph.node_mut(&id) {
            node.dependencies = deps.into_iter().collect();
        }
    }

    Ok(())
}

fn normalize_token(token: &str) -> String {
    token
        .trim_matches(|c: char| {
            !c.is_ascii_alphanumeric() && !matches!(c, '_' | '.' | '/' | ':' | '-')
        })
        .to_lowercase()
}
