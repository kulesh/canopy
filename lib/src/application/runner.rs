use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::mpsc;
use std::thread;

use tracing::info;

use crate::application::config::AppConfig;
use crate::application::state::{require_graph, AppState};
use crate::domain::{ArchitectureGraph, Repository};
use crate::error::Result;
use crate::inference::prompt::mapping_policy_prompt;
use crate::inference::{InferenceEngine, InferenceProgress};
use crate::infrastructure::{
    collect_git_signals, collect_source_tree_snapshot, compare_branches, discover_repository,
    load_workspace, map_repository_architecture_with_policy_and_progress, merge_workspace_graphs,
    parse_lcov, parse_policy_response, provider_from_env, validate_policy, C4MappingPolicy,
    InferenceCache, LlmProvider, PersistenceStore, RepoMapProgress,
};
use crate::tui;

pub async fn run(config: AppConfig) -> Result<()> {
    eprintln!("[1/5] Discovering repository...");
    let repository = discover_repository(&config.input_path)?;
    let persistence = PersistenceStore::new(&repository.root)?;

    eprintln!("[2/5] Configuring AI provider and cache...");
    let (provider, selection) = provider_from_env();
    let cache = InferenceCache::open(&persistence.canopy_dir.join("cache.db"))?;

    eprintln!("[3/5] Building architecture graph...");
    let (mut graph, mapping_note) = if let Some(workspace_path) = &config.workspace_path {
        let workspace = load_workspace(workspace_path)?;
        let mut graphs = Vec::new();
        for entry in workspace.repositories {
            let repo = discover_repository(&entry.path)?;
            let policy = try_generate_mapping_policy(&repo, &config.purpose, provider.as_deref())
                .await
                .map_err(|err| {
                    eprintln!(
                        "  -> [{}] policy generation failed, using fallback mapper: {err}",
                        repo.name
                    );
                    err
                })
                .ok()
                .flatten();
            let graph = map_single_repository_graph(&repo, policy.as_ref(), Some(&repo.name))?;
            graphs.push((entry.name, graph));
        }
        (merge_workspace_graphs(&graphs), "map:workspace".to_string())
    } else if let Some(cached) = persistence.load_graph()? {
        eprintln!("  -> loaded cached graph: {} nodes", cached.nodes.len());
        if let Some(policy) = persistence.load_mapping_policy()? {
            (
                cached,
                format!(
                    "map:cached-policy:{}:{}",
                    policy.provider.unwrap_or_else(|| "unknown".to_string()),
                    policy.model.unwrap_or_else(|| "unknown".to_string())
                ),
            )
        } else {
            (cached, "map:cached-graph".to_string())
        }
    } else {
        let (policy, note) =
            match try_generate_mapping_policy(&repository, &config.purpose, provider.as_deref())
                .await
            {
                Ok(Some(policy)) => {
                    persistence.save_mapping_policy(&policy)?;
                    let note = format!(
                        "map:policy:{}:{}",
                        policy
                            .provider
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string()),
                        policy
                            .model
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string())
                    );
                    (Some(policy), note)
                }
                Ok(None) => (None, "map:fallback:no-provider".to_string()),
                Err(err) => {
                    eprintln!("  -> mapping policy unavailable, using fallback mapper: {err}");
                    (None, "map:fallback:policy-error".to_string())
                }
            };

        let graph = map_single_repository_graph(&repository, policy.as_ref(), None)?;
        (graph, note)
    };

    let mut hasher = DefaultHasher::new();
    repository.root.hash(&mut hasher);
    selection.provider_name.hash(&mut hasher);
    selection.model.hash(&mut hasher);
    let repo_hash = format!("{:x}", hasher.finish());

    graph.rebuild_dependents();
    require_graph(&graph)?;
    persistence.save_graph(&graph)?;

    eprintln!("[4/5] Starting background semantic inference...");
    let (inference_tx, inference_rx) = mpsc::channel::<InferenceProgress>();
    let startup_graph = graph.clone();
    let startup_hash = repo_hash.clone();
    let startup_cache_path = persistence.canopy_dir.join("cache.db");
    let (startup_provider, _) = provider_from_env();
    thread::spawn(move || {
        let mut worker_graph = startup_graph;
        let worker_cache = match InferenceCache::open(&startup_cache_path) {
            Ok(cache) => cache,
            Err(err) => {
                let _ = inference_tx.send(InferenceProgress::ProviderDisabled {
                    reason: format!("unable to open inference cache: {err}"),
                });
                return;
            }
        };
        let mut worker = InferenceEngine::new(startup_provider, worker_cache);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        match runtime {
            Ok(rt) => {
                let result = rt.block_on(async {
                    worker
                        .infer_graph_with_progress(&mut worker_graph, &startup_hash, |event| {
                            let _ = inference_tx.send(event);
                        })
                        .await
                });
                if let Err(err) = result {
                    let _ = inference_tx.send(InferenceProgress::ProviderDisabled {
                        reason: err.to_string(),
                    });
                }
            }
            Err(err) => {
                let _ = inference_tx.send(InferenceProgress::ProviderDisabled {
                    reason: format!("unable to create async runtime: {err}"),
                });
            }
        }
    });

    let inference = InferenceEngine::new(provider, cache);

    let mut state = AppState::new(
        repository.clone(),
        persistence,
        graph,
        inference,
        config.author,
    );
    state.attach_inference_events(inference_rx);
    if let Ok(history) = state.persistence.read_queries() {
        state.query_history = history;
    }

    if let Some(warning) = selection.warning {
        state.status_line = warning;
    }
    state.status_line = format!("{} | {mapping_note}", state.status_line);

    let lcov = repository.root.join("coverage/lcov.info");
    if lcov.exists() {
        if let Ok(map) = parse_lcov(&lcov) {
            state.coverage = map;
        }
    }

    if let Ok(signals) = collect_git_signals(&repository.root, 100) {
        state.git_signals = signals;
    }
    if let Ok(diff) = compare_branches(&repository.root, "HEAD~1", "HEAD") {
        state.status_line = format!(
            "{} | branch delta +{} ~{} -{}",
            state.status_line, diff.added, diff.modified, diff.removed
        );
    }

    eprintln!("[5/5] Launching TUI...");
    info!(repo = %repository.root.display(), "starting canopy tui");
    tui::run_tui(&mut state)?;
    info!("canopy tui exited");

    Ok(())
}

