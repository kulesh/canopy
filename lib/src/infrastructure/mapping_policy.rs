use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::domain::Repository;
use crate::error::{CanopyError, Result};
use crate::infrastructure::llm::ModelInfo;

const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "tsx", "jsx", "go", "java", "kt", "scala", "c", "cpp", "h", "hpp",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileMappingRule {
    pub file: String,
    pub include: bool,
    pub container: Option<String>,
    pub component: Option<String>,
    pub confidence: f32,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct C4MappingPolicy {
    pub purpose: String,
    pub generated_at: DateTime<Utc>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub notes: Option<String>,
    pub mappings: Vec<FileMappingRule>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceTreeSnapshot {
    pub repository_name: String,
    pub files: Vec<String>,
    pub directories: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PolicyResponse {
    mappings: Vec<FileMappingRule>,
    #[serde(default)]
    notes: Option<String>,
}

impl SourceTreeSnapshot {
    pub fn render_tree_preview(&self, max_files: usize) -> String {
        self.files
            .iter()
            .take(max_files)
            .map(|file| format!("- {file}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn collect_source_tree_snapshot(repository: &Repository) -> Result<SourceTreeSnapshot> {
    let mut walker = WalkBuilder::new(&repository.root);
    walker.hidden(false);
    walker.git_ignore(true);
    walker.git_exclude(true);
    walker.parents(true);

    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();

    for entry in walker.build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        let path = entry.path();
        if !is_source_file(path) {
            continue;
        }

        let rel = match path.strip_prefix(&repository.root) {
            Ok(rel) => normalize_path(&rel.to_string_lossy()),
            Err(_) => continue,
        };
        files.insert(rel.clone());

        let mut parent = Path::new(&rel).parent();
        while let Some(dir) = parent {
            if dir.as_os_str().is_empty() {
                break;
            }
            directories.insert(normalize_path(&dir.to_string_lossy()));
            parent = dir.parent();
        }
    }

    Ok(SourceTreeSnapshot {
        repository_name: repository.name.clone(),
        files: files.into_iter().collect(),
        directories: directories.into_iter().collect(),
    })
}

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
        }
    }

    Ok(C4MappingPolicy {
        purpose: purpose.to_string(),
        generated_at: Utc::now(),
        provider: model_info.map(|m| m.provider.clone()),
        model: model_info.map(|m| m.model.clone()),
        notes: response.notes.take(),
        mappings: response.mappings,
    })
}

pub fn validate_policy(snapshot: &SourceTreeSnapshot, policy: &C4MappingPolicy) -> Result<()> {
    let index = policy_index(policy);
    let mut missing = Vec::new();

    for file in &snapshot.files {
        if !index.contains_key(file) {
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

    Ok(())
}

pub fn policy_index(policy: &C4MappingPolicy) -> BTreeMap<String, &FileMappingRule> {
    let mut out = BTreeMap::new();
    for rule in &policy.mappings {
        out.insert(normalize_path(&rule.file), rule);
    }
    out
}

pub fn read_purpose_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| CanopyError::io(path, source))
}

pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string()
}

pub fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| SOURCE_EXTENSIONS.contains(&ext))
        .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_policy_json_response() {
        let raw = r#"
        {
          "mappings": [
            {
              "file": "src/main.rs",
              "include": true,
              "container": "src",
              "component": "main",
              "confidence": 0.91,
              "rationale": "entrypoint"
            }
          ]
        }
        "#;
        let policy = parse_policy_response("analyze auth flow", raw, None).expect("policy");
        assert_eq!(policy.mappings.len(), 1);
        assert_eq!(policy.mappings[0].file, "src/main.rs");
        assert!(policy.mappings[0].include);
    }

    #[test]
    fn rejects_missing_component_for_included_file() {
        let raw = r#"
        {
          "mappings": [
            {
              "file": "src/main.rs",
              "include": true,
              "container": "src",
              "component": "",
              "confidence": 0.9,
              "rationale": "entrypoint"
            }
          ]
        }
        "#;
        assert!(parse_policy_response("purpose", raw, None).is_err());
    }

    #[test]
    fn rejects_non_json_response() {
        let raw = "not valid json";
        assert!(parse_policy_response("purpose", raw, None).is_err());
    }

    #[test]
    fn validates_policy_file_coverage() {
        let snapshot = SourceTreeSnapshot {
            repository_name: "demo".to_string(),
            files: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
            directories: vec!["src".to_string()],
        };
        let policy = C4MappingPolicy {
            purpose: "test".to_string(),
            generated_at: Utc::now(),
            provider: None,
            model: None,
            notes: None,
            mappings: vec![FileMappingRule {
                file: "src/main.rs".to_string(),
                include: true,
                container: Some("src".to_string()),
                component: Some("main".to_string()),
                confidence: 0.9,
                rationale: "entrypoint".to_string(),
            }],
        };
        assert!(validate_policy(&snapshot, &policy).is_err());
    }
}
