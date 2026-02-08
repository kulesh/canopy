use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::domain::Repository;
use crate::error::{CanopyError, Result};
use crate::infrastructure::llm::ModelInfo;

const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "py", "pyi", "js", "mjs", "cjs", "ts", "tsx", "jsx", "go", "java", "kt", "scala", "c",
    "cpp", "h", "hpp", "cc", "hh", "cs", "swift", "rb", "php", "lua", "sql", "sh", "bash", "zsh",
    "fish", "ps1",
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
pub struct EvidenceSpan {
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(default)]
    pub excerpt: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComponentContribution {
    pub file: String,
    pub container: String,
    pub component: String,
    pub confidence: f32,
    pub rationale: String,
    #[serde(default)]
    pub evidence: Vec<EvidenceSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticAstNode {
    pub kind: String,
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileSemanticAst {
    pub file: String,
    #[serde(default)]
    pub language: Option<String>,
    pub summary: String,
    #[serde(default)]
    pub nodes: Vec<SemanticAstNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct C4MappingPolicy {
    pub purpose: String,
    pub generated_at: DateTime<Utc>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub notes: Option<String>,
    #[serde(default)]
    pub mappings: Vec<FileMappingRule>,
    #[serde(default)]
    pub contributions: Vec<ComponentContribution>,
    #[serde(default)]
    pub semantic_asts: Vec<FileSemanticAst>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceTreeSnapshot {
    pub repository_name: String,
    pub files: Vec<String>,
    pub directories: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PolicyResponse {
    #[serde(default)]
    mappings: Vec<FileMappingRule>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    contributions: Vec<ComponentContribution>,
    #[serde(default)]
    semantic_asts: Vec<FileSemanticAst>,
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

    pub fn render_directory_preview(&self, max_directories: usize) -> String {
        self.directories
            .iter()
            .take(max_directories)
            .map(|directory| format!("- {directory}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn collect_source_tree_snapshot(repository: &Repository) -> Result<SourceTreeSnapshot> {
    let mut walker = WalkBuilder::new(&repository.root);
    walker.hidden(true);
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
        semantic_asts: response.semantic_asts,
    })
}

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

pub fn read_purpose_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| CanopyError::io(path, source))
}

pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string()
}

pub fn is_source_file(path: &Path) -> bool {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| SOURCE_EXTENSIONS.contains(&ext))
        .unwrap_or(false)
    {
        return true;
    }

    is_shebang_script(path)
}

fn is_shebang_script(path: &Path) -> bool {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return false,
    };
    if !metadata.is_file() || metadata.len() > 1_048_576 {
        return false;
    }

    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut buf = [0u8; 256];
    let read = match file.read(&mut buf) {
        Ok(read) => read,
        Err(_) => return false,
    };
    if read < 3 || &buf[..2] != b"#!" {
        return false;
    }

    let head = String::from_utf8_lossy(&buf[..read]).to_lowercase();
    [
        "python", "bash", "sh", "zsh", "fish", "node", "ruby", "perl", "php", "pwsh",
    ]
    .iter()
    .any(|interpreter| head.contains(interpreter))
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

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
    fn rejects_placeholder_component_name_for_included_file() {
        let raw = r#"
        {
          "mappings": [
            {
              "file": "pkg/__init__.py",
              "include": true,
              "container": "pkg",
              "component": "__init__",
              "confidence": 0.91,
              "rationale": "module init"
            }
          ]
        }
        "#;
        let err = parse_policy_response("purpose", raw, None).expect_err("must reject");
        let message = err.to_string();
        assert!(message.contains("too generic"));
    }

    #[test]
    fn rejects_non_json_response() {
        let raw = "not valid json";
        assert!(parse_policy_response("purpose", raw, None).is_err());
    }

    #[test]
    fn rejects_duplicate_contributions_for_same_file_component() {
        let raw = r#"
        {
          "mappings": [
            {
              "file": "src/main.rs",
              "include": true,
              "container": "src",
              "component": "entrypoint",
              "confidence": 0.9,
              "rationale": "entrypoint"
            }
          ],
          "contributions": [
            {
              "file": "src/main.rs",
              "container": "src",
              "component": "entrypoint",
              "confidence": 0.9,
              "rationale": "main behavior",
              "evidence": [{"file":"src/main.rs","start_line":1,"end_line":1,"reason":"main"}]
            },
            {
              "file": "src/main.rs",
              "container": "src",
              "component": "entrypoint",
              "confidence": 0.9,
              "rationale": "duplicate",
              "evidence": [{"file":"src/main.rs","start_line":1,"end_line":1,"reason":"main"}]
            }
          ]
        }
        "#;
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
            contributions: vec![],
            semantic_asts: vec![],
        };
        assert!(validate_policy(&snapshot, &policy).is_err());
    }

    #[test]
    fn validate_policy_rejects_multi_container_file_contributions() {
        let snapshot = SourceTreeSnapshot {
            repository_name: "demo".to_string(),
            files: vec!["src/main.rs".to_string()],
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
                component: Some("entrypoint".to_string()),
                confidence: 0.9,
                rationale: "entrypoint".to_string(),
            }],
            contributions: vec![
                ComponentContribution {
                    file: "src/main.rs".to_string(),
                    container: "src".to_string(),
                    component: "entrypoint".to_string(),
                    confidence: 0.9,
                    rationale: "entrypoint behavior".to_string(),
                    evidence: vec![EvidenceSpan {
                        file: "src/main.rs".to_string(),
                        start_line: 1,
                        end_line: 1,
                        excerpt: None,
                        reason: "entrypoint".to_string(),
                    }],
                },
                ComponentContribution {
                    file: "src/main.rs".to_string(),
                    container: "other".to_string(),
                    component: "audit".to_string(),
                    confidence: 0.8,
                    rationale: "cross-cutting".to_string(),
                    evidence: vec![EvidenceSpan {
                        file: "src/main.rs".to_string(),
                        start_line: 1,
                        end_line: 1,
                        excerpt: None,
                        reason: "audit".to_string(),
                    }],
                },
            ],
            semantic_asts: vec![],
        };
        assert!(validate_policy(&snapshot, &policy).is_err());
    }

    #[test]
    fn validate_policy_rejects_semantic_ast_unknown_file() {
        let snapshot = SourceTreeSnapshot {
            repository_name: "demo".to_string(),
            files: vec!["src/main.rs".to_string()],
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
                include: false,
                container: None,
                component: None,
                confidence: 0.1,
                rationale: "ignored".to_string(),
            }],
            contributions: vec![],
            semantic_asts: vec![FileSemanticAst {
                file: "src/other.rs".to_string(),
                language: Some("rust".to_string()),
                summary: "other".to_string(),
                nodes: vec![],
            }],
        };
        assert!(validate_policy(&snapshot, &policy).is_err());
    }

    #[test]
    fn strict_evidence_validation_requires_included_file_contributions() {
        let dir = TempDir::new().expect("temp");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").expect("main");

        let snapshot = SourceTreeSnapshot {
            repository_name: "demo".to_string(),
            files: vec!["src/main.rs".to_string()],
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
            contributions: vec![],
            semantic_asts: vec![],
        };

        assert!(validate_policy_evidence(dir.path(), &snapshot, &policy).is_err());
    }

    #[test]
    fn strict_evidence_validation_checks_line_spans() {
        let dir = TempDir::new().expect("temp");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(
            dir.path().join("src/main.rs"),
            "fn main() {\n    println!(\"hi\");\n}\n",
        )
        .expect("main");

        let snapshot = SourceTreeSnapshot {
            repository_name: "demo".to_string(),
            files: vec!["src/main.rs".to_string()],
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
            contributions: vec![ComponentContribution {
                file: "src/main.rs".to_string(),
                container: "src".to_string(),
                component: "entrypoint".to_string(),
                confidence: 0.9,
                rationale: "entrypoint behavior".to_string(),
                evidence: vec![EvidenceSpan {
                    file: "src/main.rs".to_string(),
                    start_line: 1,
                    end_line: 2,
                    excerpt: Some("fn main()".to_string()),
                    reason: "startup function".to_string(),
                }],
            }],
            semantic_asts: vec![],
        };

        assert!(validate_policy_evidence(dir.path(), &snapshot, &policy).is_ok());
    }

    #[test]
    fn is_source_file_accepts_extensionless_shebang_scripts() {
        let dir = TempDir::new().expect("temp");
        let script = dir.path().join("run_tool");
        fs::write(&script, "#!/usr/bin/env python3\nprint('ok')\n").expect("script");
        assert!(is_source_file(&script));
    }

    #[test]
    fn is_source_file_rejects_non_text_extensionless_files() {
        let dir = TempDir::new().expect("temp");
        let file = dir.path().join("blob");
        fs::write(&file, [0_u8, 159_u8, 32_u8, 240_u8]).expect("blob");
        assert!(!is_source_file(&file));
    }
}
