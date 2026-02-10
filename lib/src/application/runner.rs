use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use tracing::info;

use crate::application::config::AppConfig;
use crate::application::project::{
    start_project_scheduler, ProjectCommand, ProjectSchedulerConfig,
};
use crate::application::state::{require_graph, AppState};
use crate::domain::{
    ArchitectureGraph, ArchitectureNode, NodeKind, OnboardingPhase, Project,
    ProjectMappingExecutionMode, ProjectRepository, ProjectRuntimeState, ProjectSettings,
    Repository,
};
use crate::error::{CanopyError, Result};
use crate::inference::{InferenceEngine, InferenceExecutionMode};
use crate::infrastructure::{
    discover_repository, is_first_pass_scope_file, load_workspace, provider_from_env,
    InferenceCache, PersistenceStore, ProjectStore,
};
use crate::tui;

pub async fn run(config: AppConfig) -> Result<()> {
    let project_manifest = resolve_project_manifest_path(&config)?;
    run_project_mode(config, &project_manifest)
}

fn resolve_project_manifest_path(config: &AppConfig) -> Result<PathBuf> {
    if let Some(project_path) = config.project_path.clone() {
        return Ok(project_path);
    }

    if let Some(workspace_path) = config.workspace_path.as_ref() {
        return ensure_workspace_project_manifest(workspace_path);
    }

    ensure_single_repo_project_manifest(&config.input_path)
}

fn ensure_single_repo_project_manifest(input_path: &Path) -> Result<PathBuf> {
    let repository = discover_repository(input_path)?;
    let manifest_dir = repository.root.join(".canopy-project");
    fs::create_dir_all(&manifest_dir).map_err(|source| CanopyError::io(&manifest_dir, source))?;
    let manifest_path = manifest_dir.join("project.toml");
    if manifest_path.exists() {
        return Ok(manifest_path);
    }

    let repository_id = "repo".to_string();
    let project = Project {
        name: repository.name.clone(),
        repositories: vec![ProjectRepository {
            id: repository_id.clone(),
            name: repository.name,
            path: PathBuf::from(".."),
            enabled: true,
        }],
        active_repository_id: Some(repository_id),
        settings: ProjectSettings::default(),
    };
    write_project_manifest(&manifest_path, &project)?;
    Ok(manifest_path)
}

fn ensure_workspace_project_manifest(workspace_path: &Path) -> Result<PathBuf> {
    let workspace = load_workspace(workspace_path)?;
    if workspace.repositories.is_empty() {
        return Err(CanopyError::Validation(
            "workspace config requires at least one repository".to_string(),
        ));
    }

    let workspace_root = workspace_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let manifest_dir = workspace_root.join(".canopy-project");
    fs::create_dir_all(&manifest_dir).map_err(|source| CanopyError::io(&manifest_dir, source))?;

    let stem = workspace_path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace");
    let manifest_path = manifest_dir.join(format!("{stem}.project.toml"));
    if manifest_path.exists() {
        return Ok(manifest_path);
    }

    let mut seen_ids = BTreeSet::new();
    let mut repositories = Vec::new();
    for (index, entry) in workspace.repositories.into_iter().enumerate() {
        let base_id = sanitize_project_id(&entry.name);
        let mut id = base_id.clone();
        if id.is_empty() {
            id = format!("repo-{}", index + 1);
        }
        if seen_ids.contains(&id) {
            let mut suffix = 2usize;
            loop {
                let candidate = format!("{id}-{suffix}");
                if !seen_ids.contains(&candidate) {
                    id = candidate;
                    break;
                }
                suffix += 1;
            }
        }
        seen_ids.insert(id.clone());

        let resolved_path = if entry.path.is_relative() {
            workspace_root.join(entry.path)
        } else {
            entry.path
        };
        repositories.push(ProjectRepository {
            id,
            name: entry.name,
            path: resolved_path,
            enabled: true,
        });
    }

    let active_repository_id = repositories.first().map(|repository| repository.id.clone());
    let project = Project {
        name: stem.to_string(),
        repositories,
        active_repository_id,
        settings: ProjectSettings::default(),
    };
    write_project_manifest(&manifest_path, &project)?;
    Ok(manifest_path)
}

fn write_project_manifest(manifest_path: &Path, project: &Project) -> Result<()> {
    let payload = toml::to_string_pretty(project).map_err(|err| {
        CanopyError::Validation(format!("unable to encode project manifest: {err}"))
    })?;
    fs::write(manifest_path, payload).map_err(|source| CanopyError::io(manifest_path, source))
}

fn sanitize_project_id(name: &str) -> String {
    let normalized = name
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    normalized.trim_matches('-').to_string()
}

