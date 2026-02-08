use std::fs;
use std::path::Path;

use canopy_lib::domain::NodeKind;
use canopy_lib::infrastructure::{discover_repository, map_repository_architecture};
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

#[test]
fn golden_set_c4_inference_baseline() {
    let fixtures = vec![
        fixture_repo("rust", "main.rs", "mod auth; fn main() {}"),
        fixture_repo("python", "app.py", "import auth\ndef main():\n    pass\n"),
        fixture_repo("javascript", "app.js", "import auth from './auth.js';"),
    ];

    for fixture in fixtures {
        let repo = discover_repository(fixture.path()).expect("discover");
        let graph = map_repository_architecture(&repo).expect("map");
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
        let graph = map_repository_architecture(&repo).expect("map");
        assert!(graph
            .nodes
            .values()
            .any(|node| node.kind == NodeKind::Component));
    }
}
