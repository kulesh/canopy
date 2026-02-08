use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::mpsc;
use std::thread;

use serde_json::json;
use tracing::info;

use crate::application::config::AppConfig;
use crate::application::diagnostics::DiagnosticsLogger;
use crate::application::harness::{generate_mapping_policy_with_progress, HarnessProgressEvent};
use crate::application::state::{require_graph, AppState};
use crate::domain::{ArchitectureGraph, Repository};
use crate::error::Result;
use crate::inference::{InferenceEngine, InferenceProgress};
use crate::infrastructure::{
    collect_git_signals, compare_branches, discover_repository, load_workspace,
    map_repository_architecture_with_policy_and_progress, merge_workspace_graphs, parse_lcov,
    provider_from_env, C4MappingPolicy, InferenceCache, LlmProvider, PersistenceStore,
    RepoMapProgress,
};
use crate::tui;

pub async fn run(config: AppConfig) -> Result<()> {
    eprintln!("[1/5] Discovering repository...");
    let repository = discover_repository(&config.input_path)?;
    let persistence = PersistenceStore::new(&repository.root)?;
    let diagnostics = DiagnosticsLogger::open(&persistence.canopy_dir).ok();
    log_diagnostic(
        diagnostics.as_ref(),
        "startup_phase",
        json!({"step": 1, "total": 5, "label": "Discovering repository", "repo_root": repository.root.display().to_string()}),
    );

    eprintln!("[2/5] Configuring AI provider and cache...");
    let (provider, selection) = provider_from_env();
    let cache = InferenceCache::open(&persistence.canopy_dir.join("cache.db"))?;
    let inference_purpose = config.purpose.clone();
    log_diagnostic(
        diagnostics.as_ref(),
        "startup_phase",
        json!({"step": 2, "total": 5, "label": "Configuring AI provider and cache", "provider": selection.provider_name, "model": selection.model}),
    );

    eprintln!("[3/5] Building architecture graph...");
    let (mut graph, mapping_note) = if let Some(workspace_path) = &config.workspace_path {
        let workspace = load_workspace(workspace_path)?;
        let mut graphs = Vec::new();
        for entry in workspace.repositories {
            let repo = discover_repository(&entry.path)?;
            let policy = try_generate_mapping_policy(
                &repo,
                &config.purpose,
                provider.as_deref(),
                Some(&repo.name),
                diagnostics.as_ref(),
            )
            .await
            .map_err(|err| {
                eprintln!(
                    "  -> [{}] policy generation failed, using fallback mapper: {err}",
                    repo.name
                );
                log_diagnostic(
                    diagnostics.as_ref(),
                    "policy_fallback",
                    json!({"repository": repo.name, "reason": err.to_string()}),
                );
                err
            })
            .ok()
            .flatten();
            let graph = map_single_repository_graph(
                &repo,
                policy.as_ref(),
                Some(&repo.name),
                diagnostics.as_ref(),
            )?;
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
        let (policy, note) = match try_generate_mapping_policy(
            &repository,
            &config.purpose,
            provider.as_deref(),
            None,
            diagnostics.as_ref(),
        )
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
                log_diagnostic(
                    diagnostics.as_ref(),
                    "policy_fallback",
                    json!({"repository": repository.name, "reason": err.to_string()}),
                );
                (None, "map:fallback:policy-error".to_string())
            }
        };

        let graph =
            map_single_repository_graph(&repository, policy.as_ref(), None, diagnostics.as_ref())?;
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
    log_diagnostic(
        diagnostics.as_ref(),
        "graph_ready",
        json!({"nodes": graph.nodes.len(), "mapping_note": mapping_note.clone()}),
    );

    eprintln!("[4/5] Starting background semantic inference...");
    log_diagnostic(
        diagnostics.as_ref(),
        "startup_phase",
        json!({"step": 4, "total": 5, "label": "Starting background semantic inference"}),
    );
    let (inference_tx, inference_rx) = mpsc::channel::<InferenceProgress>();
    let startup_graph = graph.clone();
    let startup_hash = repo_hash.clone();
    let startup_cache_path = persistence.canopy_dir.join("cache.db");
    let startup_purpose = inference_purpose.clone();
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
        let mut worker = InferenceEngine::new(startup_provider, worker_cache, startup_purpose);
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

    let inference = InferenceEngine::new(provider, cache, inference_purpose);

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
        append_status_line(&mut state.status_line, &warning);
    }
    append_status_line(&mut state.status_line, &mapping_note);

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
        append_status_line(
            &mut state.status_line,
            format!(
                "branch delta +{} ~{} -{}",
                diff.added, diff.modified, diff.removed
            ),
        );
    }

    if config.no_tui {
        eprintln!("[5/5] Skipping TUI (--no-tui)...");
        log_diagnostic(
            diagnostics.as_ref(),
            "startup_phase",
            json!({"step": 5, "total": 5, "label": "Skipping TUI", "reason": "--no-tui"}),
        );
        return Ok(());
    }

    eprintln!("[5/5] Launching TUI...");
    log_diagnostic(
        diagnostics.as_ref(),
        "startup_phase",
        json!({"step": 5, "total": 5, "label": "Launching TUI"}),
    );
    info!(repo = %repository.root.display(), "starting canopy tui");
    tui::run_tui(&mut state)?;
    info!("canopy tui exited");

    Ok(())
}

