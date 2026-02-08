use std::collections::BTreeSet;

use chrono::Utc;
use regex::Regex;

use crate::domain::{ArchitectureGraph, QueryAnswer, QueryReference};

pub fn answer_query(graph: &ArchitectureGraph, query: &str) -> QueryAnswer {
    let mut references = mentions(graph, query);

    if references.is_empty() {
        let lower = query.to_lowercase();
        for node in graph.nodes.values() {
            if node.name.to_lowercase().contains(&lower)
                || node.summary.to_lowercase().contains(&lower)
                || node.path.to_string_lossy().to_lowercase().contains(&lower)
            {
                references.push(QueryReference {
                    node_id: node.id.clone(),
                    label: node.name.clone(),
                });
            }
            if references.len() >= 8 {
                break;
            }
        }
    }

    let response = if references.is_empty() {
        "No direct match found. Refine the query or reference a component with @name.".to_string()
    } else {
        let labels = references
            .iter()
            .map(|r| r.label.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!("Relevant architectural elements: {labels}")
    };

    QueryAnswer {
        query: query.to_string(),
        response,
        references,
        created_at: Utc::now(),
    }
}

pub fn mentions(graph: &ArchitectureGraph, query: &str) -> Vec<QueryReference> {
    let mention_re = Regex::new(r"@([A-Za-z0-9_\-\.:/]+)").expect("mention regex is valid");
    let mut refs = Vec::new();
    let mut seen = BTreeSet::new();

    for capture in mention_re.captures_iter(query) {
        let Some(raw) = capture.get(1) else {
            continue;
        };
        let token = raw.as_str().to_lowercase();

        for node in graph.nodes.values() {
            let matches = node.id.to_lowercase().contains(&token)
                || node.name.to_lowercase() == token
                || node.path.to_string_lossy().to_lowercase().contains(&token);
            if matches && seen.insert(node.id.clone()) {
                refs.push(QueryReference {
                    node_id: node.id.clone(),
                    label: node.name.clone(),
                });
            }
        }
    }

    refs
}