fn run_project_mode(config: AppConfig, project_path: &Path) -> Result<()> {
    let project_store = ProjectStore::new(project_path);
    let project = project_store.load_project()?;
    let runtime_state = match project_store.load_runtime_state()? {
        Some(state) => state,
        None => {
            let initial = ProjectRuntimeState::from_project(&project);
            project_store.save_runtime_state(&initial)?;
            initial
        }
    };

    let scheduler = start_project_scheduler(
        project.clone(),
        project_store,
        runtime_state.clone(),
        config.purpose.clone(),
        ProjectSchedulerConfig::from_settings(&project.settings),
    );

    let active_repository_id = select_initial_active_repository_id(&project, &runtime_state)
        .ok_or_else(|| {
            CanopyError::Validation(
                "project requires at least one enabled repository entry".to_string(),
            )
        })?;
    let active_repository_entry = project
        .repositories
        .iter()
        .find(|repository| repository.id == active_repository_id)
        .ok_or_else(|| {
            CanopyError::Validation(format!(
                "active repository id '{}' is missing from project manifest",
                active_repository_id
            ))
        })?;

    let active_repository = discover_repository(&active_repository_entry.path)?;
    let persistence = PersistenceStore::new(&active_repository.root)?;
    let graph = load_or_placeholder_graph(&active_repository, &persistence)?;
    let (provider, selection) = provider_from_env();
    let cache = InferenceCache::open(&persistence.canopy_dir.join("cache.db"))?;
    let inference_mode =
        if project.settings.mapping_execution_mode == ProjectMappingExecutionMode::StrictModel {
            InferenceExecutionMode::StrictModel
        } else {
            InferenceExecutionMode::HybridFallback
        };
    let inference =
        InferenceEngine::new_with_mode(provider, cache, config.purpose.clone(), inference_mode);

    let mut state = AppState::new(
        active_repository,
        persistence,
        graph,
        inference,
        config.author,
    );
    let command_tx = scheduler.command_tx;
    let event_rx = scheduler.event_rx;
    state.attach_project_events(event_rx);
    state.attach_project_commands(command_tx.clone());
    state.configure_project(
        project,
        runtime_state,
        project_path.to_path_buf(),
        active_repository_id,
        config.purpose.clone(),
    );

    if let Ok(history) = state.persistence.read_queries() {
        state.query_history = history;
    }

    if let Some(warning) = selection.warning {
        append_status_line(&mut state.status_line, &warning);
    }
    append_status_line(
        &mut state.status_line,
        "Project mode active. Press P for project view.",
    );

    info!(project_file = %project_path.display(), "starting canopy tui");
    let run_result = tui::run_tui(&mut state);
    let _ = command_tx.send(ProjectCommand::Shutdown);
    run_result
}

fn select_initial_active_repository_id(
    project: &Project,
    runtime_state: &ProjectRuntimeState,
) -> Option<String> {
    if let Some(active_id) = project.active_repository_id.as_ref() {
        let is_ready = runtime_state
            .repositories
            .get(active_id)
            .map(|state| state.phase == OnboardingPhase::Ready)
            .unwrap_or(false);
        if is_ready {
            return Some(active_id.clone());
        }
    }

    if let Some(repository) = project.repositories.iter().find(|repository| {
        runtime_state
            .repositories
            .get(&repository.id)
            .map(|state| state.phase == OnboardingPhase::Ready)
            .unwrap_or(false)
    }) {
        return Some(repository.id.clone());
    }

    project
        .repositories
        .iter()
        .find(|repository| repository.enabled)
        .or_else(|| project.repositories.first())
        .map(|repository| repository.id.clone())
}

fn load_or_placeholder_graph(
    repository: &Repository,
    persistence: &PersistenceStore,
) -> Result<ArchitectureGraph> {
    if let Some(graph) = persistence.load_graph()? {
        require_graph(&graph)?;
        return Ok(graph);
    }

    Ok(bootstrap_graph(repository))
}

fn bootstrap_graph(repository: &Repository) -> ArchitectureGraph {
    let root_id = format!("system:{}", sanitize_graph_id(&repository.name));
    let mut graph = ArchitectureGraph::new(
        root_id.clone(),
        ArchitectureNode::new(
            root_id,
            repository.name.clone(),
            NodeKind::System,
            repository.root.clone(),
            None,
        ),
    );

    let mut containers = BTreeMap::<String, PathBuf>::new();
    let mut walker = WalkBuilder::new(&repository.root);
    walker.hidden(true);
    walker.git_ignore(true);
    walker.git_exclude(true);
    walker.parents(true);
    walker.max_depth(Some(6));

    for entry in walker.build() {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let path = entry.path();
        if !is_first_pass_scope_file(path) {
            continue;
        }
        let Ok(relative) = path.strip_prefix(&repository.root) else {
            continue;
        };
        let mut components = relative.components();
        let container = components
            .next()
            .and_then(|component| component.as_os_str().to_str())
            .unwrap_or("root");
        let container_key = if container.is_empty() {
            "root"
        } else {
            container
        };
        containers
            .entry(container_key.to_string())
            .or_insert_with(|| repository.root.join(container_key));
    }

    if containers.is_empty() {
        containers.insert("src".to_string(), repository.root.join("src"));
    }

    for (name, path) in containers {
        let container_id = format!("container:{}", sanitize_graph_id(&name));
        let mut node = ArchitectureNode::new(
            container_id,
            name,
            NodeKind::Container,
            path,
            Some(graph.root_id.clone()),
        );
        node.summary = "Bootstrapped container while onboarding runs".to_string();
        node.confidence = 0.5;
        graph.add_node(node);
    }

    graph
}

fn sanitize_graph_id(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn append_status_line(status_line: &mut String, segment: impl AsRef<str>) {
    let segment = segment.as_ref().trim();
    if segment.is_empty() {
        return;
    }
    if status_line.trim().is_empty() {
        status_line.clear();
        status_line.push_str(segment);
        return;
    }
    status_line.push_str(" | ");
    status_line.push_str(segment);
}
