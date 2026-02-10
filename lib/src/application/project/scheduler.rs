use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::application::harness::{generate_mapping_policy_with_progress, HarnessProgressEvent};
use crate::application::project::events::{ProjectCommand, ProjectEvent};
use crate::application::project::reducer::reduce_event;
use crate::domain::{
    ArchitectureGraph, NodeKind, OnboardingPhase, Project, ProjectMappingExecutionMode,
    ProjectPolicyMode, ProjectRefinementMode, ProjectRepository, ProjectRuntimeState,
    ProjectSettings, ProjectSourceIndexMode,
};
use crate::error::{CanopyError, Result};
use crate::infrastructure::{
    changed_files_between_tree_hashes, collect_source_tree_snapshot, discover_repository,
    head_tree_hash, load_source_index, map_repository_architecture_with_policy_and_progress,
    map_repository_architecture_with_policy_and_progress_for_relative_files, provider_from_env,
    save_source_index, C4MappingPolicy, ComponentContribution, EvidenceSpan, FileMappingRule,
    PersistenceStore, ProjectStore, RepoMapProgress, SourceIndexDaemon, SourceIndexSnapshot,
};

#[derive(Debug, Clone, Copy)]
pub struct ProjectSchedulerConfig {
    pub max_concurrency: usize,
    pub auto_queue_enabled: bool,
    pub mapping_execution_mode: ProjectMappingExecutionMode,
    pub policy_mode: ProjectPolicyMode,
    pub refinement_mode: ProjectRefinementMode,
    pub source_index_mode: ProjectSourceIndexMode,
    pub incremental_max_files: usize,
    pub refine_cache_hit: bool,
}

impl Default for ProjectSchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 1,
            auto_queue_enabled: true,
            mapping_execution_mode: ProjectMappingExecutionMode::StrictModel,
            policy_mode: ProjectPolicyMode::Auto,
            refinement_mode: ProjectRefinementMode::On,
            source_index_mode: ProjectSourceIndexMode::Snapshot,
            incremental_max_files: 120,
            refine_cache_hit: false,
        }
    }
}

impl ProjectSchedulerConfig {
    pub fn from_settings(settings: &ProjectSettings) -> Self {
        Self {
            max_concurrency: settings.onboarding_concurrency.max(1),
            auto_queue_enabled: true,
            mapping_execution_mode: settings.mapping_execution_mode,
            policy_mode: settings.policy_mode,
            refinement_mode: settings.refinement_mode,
            source_index_mode: settings.source_index_mode,
            incremental_max_files: settings.incremental_max_files.max(1),
            refine_cache_hit: settings.refine_cache_hit,
        }
    }
}

pub struct ProjectSchedulerHandle {
    pub command_tx: Sender<ProjectCommand>,
    pub event_rx: Receiver<ProjectEvent>,
}

pub fn start_project_scheduler(
    project: Project,
    store: ProjectStore,
    initial_runtime: ProjectRuntimeState,
    purpose: String,
    config: ProjectSchedulerConfig,
) -> ProjectSchedulerHandle {
    let (command_tx, command_rx) = mpsc::channel::<ProjectCommand>();
    let (event_tx, event_rx) = mpsc::channel::<ProjectEvent>();
    let (worker_tx, worker_rx) = mpsc::channel::<WorkerMessage>();

    thread::spawn(move || {
        run_scheduler_loop(
            project,
            store,
            initial_runtime,
            purpose,
            config,
            command_rx,
            event_tx,
            worker_tx,
            worker_rx,
        );
    });

    ProjectSchedulerHandle {
        command_tx,
        event_rx,
    }
}

