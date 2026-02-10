mod parse;
mod snapshot;
mod types;
mod util;
mod validate;

pub use parse::parse_policy_response;
pub use snapshot::{
    collect_source_tree_snapshot, is_first_pass_excluded_path, is_first_pass_scope_file,
    is_source_file,
};
pub use types::{
    C4MappingPolicy, ComponentContribution, ComponentDependency, ComponentRef, EvidenceSpan,
    FileMappingRule, FileSemanticAst, SemanticAstNode, SourceTreeSnapshot,
};
pub use util::{normalize_path, read_purpose_file};
pub use validate::{
    contributions_by_file, policy_index, validate_policy, validate_policy_evidence,
};

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::Utc;
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
            dependencies: vec![],
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
            dependencies: vec![],
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
            dependencies: vec![],
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
            dependencies: vec![],
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
            dependencies: vec![],
            semantic_asts: vec![],
        };

        assert!(validate_policy_evidence(dir.path(), &snapshot, &policy).is_ok());
    }

    #[test]
    fn validate_policy_rejects_dependency_with_unknown_component() {
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
            contributions: vec![ComponentContribution {
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
            }],
            dependencies: vec![crate::infrastructure::ComponentDependency {
                from: crate::infrastructure::ComponentRef {
                    container: "src".to_string(),
                    component: "entrypoint".to_string(),
                },
                to: crate::infrastructure::ComponentRef {
                    container: "src".to_string(),
                    component: "missing".to_string(),
                },
                confidence: 0.8,
                rationale: "invalid reference".to_string(),
            }],
            semantic_asts: vec![],
        };
        assert!(validate_policy(&snapshot, &policy).is_err());
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
