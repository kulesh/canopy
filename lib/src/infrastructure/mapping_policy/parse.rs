use std::collections::BTreeSet;

use chrono::Utc;

use crate::error::{CanopyError, Result};
use crate::infrastructure::llm::ModelInfo;

use super::types::{C4MappingPolicy, PolicyResponse};
use super::util::normalize_path;

pub fn parse_policy_response(
    purpose: &str,
    raw: &str,
    model_info: Option<&ModelInfo>,
) -> Result<C4MappingPolicy> {
    let json = extract_json_object(raw).ok_or_else(|| {
        CanopyError::Validation("mapping policy response did not contain valid JSON".to_string())
    })?;
    let mut response: PolicyResponse = serde_json::from_str(&json)?;

    let mut seen = BTreeSet::new();
    for rule in &mut response.mappings {
        rule.file = normalize_path(&rule.file);
        if !seen.insert(rule.file.clone()) {
            return Err(CanopyError::Validation(format!(
                "mapping policy contains duplicate file entry: {}",
                rule.file
            )));
        }
        rule.confidence = rule.confidence.clamp(0.0, 1.0);
        if rule.rationale.trim().is_empty() {
            rule.rationale = "No rationale supplied".to_string();
        }
        if rule.include {
            let container_ok = rule
                .container
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            let component_ok = rule
                .component
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            if !container_ok || !component_ok {
                return Err(CanopyError::Validation(format!(
                    "include=true requires container/component for file {}",
                    rule.file
                )));
            }
            if let Some(component) = &rule.component {
                if is_placeholder_component_name(component) {
                    return Err(CanopyError::Validation(format!(
                        "component '{}' is too generic for file {}; use a behavior-based component name",
                        component, rule.file
                    )));
                }
            }
        }
    }

    let mut seen_contributions = BTreeSet::new();
    for contribution in &mut response.contributions {
        contribution.file = normalize_path(&contribution.file);
        contribution.container = normalize_path(&contribution.container).replace('/', "::");
        contribution.component = contribution.component.trim().to_string();
        contribution.confidence = contribution.confidence.clamp(0.0, 1.0);
        if contribution.rationale.trim().is_empty() {
            contribution.rationale = "No rationale supplied".to_string();
        }
        if contribution.container.is_empty() || contribution.component.is_empty() {
            return Err(CanopyError::Validation(format!(
                "contribution requires non-empty container/component for file {}",
                contribution.file
            )));
        }
        if is_placeholder_component_name(&contribution.component) {
            return Err(CanopyError::Validation(format!(
                "component '{}' is too generic for file {}; use behavior-based component naming",
                contribution.component, contribution.file
            )));
        }
        let contribution_key = format!(
            "{}::{}::{}",
            contribution.file,
            contribution.container,
            contribution.component.to_lowercase()
        );
        if !seen_contributions.insert(contribution_key) {
            return Err(CanopyError::Validation(format!(
                "duplicate semantic contribution for file {} component {}",
                contribution.file, contribution.component
            )));
        }
        if contribution.evidence.is_empty() {
            return Err(CanopyError::Validation(format!(
                "contribution for {} -> {} requires at least one evidence span",
                contribution.file, contribution.component
            )));
        }
        for evidence in &mut contribution.evidence {
            evidence.file = if evidence.file.trim().is_empty() {
                contribution.file.clone()
            } else {
                normalize_path(&evidence.file)
            };
            if evidence.start_line == 0
                || evidence.end_line == 0
                || evidence.end_line < evidence.start_line
            {
                return Err(CanopyError::Validation(format!(
                    "invalid evidence line span for {}: {}-{}",
                    evidence.file, evidence.start_line, evidence.end_line
                )));
            }
            if evidence.reason.trim().is_empty() {
                return Err(CanopyError::Validation(format!(
                    "evidence reason cannot be empty for contribution {} -> {}",
                    contribution.file, contribution.component
                )));
            }
            if let Some(excerpt) = &evidence.excerpt {
                if excerpt.trim().is_empty() {
                    evidence.excerpt = None;
                }
            }
        }
    }

    let mut seen_dependencies = BTreeSet::new();
    for dependency in &mut response.dependencies {
        dependency.from.container = normalize_path(&dependency.from.container).replace('/', "::");
        dependency.from.component = dependency.from.component.trim().to_string();
        dependency.to.container = normalize_path(&dependency.to.container).replace('/', "::");
        dependency.to.component = dependency.to.component.trim().to_string();
        dependency.confidence = dependency.confidence.clamp(0.0, 1.0);
        if dependency.rationale.trim().is_empty() {
            dependency.rationale = "No rationale supplied".to_string();
        }
        if dependency.from.container.is_empty()
            || dependency.from.component.is_empty()
            || dependency.to.container.is_empty()
            || dependency.to.component.is_empty()
        {
            return Err(CanopyError::Validation(
                "dependency requires non-empty from/to container and component".to_string(),
            ));
        }
        let key = format!(
            "{}::{}=>{}::{}",
            dependency.from.container.to_lowercase(),
            dependency.from.component.to_lowercase(),
            dependency.to.container.to_lowercase(),
            dependency.to.component.to_lowercase()
        );
        if !seen_dependencies.insert(key) {
            return Err(CanopyError::Validation(format!(
                "duplicate dependency edge {}::{} -> {}::{}",
                dependency.from.container,
                dependency.from.component,
                dependency.to.container,
                dependency.to.component
            )));
        }
    }

    let mut seen_ast_files = BTreeSet::new();
    for ast in &mut response.semantic_asts {
        ast.file = normalize_path(&ast.file);
        if !seen_ast_files.insert(ast.file.clone()) {
            return Err(CanopyError::Validation(format!(
                "semantic_asts contains duplicate file entry: {}",
                ast.file
            )));
        }
        ast.summary = ast.summary.trim().to_string();
        let mut seen_ast_nodes = BTreeSet::new();
        for node in &mut ast.nodes {
            node.kind = node.kind.trim().to_string();
            node.name = node.name.trim().to_string();
            node.summary = node.summary.trim().to_string();
            if node.kind.is_empty() || node.name.is_empty() || node.summary.is_empty() {
                return Err(CanopyError::Validation(format!(
                    "semantic AST node for {} has empty kind/name/summary",
                    ast.file
                )));
            }
            if node.start_line == 0 || node.end_line == 0 || node.end_line < node.start_line {
                return Err(CanopyError::Validation(format!(
                    "invalid semantic AST node span for {}: {}-{}",
                    ast.file, node.start_line, node.end_line
                )));
            }
            let node_key = (
                node.kind.to_lowercase(),
                node.name.to_lowercase(),
                node.start_line,
                node.end_line,
            );
            if !seen_ast_nodes.insert(node_key) {
                return Err(CanopyError::Validation(format!(
                    "semantic AST for {} contains duplicate node span {}:{}-{}",
                    ast.file, node.name, node.start_line, node.end_line
                )));
            }
        }
    }

    Ok(C4MappingPolicy {
        purpose: purpose.to_string(),
        generated_at: Utc::now(),
        provider: model_info.map(|m| m.provider.clone()),
        model: model_info.map(|m| m.model.clone()),
        notes: response.notes.take(),
        mappings: response.mappings,
        contributions: response.contributions,
        dependencies: response.dependencies,
        semantic_asts: response.semantic_asts,
    })
}

fn is_placeholder_component_name(name: &str) -> bool {
    let normalized = name.trim().to_lowercase().replace('\\', "/");
    matches!(
        normalized.as_str(),
        "__init__"
            | "__init__.py"
            | "init"
            | "mod"
            | "mod.rs"
            | "index"
            | "index.js"
            | "index.ts"
            | "index.tsx"
            | "index.jsx"
            | "lib"
            | "lib.rs"
    )
}

fn extract_json_object(raw: &str) -> Option<String> {
    let stripped = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    if stripped.starts_with('{') && stripped.ends_with('}') {
        return Some(stripped.to_string());
    }

    let start = stripped.find('{')?;
    let end = stripped.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(stripped[start..=end].to_string())
}
