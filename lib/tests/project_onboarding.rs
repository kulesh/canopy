use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use canopy_lib::application::project::{
    start_project_scheduler, ProjectCommand, ProjectEvent, ProjectSchedulerConfig,
};
use canopy_lib::domain::{ProjectMappingExecutionMode, ProjectRuntimeState};
use canopy_lib::infrastructure::ProjectStore;
use tempfile::TempDir;

fn init_git(path: &Path) {
    std::process::Command::new("git")
        .arg("init")
        .current_dir(path)
        .output()
        .expect("git init");
}

fn make_repo(path: &Path, filename: &str, source: &str) {
    fs::create_dir_all(path.join("src")).expect("create src");
    fs::write(path.join("src").join(filename), source).expect("write source");
    init_git(path);
}

fn wait_for_event(
    event_rx: &std::sync::mpsc::Receiver<ProjectEvent>,
    timeout: Duration,
    predicate: impl Fn(&ProjectEvent) -> bool,
) -> ProjectEvent {
    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            panic!("timed out waiting for project event");
        }
        let remaining = deadline.saturating_duration_since(now);
        let event = event_rx
            .recv_timeout(remaining)
            .expect("project event received");
        if predicate(&event) {
            return event;
        }
    }
}

fn wait_for_terminal_event(
    event_rx: &std::sync::mpsc::Receiver<ProjectEvent>,
    repository_id: &str,
    timeout: Duration,
) -> ProjectEvent {
    wait_for_event(event_rx, timeout, |event| {
        matches!(
            event,
            ProjectEvent::RepositoryReady { repository_id: id, .. }
                | ProjectEvent::RepositoryFailed { repository_id: id, .. }
                | ProjectEvent::RepositoryCanceled { repository_id: id, .. }
                if id == repository_id
        )
    })
}

#[test]
fn onboarding_scheduler_supports_queue_cancel_retry_flow() {
    let workspace = TempDir::new().expect("workspace");
    let repo_a = workspace.path().join("repo-a");
    let repo_b = workspace.path().join("repo-b");
    fs::create_dir_all(&repo_a).expect("repo-a");
    fs::create_dir_all(&repo_b).expect("repo-b");
    make_repo(&repo_a, "main.rs", "mod auth; fn main() { auth::run(); }");
    make_repo(&repo_b, "lib.rs", "pub fn run() {}");

    let manifest = workspace.path().join("project.toml");
    fs::write(
        &manifest,
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
    .expect("manifest");

    let store = ProjectStore::new(&manifest);
    let project = store.load_project().expect("project");
    let runtime_state = ProjectRuntimeState::from_project(&project);
    store
        .save_runtime_state(&runtime_state)
        .expect("save initial runtime");

    let original_anthropic = std::env::var("ANTHROPIC_API_KEY").ok();
    let original_openai = std::env::var("OPENAI_API_KEY").ok();
    std::env::remove_var("ANTHROPIC_API_KEY");
    std::env::remove_var("OPENAI_API_KEY");

    let scheduler = start_project_scheduler(
        project,
        store.clone(),
        runtime_state,
        "Understand architecture".to_string(),
        ProjectSchedulerConfig {
            max_concurrency: 1,
            auto_queue_enabled: false,
            mapping_execution_mode: ProjectMappingExecutionMode::LegacyHybrid,
            ..ProjectSchedulerConfig::default()
        },
    );

    scheduler
        .command_tx
        .send(ProjectCommand::QueueRepository {
            repository_id: "repo-a".to_string(),
        })
        .expect("queue repo-a");
    scheduler
        .command_tx
        .send(ProjectCommand::QueueRepository {
            repository_id: "repo-b".to_string(),
        })
        .expect("queue repo-b");
    scheduler
        .command_tx
        .send(ProjectCommand::CancelRepository {
            repository_id: "repo-b".to_string(),
        })
        .expect("cancel repo-b");

    let terminal_b_canceled =
        wait_for_terminal_event(&scheduler.event_rx, "repo-b", Duration::from_secs(30));
    assert!(
        matches!(terminal_b_canceled, ProjectEvent::RepositoryCanceled { .. }),
        "expected repo-b canceled before retry, got {terminal_b_canceled:?}"
    );

    let terminal_a =
        wait_for_terminal_event(&scheduler.event_rx, "repo-a", Duration::from_secs(30));
    assert!(
        matches!(terminal_a, ProjectEvent::RepositoryReady { .. }),
        "expected repo-a ready, got {terminal_a:?}"
    );

    scheduler
        .command_tx
        .send(ProjectCommand::RetryRepository {
            repository_id: "repo-b".to_string(),
        })
        .expect("retry repo-b");

    let terminal_b_ready =
        wait_for_terminal_event(&scheduler.event_rx, "repo-b", Duration::from_secs(30));
    assert!(
        matches!(terminal_b_ready, ProjectEvent::RepositoryReady { .. }),
        "expected repo-b ready after retry, got {terminal_b_ready:?}"
    );

    scheduler
        .command_tx
        .send(ProjectCommand::Shutdown)
        .expect("shutdown");

    if let Some(value) = original_anthropic {
        std::env::set_var("ANTHROPIC_API_KEY", value);
    }
    if let Some(value) = original_openai {
        std::env::set_var("OPENAI_API_KEY", value);
    }

    let saved_runtime = store
        .load_runtime_state()
        .expect("load runtime")
        .expect("runtime state");
    assert_eq!(
        saved_runtime.repositories["repo-a"].phase,
        canopy_lib::domain::OnboardingPhase::Ready
    );
    assert_eq!(
        saved_runtime.repositories["repo-b"].phase,
        canopy_lib::domain::OnboardingPhase::Ready
    );
}
