use std::fs;
use std::process::Command;

use tempfile::TempDir;

fn init_git(path: &std::path::Path) {
    let status = Command::new("git")
        .arg("init")
        .current_dir(path)
        .status()
        .expect("git init");
    assert!(status.success());
}

#[test]
fn single_repo_mode_launches_tui_and_generates_project_manifest() {
    let repo = TempDir::new().expect("temp");
    fs::create_dir_all(repo.path().join("src")).expect("src");
    fs::write(repo.path().join("src/main.rs"), "fn main() {}\n").expect("main");
    init_git(repo.path());

    let output = Command::new(env!("CARGO_BIN_EXE_canopy"))
        .arg(repo.path())
        .env("CANOPY_TUI_TEST_MODE", "1")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .output()
        .expect("run canopy");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(repo.path().join(".canopy-project/project.toml").exists());
}

#[test]
fn project_mode_launches_tui() {
    let workspace = TempDir::new().expect("workspace");
    let repo_a = workspace.path().join("repo-a");
    let repo_b = workspace.path().join("repo-b");
    fs::create_dir_all(repo_a.join("src")).expect("repo-a src");
    fs::create_dir_all(repo_b.join("src")).expect("repo-b src");
    fs::write(repo_a.join("src/main.rs"), "fn main() {}\n").expect("repo-a main");
    fs::write(repo_b.join("src/lib.rs"), "pub fn run() {}\n").expect("repo-b lib");
    init_git(&repo_a);
    init_git(&repo_b);

    let project_file = workspace.path().join("project.toml");
    fs::write(
        &project_file,
        r#"
name = "demo"
active_repository_id = "repo-a"

[[repositories]]
id = "repo-a"
name = "repo-a"
path = "repo-a"

[[repositories]]
id = "repo-b"
name = "repo-b"
path = "repo-b"
"#,
    )
    .expect("project manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_canopy"))
        .arg("--project")
        .arg(&project_file)
        .env("CANOPY_TUI_TEST_MODE", "1")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .output()
        .expect("run canopy in project mode");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
