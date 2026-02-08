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
fn no_tui_startup_reports_pipeline_stages() {
    let repo = TempDir::new().expect("temp");
    fs::create_dir_all(repo.path().join("src")).expect("src");
    fs::write(repo.path().join("src/main.rs"), "fn main() {}\n").expect("main");
    init_git(repo.path());

    let output = Command::new(env!("CARGO_BIN_EXE_canopy"))
        .arg("--no-tui")
        .arg(repo.path())
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env("CANOPY_SKIP_SDK_HARNESS", "1")
        .output()
        .expect("run canopy");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[1/5] Discovering repository"));
    assert!(stderr.contains("[2/5] Configuring AI provider and cache"));
    assert!(stderr.contains("[3/5] Building architecture graph"));
    assert!(stderr.contains("[4/5] Starting background semantic inference"));
    assert!(stderr.contains("[5/5] Skipping TUI (--no-tui)"));
}