fn map_single_repository_graph(
    repository: &Repository,
    policy: Option<&C4MappingPolicy>,
    label: Option<&str>,
    diagnostics: Option<&DiagnosticsLogger>,
) -> Result<ArchitectureGraph> {
    map_repository_architecture_with_policy_and_progress(repository, policy, |event| {
        let event_name = format!("{event:?}");
        match event {
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
                    eprintln!(
                        "  -> policy applied: include={included_files} exclude={excluded_files}"
                    )
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
        }
        log_diagnostic(
            diagnostics,
            "repo_map_progress",
            json!({"repository": label.unwrap_or(&repository.name), "event": event_name}),
        );
    })
}

async fn try_generate_mapping_policy(
    repository: &Repository,
    purpose: &str,
    provider: Option<&dyn LlmProvider>,
    label: Option<&str>,
    diagnostics: Option<&DiagnosticsLogger>,
) -> Result<Option<C4MappingPolicy>> {
    generate_mapping_policy_with_progress(repository, purpose, provider, |event| {
        emit_harness_progress(label, event, diagnostics);
    })
    .await
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

fn emit_harness_progress(
    repo_label: Option<&str>,
    event: HarnessProgressEvent,
    diagnostics: Option<&DiagnosticsLogger>,
) {
    let prefix = match repo_label {
        Some(name) => format!("  -> [{name}] policy"),
        None => "  -> policy".to_string(),
    };
    match event {
        HarnessProgressEvent::Phase {
            label: phase_label,
            percent,
        } => {
            eprintln!("{prefix} {} {phase_label}", progress_bar(percent));
            log_diagnostic(
                diagnostics,
                "policy_progress",
                json!({"repository": repo_label, "event": "phase", "label": phase_label, "percent": percent}),
            );
        }
        HarnessProgressEvent::AttemptStart { current, total } => {
            eprintln!("{prefix} {} attempt {current}/{total}", progress_bar(25));
            log_diagnostic(
                diagnostics,
                "policy_progress",
                json!({"repository": repo_label, "event": "attempt_start", "current": current, "total": total}),
            );
        }
        HarnessProgressEvent::ToolCall {
            tool,
            input_summary,
        } => {
            eprintln!("{prefix} tool {tool} {input_summary}");
            log_diagnostic(
                diagnostics,
                "policy_tool_call",
                json!({"repository": repo_label, "tool": tool, "input_summary": input_summary}),
            );
        }
        HarnessProgressEvent::ToolResult { tool, is_error } => {
            let status = if is_error { "error" } else { "ok" };
            eprintln!("{prefix} tool-result {tool} {status}");
            log_diagnostic(
                diagnostics,
                "policy_tool_result",
                json!({"repository": repo_label, "tool": tool, "status": status}),
            );
        }
        HarnessProgressEvent::Verification { percent } => {
            eprintln!("{prefix} {} verifying policy", progress_bar(percent));
            log_diagnostic(
                diagnostics,
                "policy_progress",
                json!({"repository": repo_label, "event": "verification", "percent": percent}),
            );
        }
        HarnessProgressEvent::RepairRequested { reason, percent } => {
            eprintln!(
                "{prefix} {} repair requested: {}",
                progress_bar(percent),
                clip_single_line(&reason, 120)
            );
            log_diagnostic(
                diagnostics,
                "policy_repair",
                json!({"repository": repo_label, "percent": percent, "reason": reason}),
            );
        }
        HarnessProgressEvent::Completed { provider, model } => {
            eprintln!(
                "{prefix} {} complete ({provider}:{model})",
                progress_bar(100)
            );
            log_diagnostic(
                diagnostics,
                "policy_completed",
                json!({"repository": repo_label, "provider": provider, "model": model}),
            );
        }
    }
}

fn log_diagnostic(
    diagnostics: Option<&DiagnosticsLogger>,
    event: &str,
    payload: serde_json::Value,
) {
    if let Some(logger) = diagnostics {
        let _ = logger.log(event, payload);
    }
}

fn progress_bar(percent: u8) -> String {
    let width = 18usize;
    let capped = percent.min(100) as usize;
    let filled = (capped * width) / 100;
    let empty = width.saturating_sub(filled);
    format!(
        "[{}{}] {:>3}%",
        "#".repeat(filled),
        "-".repeat(empty),
        capped
    )
}

fn clip_single_line(input: &str, max_chars: usize) -> String {
    let compact = input.replace('\n', " ");
    if compact.chars().count() <= max_chars {
        return compact;
    }
    let clipped: String = compact.chars().take(max_chars).collect();
    format!("{clipped}...")
}
