use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{CanopyError, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeKind {
    System,
    Container,
    Component,
    CodeUnit,
}

impl NodeKind {
    pub fn rank(self) -> usize {
        match self {
            Self::System => 0,
            Self::Container => 1,
            Self::Component => 2,
            Self::CodeUnit => 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provenance {
    pub source: ProvenanceSource,
    pub author: Option<String>,
    pub reason: Option<String>,
    pub edited_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceSource {
    Ai,
    Human,
}

impl Provenance {
    pub fn ai() -> Self {
        Self {
            source: ProvenanceSource::Ai,
            author: None,
            reason: None,
            edited_at: None,
        }
    }

    pub fn human(author: Option<String>, reason: Option<String>) -> Self {
        Self {
            source: ProvenanceSource::Human,
            author,
            reason,
            edited_at: Some(Utc::now()),
        }
    }

    pub fn is_human(self) -> bool {
        self.source == ProvenanceSource::Human
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchitectureNode {
    pub id: String,
    pub name: String,
    pub kind: NodeKind,
    pub path: PathBuf,
    pub parent_id: Option<String>,
    pub children: Vec<String>,
    pub dependencies: Vec<String>,
    pub dependents: Vec<String>,
    pub summary: String,
    pub confidence: f32,
    pub provenance: Provenance,
    pub last_analyzed: Option<DateTime<Utc>>,
}

impl ArchitectureNode {
    pub fn new(
        id: String,
        name: String,
        kind: NodeKind,
        path: PathBuf,
        parent_id: Option<String>,
    ) -> Self {
        Self {
            id,
            name,
            kind,
            path,
            parent_id,
            children: Vec::new(),
            dependencies: Vec::new(),
            dependents: Vec::new(),
            summary: String::new(),
            confidence: 0.0,
            provenance: Provenance::ai(),
            last_analyzed: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchitectureGraph {
    pub root_id: String,
    pub nodes: BTreeMap<String, ArchitectureNode>,
}

impl ArchitectureGraph {
    pub fn new(root_id: String, root: ArchitectureNode) -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(root_id.clone(), root);
        Self { root_id, nodes }
    }

    pub fn node(&self, id: &str) -> Option<&ArchitectureNode> {
        self.nodes.get(id)
    }

    pub fn node_mut(&mut self, id: &str) -> Option<&mut ArchitectureNode> {
        self.nodes.get_mut(id)
    }

    pub fn add_node(&mut self, node: ArchitectureNode) {
        let id = node.id.clone();
        if let Some(parent_id) = &node.parent_id {
            if let Some(parent) = self.nodes.get_mut(parent_id) {
                if !parent.children.contains(&id) {
                    parent.children.push(id.clone());
                }
            }
        }
        self.nodes.insert(id, node);
    }

    pub fn lineage(&self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut current = self.node(id).map(|n| n.id.clone());
        while let Some(node_id) = current {
            out.push(node_id.clone());
            current = self.node(&node_id).and_then(|n| n.parent_id.clone());
        }
        out.reverse();
        out
    }

    pub fn validate(&self) -> Result<()> {
        if !self.nodes.contains_key(&self.root_id) {
            return Err(CanopyError::Validation("root node missing".to_string()));
        }

        let mut seen = BTreeSet::new();
        for (id, node) in &self.nodes {
            if node.id != *id {
                return Err(CanopyError::Validation(format!(
                    "node key/id mismatch for {id}"
                )));
            }
            if let Some(parent_id) = &node.parent_id {
                if !self.nodes.contains_key(parent_id) {
                    return Err(CanopyError::Validation(format!(
                        "node {id} references missing parent {parent_id}"
                    )));
                }
                let parent_rank = self
                    .nodes
                    .get(parent_id)
                    .map(|n| n.kind.rank())
                    .unwrap_or_default();
                if node.kind.rank() < parent_rank {
                    return Err(CanopyError::Validation(format!(
                        "node {id} has invalid rank under {parent_id}"
                    )));
                }
            }
            for dep in &node.dependencies {
                if !self.nodes.contains_key(dep) {
                    return Err(CanopyError::Validation(format!(
                        "node {id} references missing dependency {dep}"
                    )));
                }
            }
            if !seen.insert(id) {
                return Err(CanopyError::Validation(format!(
                    "duplicate node id discovered {id}"
                )));
            }
        }

        Ok(())
    }

    pub fn rebuild_dependents(&mut self) {
        for node in self.nodes.values_mut() {
            node.dependents.clear();
        }

        let ids: Vec<String> = self.nodes.keys().cloned().collect();
        for id in ids {
            let deps = self
                .nodes
                .get(&id)
                .map(|n| n.dependencies.clone())
                .unwrap_or_default();
            for dep in deps {
                if let Some(node) = self.nodes.get_mut(&dep) {
                    if !node.dependents.contains(&id) {
                        node.dependents.push(id.clone());
                    }
                }
            }
        }
    }

    pub fn impact_set(&self, id: &str, max_depth: usize) -> BTreeSet<String> {
        let mut visited = BTreeSet::new();
        if max_depth == 0 {
            return visited;
        }

        let mut frontier = vec![(id.to_string(), 0usize)];
        while let Some((next, depth)) = frontier.pop() {
            if depth >= max_depth {
                continue;
            }
            if let Some(node) = self.node(&next) {
                for dependent in &node.dependents {
                    if visited.insert(dependent.clone()) {
                        frontier.push((dependent.clone(), depth + 1));
                    }
                }
            }
        }

        visited
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_graph() -> ArchitectureGraph {
        let root_id = "system:test".to_string();
        let root = ArchitectureNode::new(
            root_id.clone(),
            "test".to_string(),
            NodeKind::System,
            PathBuf::from("."),
            None,
        );
        ArchitectureGraph::new(root_id, root)
    }

    #[test]
    fn validate_rejects_missing_root_node() {
        let mut graph = sample_graph();
        graph.root_id = "system:missing".to_string();
        let err = graph.validate().expect_err("root validation should fail");
        assert!(err.to_string().contains("root node missing"));
    }

    #[test]
    fn validate_rejects_missing_dependency_node() {
        let mut graph = sample_graph();
        let mut component = ArchitectureNode::new(
            "component:app:api".to_string(),
            "api".to_string(),
            NodeKind::Component,
            PathBuf::from("src/api.rs"),
            Some("system:test".to_string()),
        );
        component
            .dependencies
            .push("component:app:missing".to_string());
        graph.add_node(component);

        let err = graph
            .validate()
            .expect_err("dependency validation should fail");
        assert!(err.to_string().contains("missing dependency"));
    }

    #[test]
    fn rebuild_dependents_is_deterministic() {
        let mut graph = sample_graph();
        let mut upstream = ArchitectureNode::new(
            "component:app:upstream".to_string(),
            "upstream".to_string(),
            NodeKind::Component,
            PathBuf::from("src/upstream.rs"),
            Some("system:test".to_string()),
        );
        upstream
            .dependencies
            .push("component:app:downstream".to_string());
        let downstream = ArchitectureNode::new(
            "component:app:downstream".to_string(),
            "downstream".to_string(),
            NodeKind::Component,
            PathBuf::from("src/downstream.rs"),
            Some("system:test".to_string()),
        );
        graph.add_node(upstream);
        graph.add_node(downstream);

        graph.rebuild_dependents();
        let first = graph
            .node("component:app:downstream")
            .map(|node| node.dependents.clone())
            .expect("downstream node");
        graph.rebuild_dependents();
        let second = graph
            .node("component:app:downstream")
            .map(|node| node.dependents.clone())
            .expect("downstream node");

        assert_eq!(first, second);
        assert_eq!(second, vec!["component:app:upstream".to_string()]);
    }

    #[test]
    fn impact_set_respects_depth_budget() {
        let mut graph = sample_graph();

        let mut component_a = ArchitectureNode::new(
            "component:app:a".to_string(),
            "a".to_string(),
            NodeKind::Component,
            PathBuf::from("src/a.rs"),
            Some("system:test".to_string()),
        );
        let mut component_b = ArchitectureNode::new(
            "component:app:b".to_string(),
            "b".to_string(),
            NodeKind::Component,
            PathBuf::from("src/b.rs"),
            Some("system:test".to_string()),
        );
        let component_c = ArchitectureNode::new(
            "component:app:c".to_string(),
            "c".to_string(),
            NodeKind::Component,
            PathBuf::from("src/c.rs"),
            Some("system:test".to_string()),
        );

        component_a.dependencies.push("component:app:b".to_string());
        component_b.dependencies.push("component:app:c".to_string());

        graph.add_node(component_a);
        graph.add_node(component_b);
        graph.add_node(component_c);
        graph.rebuild_dependents();

        assert!(graph.impact_set("component:app:c", 0).is_empty());

        let depth_one = graph.impact_set("component:app:c", 1);
        assert!(depth_one.contains("component:app:b"));
        assert!(!depth_one.contains("component:app:a"));

        let depth_two = graph.impact_set("component:app:c", 2);
        assert!(depth_two.contains("component:app:b"));
        assert!(depth_two.contains("component:app:a"));
    }
}
