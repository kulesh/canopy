use std::fs;
use std::path::Path;

use canopy_lib::domain::{NodeKind, Repository};
use canopy_lib::infrastructure::{
    collect_source_tree_snapshot, discover_repository, map_repository_architecture_with_policy,
    C4MappingPolicy, ComponentContribution, EvidenceSpan, FileMappingRule,
};
use tempfile::TempDir;

fn init_git(path: &Path) {
    std::process::Command::new("git")
        .arg("init")
        .current_dir(path)
        .output()
        .expect("git init");
}

fn fixture_repo(language: &str, file_name: &str, code: &str) -> TempDir {
    let dir = TempDir::new().expect("temp");
    fs::create_dir_all(dir.path().join("src")).expect("src");
    fs::write(dir.path().join("src").join(file_name), code).expect("write source");
    fs::write(dir.path().join("README.md"), format!("{language} fixture")).expect("readme");
    init_git(dir.path());
    dir
}

fn strict_smoke_policy(repository: &Repository) -> C4MappingPolicy {
    let snapshot = collect_source_tree_snapshot(repository).expect("snapshot");
    let mappings = snapshot
        .files
        .iter()
        .map(|file| FileMappingRule {
            file: file.clone(),
            include: true,
            container: Some("src".to_string()),
            component: Some("main".to_string()),
            confidence: 0.8,
            rationale: "test scaffold".to_string(),
        })
        .collect::<Vec<_>>();
    let contributions = snapshot
        .files
        .iter()
        .map(|file| ComponentContribution {
            file: file.clone(),
            container: "src".to_string(),
            component: "main".to_string(),
            confidence: 0.8,
            rationale: "test scaffold".to_string(),
            evidence: vec![EvidenceSpan {
                file: file.clone(),
                start_line: 1,
                end_line: 1,
                excerpt: None,
                reason: "test scaffold evidence".to_string(),
            }],
        })
        .collect::<Vec<_>>();
    C4MappingPolicy {
        purpose: "test scaffold".to_string(),
        generated_at: chrono::Utc::now(),
        provider: Some("test".to_string()),
        model: Some("test-model".to_string()),
        notes: None,
        mappings,
        contributions,
        dependencies: vec![],
        semantic_asts: vec![],
    }
}

#[test]
fn golden_set_c4_inference_baseline() {
    let fixtures = vec![
        fixture_repo("rust", "main.rs", "mod auth; fn main() {}"),
        fixture_repo("python", "app.py", "import auth\ndef main():\n    pass\n"),
        fixture_repo("javascript", "app.js", "import auth from './auth.js';"),
    ];

    for fixture in fixtures {
        let repo = discover_repository(fixture.path()).expect("discover");
        let policy = strict_smoke_policy(&repo);
        let graph = map_repository_architecture_with_policy(&repo, &policy).expect("map");
        graph.validate().expect("graph valid");

        let systems = graph
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::System)
            .count();
        let containers = graph
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Container)
            .count();
        let components = graph
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Component)
            .count();

        assert_eq!(systems, 1);
        assert!(containers >= 1);
        assert!(components >= 1);
    }
}

#[test]
fn language_coverage_validation_for_mvp() {
    let fixtures = vec![
        fixture_repo("rust", "main.rs", "fn main() {}"),
        fixture_repo("python", "main.py", "def main():\n    pass\n"),
        fixture_repo("go", "main.go", "package main\nfunc main() {}"),
        fixture_repo("javascript", "main.js", "function main() {}"),
        fixture_repo(
            "java",
            "Main.java",
            "class Main { public static void main(String[] args) {} }",
        ),
    ];

    for fixture in fixtures {
        let repo = discover_repository(fixture.path()).expect("discover");
        let policy = strict_smoke_policy(&repo);
        let graph = map_repository_architecture_with_policy(&repo, &policy).expect("map");
        assert!(graph
            .nodes
            .values()
            .any(|node| node.kind == NodeKind::Component));
    }
}