fn map_single_repository_graph(
    repository: &Repository,
    policy: Option<&C4MappingPolicy>,
    label: Option<&str>,
) -> Result<ArchitectureGraph> {
    map_repository_architecture_with_policy_and_progress(repository, policy, |event| match event {
        RepoMapProgress::ScanStarted => match label {
            Some(name) => eprintln!("  -> [{name}] scanning files"),
            None => eprintln!("  -> scanning files"),
        },
        RepoMapProgress::FilesScanned { source_files } => match label {
            Some(name) => eprintln!("  -> [{name}] source files scanned: {source_files}"),
            None => eprintln!("  -> source files scanned: {source_files}"),
        },
        RepoMapProgress::ScanCompleted { source_files } => match label {
            Some(name) => eprintln!("  -> [{name}] scan complete: {source_files} source files"),
            None => eprintln!("  -> scan complete: {source_files} source files"),
        },
        RepoMapProgress::PolicyApplied {
            included_files,
            excluded_files,
        } => match label {
            Some(name) => eprintln!(
                "  -> [{name}] policy applied: include={included_files} exclude={excluded_files}"
            ),
            None => {
                eprintln!("  -> policy applied: include={included_files} exclude={excluded_files}")
            }
        },
        RepoMapProgress::ContainersBuilt { containers } => match label {
            Some(name) => eprintln!("  -> [{name}] containers discovered: {containers}"),
            None => eprintln!("  -> containers discovered: {containers}"),
        },
        RepoMapProgress::DependencyPass { components } => match label {
            Some(name) => {
                eprintln!("  -> [{name}] dependency pass across {components} component(s)")
            }
            None => eprintln!("  -> dependency pass across {components} component(s)"),
        },
        RepoMapProgress::Completed { nodes } => match label {
            Some(name) => eprintln!("  -> [{name}] graph complete: {nodes} nodes"),
            None => eprintln!("  -> graph complete: {nodes} nodes"),
        },
    })
}

async fn try_generate_mapping_policy(
    repository: &Repository,
    purpose: &str,
    provider: Option<&dyn LlmProvider>,
) -> Result<Option<C4MappingPolicy>> {
    let Some(provider) = provider else {
        return Ok(None);
    };

    let snapshot = collect_source_tree_snapshot(repository)?;
    if snapshot.files.is_empty() {
        return Ok(None);
    }

    let prompt = mapping_policy_prompt(&snapshot, purpose, 1200);
    let completion = provider.complete(&prompt).await?;
    let model_info = provider.model_info();
    let policy = parse_policy_response(purpose, &completion.text, Some(&model_info))?;
    validate_policy(&snapshot, &policy)?;

    Ok(Some(policy))
}
