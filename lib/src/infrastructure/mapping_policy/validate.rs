use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{CanopyError, Result};

use super::types::{C4MappingPolicy, ComponentContribution, FileMappingRule, SourceTreeSnapshot};
use super::util::normalize_path;

pub fn validate_policy(snapshot: &SourceTreeSnapshot, policy: &C4MappingPolicy) -> Result<()> {
    let index = policy_index(policy);
    let contributions = contributions_by_file(policy);
    let mut missing = Vec::new();

    for file in &snapshot.files {
        let mapping = index.get(file);
        let has_contributions = contributions.contains_key(file);
        if has_contributions {
            if mapping.map(|rule| !rule.include).unwrap_or(false) {
                return Err(CanopyError::Validation(format!(
                    "file {file} has include=false mapping but also has semantic contributions"
                )));
            }
            continue;
        }

        if mapping.is_none() {
            missing.push(file.clone());
        }
    }

    if !missing.is_empty() {
        return Err(CanopyError::Validation(format!(
            "mapping policy missing {} source file(s): {}",
            missing.len(),
            missing
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    for contribution in &policy.contributions {
        if !snapshot.files.contains(&contribution.file) {
            return Err(CanopyError::Validation(format!(
                "contribution references unknown file: {}",
                contribution.file
            )));
        }
        for evidence in &contribution.evidence {
            if !snapshot.files.contains(&evidence.file) {
                return Err(CanopyError::Validation(format!(
                    "evidence references unknown file: {}",
                    evidence.file
                )));
            }
        }
    }

    let known_components = policy
        .contributions
        .iter()
        .map(|contribution| {
            (
                normalize_path(&contribution.container).replace('/', "::"),
                contribution.component.trim().to_lowercase(),
            )
        })
        .collect::<BTreeSet<(String, String)>>();

    for dependency in &policy.dependencies {
        let from = (
            normalize_path(&dependency.from.container).replace('/', "::"),
            dependency.from.component.trim().to_lowercase(),
        );
        let to = (
            normalize_path(&dependency.to.container).replace('/', "::"),
            dependency.to.component.trim().to_lowercase(),
        );
        if !known_components.contains(&from) {
            return Err(CanopyError::Validation(format!(
                "dependency source {}::{} does not exist in contributions",
                dependency.from.container, dependency.from.component
            )));
        }
        if !known_components.contains(&to) {
            return Err(CanopyError::Validation(format!(
                "dependency target {}::{} does not exist in contributions",
                dependency.to.container, dependency.to.component
            )));
        }
    }

    for ast in &policy.semantic_asts {
        if !snapshot.files.contains(&ast.file) {
            return Err(CanopyError::Validation(format!(
                "semantic_asts references unknown file: {}",
                ast.file
            )));
        }
    }

    for file in &snapshot.files {
        let file_contributions = contributions.get(file).cloned().unwrap_or_default();
        if file_contributions.is_empty() {
            continue;
        }

        let mut container_set = BTreeSet::new();
        for contribution in file_contributions {
            container_set.insert(normalize_path(&contribution.container).replace('/', "::"));
        }
        if container_set.len() > 1 {
            return Err(CanopyError::Validation(format!(
                "file {file} contributes to multiple containers: {}",
                container_set.into_iter().collect::<Vec<_>>().join(", ")
            )));
        }

        if let Some(rule) = index.get(file) {
            if rule.include {
                let rule_container = rule
                    .container
                    .as_ref()
                    .map(|v| normalize_path(v).replace('/', "::"))
                    .unwrap_or_default();
                if let Some(contribution_container) = container_set.into_iter().next() {
                    if !rule_container.is_empty() && rule_container != contribution_container {
                        return Err(CanopyError::Validation(format!(
                            "file {file} mapping container '{}' conflicts with contribution container '{}'",
                            rule_container, contribution_container
                        )));
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn policy_index(policy: &C4MappingPolicy) -> BTreeMap<String, &FileMappingRule> {
    let mut out = BTreeMap::new();
    for rule in &policy.mappings {
        out.insert(normalize_path(&rule.file), rule);
    }
    out
}

pub fn contributions_by_file(
    policy: &C4MappingPolicy,
) -> BTreeMap<String, Vec<&ComponentContribution>> {
    let mut out: BTreeMap<String, Vec<&ComponentContribution>> = BTreeMap::new();
    for contribution in &policy.contributions {
        out.entry(normalize_path(&contribution.file))
            .or_default()
            .push(contribution);
    }
    out
}

pub fn validate_policy_evidence(
    repository_root: &Path,
    snapshot: &SourceTreeSnapshot,
    policy: &C4MappingPolicy,
) -> Result<()> {
    validate_policy(snapshot, policy)?;
    let contributions = contributions_by_file(policy);
    let mappings = policy_index(policy);
    let mut file_lines_cache: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for file in &snapshot.files {
        let has_contributions = contributions.contains_key(file);
        let is_excluded = mappings
            .get(file)
            .map(|rule| !rule.include)
            .unwrap_or(false);
        if !is_excluded && !has_contributions {
            return Err(CanopyError::Validation(format!(
                "included file {file} has no semantic contributions; content evidence is required"
            )));
        }
    }

    for contribution in &policy.contributions {
        let contribution_file = normalize_path(&contribution.file);
        let mut has_primary_evidence = false;
        for evidence in &contribution.evidence {
            let evidence_file = normalize_path(&evidence.file);
            if evidence_file == contribution_file {
                has_primary_evidence = true;
            }
            let lines = load_file_lines(repository_root, &evidence_file, &mut file_lines_cache)?;
            if evidence.start_line > lines.len() || evidence.end_line > lines.len() {
                return Err(CanopyError::Validation(format!(
                    "evidence span {}:{}-{} is outside file length {}",
                    evidence_file,
                    evidence.start_line,
                    evidence.end_line,
                    lines.len()
                )));
            }
            if let Some(excerpt) = &evidence.excerpt {
                let span = lines[evidence.start_line - 1..evidence.end_line].join("\n");
                if !span.contains(excerpt.trim()) {
                    return Err(CanopyError::Validation(format!(
                        "evidence excerpt mismatch for {}:{}-{}",
                        evidence_file, evidence.start_line, evidence.end_line
                    )));
                }
            }
        }
        if !has_primary_evidence {
            return Err(CanopyError::Validation(format!(
                "contribution {} -> {} must include evidence in the same file",
                contribution.file, contribution.component
            )));
        }
    }

    Ok(())
}

fn load_file_lines(
    repository_root: &Path,
    relative_file: &str,
    cache: &mut BTreeMap<String, Vec<String>>,
) -> Result<Vec<String>> {
    if let Some(lines) = cache.get(relative_file) {
        return Ok(lines.clone());
    }
    let full_path = repository_root.join(PathBuf::from(relative_file));
    let contents =
        fs::read_to_string(&full_path).map_err(|source| CanopyError::io(&full_path, source))?;
    let lines = contents
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    cache.insert(relative_file.to_string(), lines.clone());
    Ok(lines)
}
