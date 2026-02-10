use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentRef {
    pub container: String,
    pub component: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComponentDependency {
    pub from: ComponentRef,
    pub to: ComponentRef,
    pub confidence: f32,
    pub rationale: String,
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
    pub dependencies: Vec<ComponentDependency>,
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
pub(crate) struct PolicyResponse {
    #[serde(default)]
    pub(crate) mappings: Vec<FileMappingRule>,
    #[serde(default)]
    pub(crate) notes: Option<String>,
    #[serde(default)]
    pub(crate) contributions: Vec<ComponentContribution>,
    #[serde(default)]
    pub(crate) dependencies: Vec<ComponentDependency>,
    #[serde(default)]
    pub(crate) semantic_asts: Vec<FileSemanticAst>,
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