#[derive(Debug, Clone)]
enum WorkerMessage {
    Event(ProjectEvent),
    Finished { repository_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct OnboardingCacheState {
    #[serde(default)]
    last_tree_hash: Option<String>,
    #[serde(default)]
    last_completed_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct FastMapOutcome {
    graph: ArchitectureGraph,
    changed_files: Vec<PathBuf>,
    cache_hit: bool,
}

#[allow(clippy::too_many_arguments)]
fn run_scheduler_loop(
    project: Project,
    store: ProjectStore,
    mut runtime_state: ProjectRuntimeState,
    purpose: String,
    config: ProjectSchedulerConfig,
    command_rx: Receiver<ProjectCommand>,
    event_tx: Sender<ProjectEvent>,
    worker_tx: Sender<WorkerMessage>,
    worker_rx: Receiver<WorkerMessage>,
) {
    let repositories: BTreeMap<String, ProjectRepository> = project
        .repositories
        .iter()
        .cloned()
        .map(|repo| (repo.id.clone(), repo))
        .collect();

    let mut queue = VecDeque::<String>::new();
    let mut running = BTreeMap::<String, Arc<AtomicBool>>::new();
    let mut shutdown_requested = false;

    if config.auto_queue_enabled {
        for repo in repositories.values().filter(|repo| repo.enabled) {
            let phase = runtime_state
                .repositories
                .get(&repo.id)
                .map(|state| state.phase)
                .unwrap_or(OnboardingPhase::NotStarted);
            if phase != OnboardingPhase::Ready {
                queue_repository_id(&mut queue, &repo.id);
                emit_event(
                    &event_tx,
                    &store,
                    &mut runtime_state,
                    ProjectEvent::RepositoryQueued {
                        repository_id: repo.id.clone(),
                        message: Some("queued automatically on startup".to_string()),
                    },
                );
            }
        }
    }

    loop {
        while let Ok(command) = command_rx.try_recv() {
            match command {
                ProjectCommand::QueueAllEnabled => {
                    for repo in repositories.values().filter(|repo| repo.enabled) {
                        if running.contains_key(&repo.id) {
                            continue;
                        }
                        if queue_repository_id(&mut queue, &repo.id) {
                            emit_event(
                                &event_tx,
                                &store,
                                &mut runtime_state,
                                ProjectEvent::RepositoryQueued {
                                    repository_id: repo.id.clone(),
                                    message: Some("queued".to_string()),
                                },
                            );
                        }
                    }
                }
                ProjectCommand::QueueRepository { repository_id }
                | ProjectCommand::RetryRepository { repository_id } => {
                    if repositories.contains_key(&repository_id)
                        && !running.contains_key(&repository_id)
                        && queue_repository_id(&mut queue, &repository_id)
                    {
                        emit_event(
                            &event_tx,
                            &store,
                            &mut runtime_state,
                            ProjectEvent::RepositoryQueued {
                                repository_id,
                                message: Some("queued".to_string()),
                            },
                        );
                    }
                }
                ProjectCommand::CancelRepository { repository_id } => {
                    if remove_queued_repository(&mut queue, &repository_id) {
                        emit_event(
                            &event_tx,
                            &store,
                            &mut runtime_state,
                            ProjectEvent::RepositoryCanceled {
                                repository_id,
                                message: Some("canceled before onboarding started".to_string()),
                            },
                        );
                        continue;
                    }

                    if let Some(cancel_flag) = running.get(&repository_id) {
                        cancel_flag.store(true, Ordering::SeqCst);
                        emit_event(
                            &event_tx,
                            &store,
                            &mut runtime_state,
                            ProjectEvent::RepositoryPhase {
                                repository_id,
                                phase: OnboardingPhase::Canceled,
                                progress_percent: None,
                                message: Some(
                                    "cancel requested, waiting for checkpoint".to_string(),
                                ),
                            },
                        );
                    }
                }
                ProjectCommand::SwitchActiveRepository { repository_id } => {
                    emit_event(
                        &event_tx,
                        &store,
                        &mut runtime_state,
                        ProjectEvent::ActiveRepositoryChanged { repository_id },
                    );
                }
                ProjectCommand::Shutdown => {
                    shutdown_requested = true;
                    queue.clear();
                    for cancel_flag in running.values() {
                        cancel_flag.store(true, Ordering::SeqCst);
                    }
                }
            }
        }

        while let Ok(message) = worker_rx.try_recv() {
            match message {
                WorkerMessage::Event(event) => {
                    emit_event(&event_tx, &store, &mut runtime_state, event);
                }
                WorkerMessage::Finished { repository_id } => {
                    running.remove(&repository_id);
                }
            }
        }

        while running.len() < config.max_concurrency {
            let Some(repository_id) = queue.pop_front() else {
                break;
            };
            if running.contains_key(&repository_id) {
                continue;
            }

            let Some(entry) = repositories.get(&repository_id).cloned() else {
                emit_event(
                    &event_tx,
                    &store,
                    &mut runtime_state,
                    ProjectEvent::RepositoryFailed {
                        repository_id,
                        error: "repository id not found in project manifest".to_string(),
                    },
                );
                continue;
            };

            let cancel_flag = Arc::new(AtomicBool::new(false));
            running.insert(repository_id.clone(), cancel_flag.clone());
            let worker_purpose = purpose.clone();
            let worker_tx_cloned = worker_tx.clone();
            let worker_config = config;
            thread::spawn(move || {
                run_onboarding_worker(
                    repository_id,
                    entry,
                    worker_purpose,
                    worker_config,
                    cancel_flag,
                    worker_tx_cloned,
                );
            });
        }

        if shutdown_requested && running.is_empty() {
            break;
        }

        thread::sleep(Duration::from_millis(30));
    }
}

fn emit_event(
    event_tx: &Sender<ProjectEvent>,
    store: &ProjectStore,
    runtime_state: &mut ProjectRuntimeState,
    event: ProjectEvent,
) {
    reduce_event(runtime_state, &event);
    let _ = store.save_runtime_state(runtime_state);
    let _ = event_tx.send(event);
}

fn run_onboarding_worker(
    repository_id: String,
    entry: ProjectRepository,
    purpose: String,
    config: ProjectSchedulerConfig,
    cancel_flag: Arc<AtomicBool>,
    worker_tx: Sender<WorkerMessage>,
) {
    let cancel_requested = || cancel_flag.load(Ordering::SeqCst);
    let send_event = |event: ProjectEvent, tx: &Sender<WorkerMessage>| {
        let _ = tx.send(WorkerMessage::Event(event));
    };

    send_event(
        ProjectEvent::RepositoryPhase {
            repository_id: repository_id.clone(),
            phase: OnboardingPhase::Discovering,
            progress_percent: Some(5),
            message: Some("discovering repository".to_string()),
        },
        &worker_tx,
    );

    let repository = match discover_repository(&entry.path) {
        Ok(repository) => repository,
        Err(err) => {
            send_event(
                ProjectEvent::RepositoryFailed {
                    repository_id: repository_id.clone(),
                    error: err.to_string(),
                },
                &worker_tx,
            );
            let _ = worker_tx.send(WorkerMessage::Finished { repository_id });
            return;
        }
    };
    let persistence = match PersistenceStore::new(&repository.root) {
        Ok(store) => store,
        Err(err) => {
            send_event(
                ProjectEvent::RepositoryFailed {
                    repository_id: repository_id.clone(),
                    error: err.to_string(),
                },
                &worker_tx,
            );
            let _ = worker_tx.send(WorkerMessage::Finished { repository_id });
            return;
        }
    };

    if strict_mapping_mode(config) {
        match onboarding_strict_stage(
            &repository_id,
            &repository,
            &persistence,
            &purpose,
            config,
            cancel_requested,
            |event| {
                let _ = worker_tx.send(WorkerMessage::Event(event));
            },
        ) {
            Ok(outcome) => {
                if cancel_requested() {
                    send_event(
                        ProjectEvent::RepositoryCanceled {
                            repository_id: repository_id.clone(),
                            message: Some("onboarding canceled".to_string()),
                        },
                        &worker_tx,
                    );
                } else {
                    send_event(
                        ProjectEvent::RepositoryReady {
                            repository_id: repository_id.clone(),
                            nodes: outcome.graph.nodes.len(),
                            message: Some(if outcome.cache_hit {
                                "graph loaded from cache (tree hash match)".to_string()
                            } else {
                                "strict model mapping complete".to_string()
                            }),
                        },
                        &worker_tx,
                    );
                }
            }
            Err(err) => {
                if cancel_requested() || matches!(err, CanopyError::Canceled) {
                    send_event(
                        ProjectEvent::RepositoryCanceled {
                            repository_id: repository_id.clone(),
                            message: Some("onboarding canceled".to_string()),
                        },
                        &worker_tx,
                    );
                } else {
                    send_event(
                        ProjectEvent::RepositoryFailed {
                            repository_id: repository_id.clone(),
                            error: err.to_string(),
                        },
                        &worker_tx,
                    );
                }
            }
        }
        let _ = worker_tx.send(WorkerMessage::Finished { repository_id });
        return;
    }

    let fast_outcome = onboarding_fast_stage(
        &repository_id,
        &repository,
        &persistence,
        config,
        cancel_requested,
        |event| {
            let _ = worker_tx.send(WorkerMessage::Event(event));
        },
    );

    match fast_outcome {
        Ok(outcome) => {
            if cancel_requested() {
                send_event(
                    ProjectEvent::RepositoryCanceled {
                        repository_id: repository_id.clone(),
                        message: Some("onboarding canceled".to_string()),
                    },
                    &worker_tx,
                );
                let _ = worker_tx.send(WorkerMessage::Finished { repository_id });
                return;
            }

            send_event(
                ProjectEvent::RepositoryReady {
                    repository_id: repository_id.clone(),
                    nodes: outcome.graph.nodes.len(),
                    message: Some(if outcome.cache_hit {
                        "graph loaded from cache (tree hash match)".to_string()
                    } else if outcome.changed_files.is_empty() {
                        "fast map ready".to_string()
                    } else {
                        format!(
                            "fast map ready (incremental; {} changed file(s))",
                            outcome.changed_files.len()
                        )
                    }),
                },
                &worker_tx,
            );

            if should_run_refinement(config) && (!outcome.cache_hit || config.refine_cache_hit) {
                send_event(
                    ProjectEvent::RepositoryPhase {
                        repository_id: repository_id.clone(),
                        phase: OnboardingPhase::Ready,
                        progress_percent: Some(70),
                        message: Some(
                            "refining architecture with policy in background".to_string(),
                        ),
                    },
                    &worker_tx,
                );
                match onboarding_refinement_stage(
                    &repository_id,
                    &repository,
                    &persistence,
                    &purpose,
                    config,
                    cancel_requested,
                    |event| {
                        let _ = worker_tx.send(WorkerMessage::Event(event));
                    },
                ) {
                    Ok(refined_nodes) => {
                        send_event(
                            ProjectEvent::RepositoryReady {
                                repository_id: repository_id.clone(),
                                nodes: refined_nodes,
                                message: Some("refinement complete".to_string()),
                            },
                            &worker_tx,
                        );
                    }
                    Err(err) if cancel_requested() || matches!(err, CanopyError::Canceled) => {
                        send_event(
                            ProjectEvent::RepositoryCanceled {
                                repository_id: repository_id.clone(),
                                message: Some("onboarding canceled during refinement".to_string()),
                            },
                            &worker_tx,
                        );
                    }
                    Err(err) => {
                        send_event(
                            ProjectEvent::RepositoryPhase {
                                repository_id: repository_id.clone(),
                                phase: OnboardingPhase::Ready,
                                progress_percent: Some(100),
                                message: Some(format!(
                                    "refinement failed, keeping fast map ({err})"
                                )),
                            },
                            &worker_tx,
                        );
                    }
                }
            }
        }
        Err(err) => {
            if cancel_requested() || matches!(err, CanopyError::Canceled) {
                send_event(
                    ProjectEvent::RepositoryCanceled {
                        repository_id: repository_id.clone(),
                        message: Some("onboarding canceled".to_string()),
                    },
                    &worker_tx,
                );
            } else {
                send_event(
                    ProjectEvent::RepositoryFailed {
                        repository_id: repository_id.clone(),
                        error: err.to_string(),
                    },
                    &worker_tx,
                );
            }
        }
    }

    let _ = worker_tx.send(WorkerMessage::Finished { repository_id });
}

fn onboarding_strict_stage(
    repository_id: &str,
    repository: &crate::domain::Repository,
    persistence: &PersistenceStore,
    purpose: &str,
    config: ProjectSchedulerConfig,
    is_canceled: impl Fn() -> bool,
    mut emit: impl FnMut(ProjectEvent),
) -> Result<FastMapOutcome> {
    if is_canceled() {
        return Err(CanopyError::Canceled);
    }

    let onboarding_state_path = persistence.canopy_dir.join("onboarding_state.json");
    let source_index_path = persistence.canopy_dir.join("source_index.json");
    let previous_state = load_onboarding_cache_state(&onboarding_state_path)?;
    let previous_source_index = load_source_index(&source_index_path)?;

    let tree_hash = head_tree_hash(&repository.root).ok();
    if let (Some(current_tree_hash), Some(previous_tree_hash)) = (
        tree_hash.as_ref(),
        previous_state
            .as_ref()
            .and_then(|state| state.last_tree_hash.as_ref()),
    ) {
        if current_tree_hash == previous_tree_hash {
            if let Some(cached_graph) = persistence.load_graph()? {
                return Ok(FastMapOutcome {
                    graph: cached_graph,
                    changed_files: Vec::new(),
                    cache_hit: true,
                });
            }
        }
    }

    let _daemon_guard = if source_index_mode_daemon(config) {
        Some(SourceIndexDaemon::start(
            repository.root.clone(),
            source_index_path.clone(),
            Duration::from_secs(2),
        ))
    } else {
        None
    };
    let current_source_index = SourceIndexSnapshot::build(&repository.root);
    save_source_index(&source_index_path, &current_source_index)?;
    let changed_files = changed_files_for_fast_stage(
        repository,
        previous_state.as_ref(),
        tree_hash.as_deref(),
        previous_source_index.as_ref(),
        &current_source_index,
    )?;

    let (provider, _) = provider_from_env();
    let provider = provider.ok_or_else(|| {
        CanopyError::Validation(
            "strict mapping requires an AI provider (set ANTHROPIC_API_KEY or OPENAI_API_KEY)"
                .to_string(),
        )
    })?;

    emit(ProjectEvent::RepositoryPhase {
        repository_id: repository_id.to_string(),
        phase: OnboardingPhase::Policy,
        progress_percent: Some(25),
        message: Some("strict mode: generating mapping policy".to_string()),
    });

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| CanopyError::Llm(format!("unable to create tokio runtime: {err}")))?;
    let policy = runtime.block_on(generate_mapping_policy_with_progress(
        repository,
        purpose,
        Some(provider.as_ref()),
        |event| {
            emit(ProjectEvent::RepositoryPhase {
                repository_id: repository_id.to_string(),
                phase: OnboardingPhase::Policy,
                progress_percent: Some(25 + (policy_percent(event.clone()) / 4)),
                message: Some(policy_message(event)),
            });
        },
    ))?;
    let policy = policy.ok_or_else(|| {
        CanopyError::Validation(
            "strict mapping requires provider-generated policy, but none was returned".to_string(),
        )
    })?;
    persistence.save_mapping_policy(&policy)?;

    if is_canceled() {
        return Err(CanopyError::Canceled);
    }

    emit(ProjectEvent::RepositoryPhase {
        repository_id: repository_id.to_string(),
        phase: OnboardingPhase::Mapping,
        progress_percent: Some(60),
        message: Some("strict mode: mapping architecture from policy".to_string()),
    });

    let mut graph = map_repository_architecture_with_policy_and_progress(
        repository,
        Some(&policy),
        |progress| {
            emit(ProjectEvent::RepositoryPhase {
                repository_id: repository_id.to_string(),
                phase: OnboardingPhase::Mapping,
                progress_percent: Some(60 + (percent_for_progress(&progress) / 3)),
                message: Some(format!(
                    "strict policy map: {}",
                    message_for_progress(progress)
                )),
            });
        },
    )?;

    graph.rebuild_dependents();
    graph.validate()?;
    persistence.save_graph(&graph)?;
    save_onboarding_cache_state(
        &onboarding_state_path,
        &OnboardingCacheState {
            last_tree_hash: tree_hash,
            last_completed_at: Some(Utc::now()),
        },
    )?;

    Ok(FastMapOutcome {
        graph,
        changed_files,
        cache_hit: false,
    })
}

fn onboarding_fast_stage(
    repository_id: &str,
    repository: &crate::domain::Repository,
    persistence: &PersistenceStore,
    config: ProjectSchedulerConfig,
    is_canceled: impl Fn() -> bool,
    mut emit: impl FnMut(ProjectEvent),
) -> Result<FastMapOutcome> {
    if is_canceled() {
        return Err(CanopyError::Canceled);
    }

    let onboarding_state_path = persistence.canopy_dir.join("onboarding_state.json");
    let source_index_path = persistence.canopy_dir.join("source_index.json");
    let previous_state = load_onboarding_cache_state(&onboarding_state_path)?;
    let previous_source_index = load_source_index(&source_index_path)?;

    let tree_hash = head_tree_hash(&repository.root).ok();
    if let (Some(current_tree_hash), Some(previous_tree_hash)) = (
        tree_hash.as_ref(),
        previous_state
            .as_ref()
            .and_then(|state| state.last_tree_hash.as_ref()),
    ) {
        if current_tree_hash == previous_tree_hash {
            if let Some(cached_graph) = persistence.load_graph()? {
                return Ok(FastMapOutcome {
                    graph: cached_graph,
                    changed_files: Vec::new(),
                    cache_hit: true,
                });
            }
        }
    }

    let _daemon_guard = if source_index_mode_daemon(config) {
        Some(SourceIndexDaemon::start(
            repository.root.clone(),
            source_index_path.clone(),
            Duration::from_secs(2),
        ))
    } else {
        None
    };
    let current_source_index = SourceIndexSnapshot::build(&repository.root);
    save_source_index(&source_index_path, &current_source_index)?;

    let changed_files = changed_files_for_fast_stage(
        repository,
        previous_state.as_ref(),
        tree_hash.as_deref(),
        previous_source_index.as_ref(),
        &current_source_index,
    )?;

    if is_canceled() {
        return Err(CanopyError::Canceled);
    }

    let cached_graph = persistence.load_graph()?;
    let incremental_limit = config.incremental_max_files.max(1);
    let use_incremental = cached_graph.is_some()
        && !changed_files.is_empty()
        && changed_files.len() <= incremental_limit;
    let mut graph = if use_incremental {
        let legacy_policy = legacy_fallback_policy_for_files(repository, &changed_files);
        emit(ProjectEvent::RepositoryPhase {
            repository_id: repository_id.to_string(),
            phase: OnboardingPhase::Mapping,
            progress_percent: Some(30),
            message: Some(format!(
                "running incremental fast map for {} changed file(s)",
                changed_files.len()
            )),
        });
        let delta = map_repository_architecture_with_policy_and_progress_for_relative_files(
            repository,
            Some(&legacy_policy),
            &changed_files,
            |progress| {
                emit(ProjectEvent::RepositoryPhase {
                    repository_id: repository_id.to_string(),
                    phase: OnboardingPhase::Mapping,
                    progress_percent: Some(percent_for_progress(&progress)),
                    message: Some(format!("incremental: {}", message_for_progress(progress))),
                });
            },
        )?;
        merge_incremental_graph(
            cached_graph.as_ref().expect("cached graph"),
            &delta,
            &changed_files,
        )
    } else {
        emit(ProjectEvent::RepositoryPhase {
            repository_id: repository_id.to_string(),
            phase: OnboardingPhase::Mapping,
            progress_percent: Some(30),
            message: Some("running full fast map".to_string()),
        });
        let legacy_policy = legacy_fallback_policy_for_snapshot(repository);
        map_repository_architecture_with_policy_and_progress(
            repository,
            Some(&legacy_policy),
            |progress| {
                emit(ProjectEvent::RepositoryPhase {
                    repository_id: repository_id.to_string(),
                    phase: OnboardingPhase::Mapping,
                    progress_percent: Some(percent_for_progress(&progress)),
                    message: Some(message_for_progress(progress)),
                });
            },
        )?
    };

    if is_canceled() {
        return Err(CanopyError::Canceled);
    }

    emit(ProjectEvent::RepositoryPhase {
        repository_id: repository_id.to_string(),
        phase: OnboardingPhase::Mapping,
        progress_percent: Some(35),
        message: Some("mapping architecture graph".to_string()),
    });

    emit(ProjectEvent::RepositoryPhase {
        repository_id: repository_id.to_string(),
        phase: OnboardingPhase::Validating,
        progress_percent: Some(98),
        message: Some("validating fast map".to_string()),
    });

    graph.rebuild_dependents();
    graph.validate()?;
    persistence.save_graph(&graph)?;

    save_onboarding_cache_state(
        &onboarding_state_path,
        &OnboardingCacheState {
            last_tree_hash: tree_hash.clone(),
            last_completed_at: Some(Utc::now()),
        },
    )?;

    Ok(FastMapOutcome {
        graph,
        changed_files,
        cache_hit: false,
    })
}

fn onboarding_refinement_stage(
    repository_id: &str,
    repository: &crate::domain::Repository,
    persistence: &PersistenceStore,
    purpose: &str,
    config: ProjectSchedulerConfig,
    is_canceled: impl Fn() -> bool,
    mut emit: impl FnMut(ProjectEvent),
) -> Result<usize> {
    if policy_mode_disabled(config) {
        return Ok(persistence
            .load_graph()?
            .map(|graph| graph.nodes.len())
            .unwrap_or_default());
    }

    if is_canceled() {
        return Err(CanopyError::Canceled);
    }

    let (provider, _) = provider_from_env();
    let Some(provider) = provider else {
        return Ok(persistence
            .load_graph()?
            .map(|graph| graph.nodes.len())
            .unwrap_or_default());
    };
    emit(ProjectEvent::RepositoryPhase {
        repository_id: repository_id.to_string(),
        phase: OnboardingPhase::Ready,
        progress_percent: Some(75),
        message: Some("policy refinement: generating mapping policy".to_string()),
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| CanopyError::Llm(format!("unable to create tokio runtime: {err}")))?;
    let policy = runtime.block_on(generate_mapping_policy_with_progress(
        repository,
        purpose,
        Some(provider.as_ref()),
        |event| {
            emit(ProjectEvent::RepositoryPhase {
                repository_id: repository_id.to_string(),
                phase: OnboardingPhase::Ready,
                progress_percent: Some(75 + (policy_percent(event.clone()) / 5)),
                message: Some(policy_message(event)),
            });
        },
    ))?;

    if is_canceled() {
        return Err(CanopyError::Canceled);
    }

    let mut graph = map_repository_architecture_with_policy_and_progress(
        repository,
        policy.as_ref(),
        |progress| {
            emit(ProjectEvent::RepositoryPhase {
                repository_id: repository_id.to_string(),
                phase: OnboardingPhase::Ready,
                progress_percent: Some(85 + (percent_for_progress(&progress) / 7)),
                message: Some(format!(
                    "policy refinement: {}",
                    message_for_progress(progress)
                )),
            });
        },
    )?;

    graph.rebuild_dependents();
    graph.validate()?;
    if let Some(policy) = &policy {
        persistence.save_mapping_policy(policy)?;
    }
    persistence.save_graph(&graph)?;

    let onboarding_state_path = persistence.canopy_dir.join("onboarding_state.json");
    let tree_hash = head_tree_hash(&repository.root).ok();
    save_onboarding_cache_state(
        &onboarding_state_path,
        &OnboardingCacheState {
            last_tree_hash: tree_hash,
            last_completed_at: Some(Utc::now()),
        },
    )?;

    Ok(graph.nodes.len())
}

fn changed_files_for_fast_stage(
    repository: &crate::domain::Repository,
    previous_state: Option<&OnboardingCacheState>,
    current_tree_hash: Option<&str>,
    previous_source_index: Option<&SourceIndexSnapshot>,
    current_source_index: &SourceIndexSnapshot,
) -> Result<Vec<PathBuf>> {
    let mut changed = BTreeSet::<PathBuf>::new();
    for path in current_source_index.changed_files_since(previous_source_index) {
        changed.insert(path);
    }

    if let (Some(old_hash), Some(new_hash)) = (
        previous_state.and_then(|state| state.last_tree_hash.as_deref()),
        current_tree_hash,
    ) {
        if old_hash != new_hash {
            if let Ok(diff_paths) =
                changed_files_between_tree_hashes(&repository.root, old_hash, new_hash)
            {
                for path in diff_paths {
                    changed.insert(path);
                }
            }
        }
    }

    Ok(changed.into_iter().collect())
}

fn merge_incremental_graph(
    base_graph: &ArchitectureGraph,
    delta_graph: &ArchitectureGraph,
    changed_files: &[PathBuf],
) -> ArchitectureGraph {
    let mut merged = base_graph.clone();
    let changed_set = changed_files
        .iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect::<BTreeSet<String>>();

    // Remove changed code units before upserting incremental nodes.
    let mut to_remove = Vec::new();
    for node in merged.nodes.values() {
        if node.kind == NodeKind::CodeUnit {
            let key = node.path.to_string_lossy().replace('\\', "/");
            if changed_set.contains(&key) {
                to_remove.push(node.id.clone());
            }
        }
    }
    for node_id in to_remove {
        merged.nodes.remove(&node_id);
    }

    for node in delta_graph.nodes.values() {
        if node.id == delta_graph.root_id {
            continue;
        }
        merged.nodes.insert(node.id.clone(), node.clone());
    }
    merged.rebuild_dependents();
    merged
}

fn load_onboarding_cache_state(path: &Path) -> Result<Option<OnboardingCacheState>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|source| CanopyError::io(path, source))?;
    let state = serde_json::from_slice::<OnboardingCacheState>(&bytes)?;
    Ok(Some(state))
}

fn save_onboarding_cache_state(path: &Path, state: &OnboardingCacheState) -> Result<()> {
    let payload = serde_json::to_vec_pretty(state)?;
    std::fs::write(path, payload).map_err(|source| CanopyError::io(path, source))
}

fn legacy_fallback_policy_for_snapshot(repository: &crate::domain::Repository) -> C4MappingPolicy {
    let snapshot = collect_source_tree_snapshot(repository).unwrap_or_else(|_| {
        crate::infrastructure::SourceTreeSnapshot {
            repository_name: repository.name.clone(),
            files: Vec::new(),
            directories: Vec::new(),
        }
    });
    legacy_fallback_policy_from_files(&snapshot.files)
}

fn legacy_fallback_policy_for_files(
    repository: &crate::domain::Repository,
    files: &[PathBuf],
) -> C4MappingPolicy {
    let source_files = files
        .iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();
    if source_files.is_empty() {
        return legacy_fallback_policy_for_snapshot(repository);
    }
    legacy_fallback_policy_from_files(&source_files)
}

fn legacy_fallback_policy_from_files(files: &[String]) -> C4MappingPolicy {
    let mut mappings = Vec::new();
    let mut contributions = Vec::new();
    for file in files {
        let path = PathBuf::from(file);
        let container = path
            .components()
            .next()
            .and_then(|component| component.as_os_str().to_str())
            .unwrap_or("app")
            .to_string();
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("component");
        let component = if stem == "__init__" || stem == "mod" || stem == "index" {
            path.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("app")
                .to_string()
        } else {
            stem.to_string()
        };
        mappings.push(FileMappingRule {
            file: file.clone(),
            include: true,
            container: Some(container.clone()),
            component: Some(component.clone()),
            confidence: 0.65,
            rationale: "legacy fallback mapping".to_string(),
        });
        contributions.push(ComponentContribution {
            file: file.clone(),
            container,
            component,
            confidence: 0.65,
            rationale: "legacy fallback mapping".to_string(),
            evidence: vec![EvidenceSpan {
                file: file.clone(),
                start_line: 1,
                end_line: 1,
                excerpt: None,
                reason: "legacy fallback evidence".to_string(),
            }],
        });
    }
    C4MappingPolicy {
        purpose: "legacy fallback mapping".to_string(),
        generated_at: Utc::now(),
        provider: Some("legacy-fallback".to_string()),
        model: Some("legacy".to_string()),
        notes: Some("legacy_hybrid execution mode".to_string()),
        mappings,
        contributions,
        dependencies: vec![],
        semantic_asts: vec![],
    }
}

fn source_index_mode_daemon(config: ProjectSchedulerConfig) -> bool {
    config.source_index_mode == ProjectSourceIndexMode::Daemon
}

fn should_run_refinement(config: ProjectSchedulerConfig) -> bool {
    config.refinement_mode == ProjectRefinementMode::On
}

fn policy_mode_disabled(config: ProjectSchedulerConfig) -> bool {
    config.policy_mode == ProjectPolicyMode::Off
}

fn strict_mapping_mode(config: ProjectSchedulerConfig) -> bool {
    config.mapping_execution_mode == ProjectMappingExecutionMode::StrictModel
}

fn policy_percent(event: HarnessProgressEvent) -> u8 {
    match event {
        HarnessProgressEvent::Phase { percent, .. } => 20 + (percent / 2),
        HarnessProgressEvent::AttemptStart { current, total } => {
            let total = total.max(1);
            let ratio = (current.min(total) as f32) / (total as f32);
            (25.0 + ratio * 20.0).round() as u8
        }
        HarnessProgressEvent::ToolCall { .. } => 55,
        HarnessProgressEvent::ToolResult { .. } => 60,
        HarnessProgressEvent::Verification { percent } => 60 + (percent / 4),
        HarnessProgressEvent::RepairRequested { percent, .. } => 60 + (percent / 4),
        HarnessProgressEvent::Completed { .. } => 75,
    }
}

fn policy_message(event: HarnessProgressEvent) -> String {
    match event {
        HarnessProgressEvent::Phase { label, .. } => format!("policy: {label}"),
        HarnessProgressEvent::AttemptStart { current, total } => {
            format!("policy: attempt {current}/{total}")
        }
        HarnessProgressEvent::ToolCall { tool, .. } => format!("policy: tool {tool}"),
        HarnessProgressEvent::ToolResult { tool, is_error } => {
            if is_error {
                format!("policy: tool {tool} failed")
            } else {
                format!("policy: tool {tool} ok")
            }
        }
        HarnessProgressEvent::Verification { .. } => "policy: verification".to_string(),
        HarnessProgressEvent::RepairRequested { .. } => "policy: repair requested".to_string(),
        HarnessProgressEvent::Completed { provider, model } => {
            format!("policy complete ({provider}:{model})")
        }
    }
}

fn percent_for_progress(progress: &RepoMapProgress) -> u8 {
    match progress {
        RepoMapProgress::ScanStarted => 10,
        RepoMapProgress::FilesScanned { .. } => 18,
        RepoMapProgress::ScanCompleted { .. } => 30,
        RepoMapProgress::PolicyApplied { .. } => 45,
        RepoMapProgress::ContainersBuilt { .. } => 60,
        RepoMapProgress::DependencyPass { .. } => 80,
        RepoMapProgress::Completed { .. } => 90,
    }
}

fn message_for_progress(progress: RepoMapProgress) -> String {
    match progress {
        RepoMapProgress::ScanStarted => "scanning source files".to_string(),
        RepoMapProgress::FilesScanned { source_files } => {
            format!("scanned {source_files} source files")
        }
        RepoMapProgress::ScanCompleted { source_files } => {
            format!("scan complete ({source_files} source files)")
        }
        RepoMapProgress::PolicyApplied {
            included_files,
            excluded_files,
        } => format!("policy applied include={included_files} exclude={excluded_files}"),
        RepoMapProgress::ContainersBuilt { containers } => {
            format!("containers discovered: {containers}")
        }
        RepoMapProgress::DependencyPass { components } => {
            format!("dependency pass across {components} component(s)")
        }
        RepoMapProgress::Completed { nodes } => format!("graph complete ({nodes} nodes)"),
    }
}

fn queue_repository_id(queue: &mut VecDeque<String>, repository_id: &str) -> bool {
    if queue.iter().any(|queued| queued == repository_id) {
        return false;
    }
    queue.push_back(repository_id.to_string());
    true
}

fn remove_queued_repository(queue: &mut VecDeque<String>, repository_id: &str) -> bool {
    let Some(index) = queue.iter().position(|queued| queued == repository_id) else {
        return false;
    };
    queue.remove(index);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_deduplicates_repository_ids() {
        let mut queue = VecDeque::new();
        assert!(queue_repository_id(&mut queue, "repo-a"));
        assert!(!queue_repository_id(&mut queue, "repo-a"));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn cancel_removes_queued_repository() {
        let mut queue = VecDeque::from(vec![
            "repo-a".to_string(),
            "repo-b".to_string(),
            "repo-c".to_string(),
        ]);
        assert!(remove_queued_repository(&mut queue, "repo-b"));
        assert_eq!(
            queue.into_iter().collect::<Vec<_>>(),
            vec!["repo-a".to_string(), "repo-c".to_string()]
        );
    }
}
