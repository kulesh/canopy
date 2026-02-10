use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::domain::graph::ProvenanceSource;
use crate::domain::{ArchitectureGraph, NodeKind};
use crate::error::Result;
use crate::infrastructure::{CompletionRequest, InferenceCache, LlmProvider};

use super::prompt::node_summary_request;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InferenceStats {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
}

#[derive(Debug, Clone)]
pub enum InferenceProgress {
    Started {
        total_nodes: usize,
    },
    NodeDone {
        processed: usize,
        total: usize,
        node_id: String,
        node_name: String,
        summary: String,
        confidence: f32,
        source: &'static str,
    },
    ProviderDisabled {
        reason: String,
    },
    Completed {
        stats: InferenceStats,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceExecutionMode {
    StrictModel,
    HybridFallback,
}

pub struct InferenceEngine {
    provider: Option<Box<dyn LlmProvider>>,
    cache: InferenceCache,
    purpose: String,
    mode: InferenceExecutionMode,
    pub stats: InferenceStats,
}

impl InferenceEngine {
    pub fn new(
        provider: Option<Box<dyn LlmProvider>>,
        cache: InferenceCache,
        purpose: String,
    ) -> Self {
        Self::new_with_mode(
            provider,
            cache,
            purpose,
            InferenceExecutionMode::StrictModel,
        )
    }

    pub fn new_with_mode(
        provider: Option<Box<dyn LlmProvider>>,
        cache: InferenceCache,
        purpose: String,
        mode: InferenceExecutionMode,
    ) -> Self {
        Self {
            provider,
            cache,
            purpose,
            mode,
            stats: InferenceStats::default(),
        }
    }

    pub async fn infer_graph(
        &mut self,
        graph: &mut ArchitectureGraph,
        repo_hash: &str,
    ) -> Result<()> {
        self.infer_graph_with_progress(graph, repo_hash, |_| {})
            .await
    }

    pub async fn infer_graph_with_progress<F>(
        &mut self,
        graph: &mut ArchitectureGraph,
        repo_hash: &str,
        mut on_progress: F,
    ) -> Result<()>
    where
        F: FnMut(InferenceProgress),
    {
        let ids: Vec<String> = graph.nodes.keys().cloned().collect();
        let mut provider_available = self.provider.is_some();
        let total_nodes = graph
            .nodes
            .values()
            .filter(|node| node.kind != NodeKind::CodeUnit)
            .count();
        let mut processed = 0usize;
        on_progress(InferenceProgress::Started { total_nodes });
        if self.mode == InferenceExecutionMode::StrictModel && self.provider.is_none() {
            on_progress(InferenceProgress::ProviderDisabled {
                reason: "strict summaries require an AI provider".to_string(),
            });
            on_progress(InferenceProgress::Completed {
                stats: self.stats.clone(),
            });
            return Ok(());
        }

        for node_id in ids {
            let Some(node_snapshot) = graph.node(&node_id).cloned() else {
                continue;
            };

            if node_snapshot.kind == NodeKind::CodeUnit {
                continue;
            }

            // Human edits always take precedence until explicit regeneration.
            if node_snapshot.provenance.source == ProvenanceSource::Human {
                continue;
            }

            let request = node_summary_request(graph, &node_snapshot, &self.purpose);
            let cache_key = format!(
                "{repo_hash}:{}:{}",
                node_snapshot.id,
                digest_request(&request)
            );

            let (summary, confidence, prompt_tokens, completion_tokens, source) = if let Some(
                cached,
            ) =
                self.cache.get(&cache_key)?
            {
                self.stats.cache_hits += 1;
                (cached, 0.92, 0, 0, "cache")
            } else {
                self.stats.cache_misses += 1;
                if provider_available {
                    if let Some(provider) = &self.provider {
                        match provider.complete(&request).await {
                            Ok(completion) => {
                                self.cache.set(&cache_key, &completion.text)?;
                                (
                                    completion.text,
                                    0.86,
                                    completion.prompt_tokens,
                                    completion.completion_tokens,
                                    "provider",
                                )
                            }
                            Err(err) => {
                                warn!(
                                    node_id = %node_snapshot.id,
                                    error = %err,
                                    "llm inference failed, disabling provider and falling back to local summaries"
                                );
                                provider_available = false;
                                on_progress(InferenceProgress::ProviderDisabled {
                                    reason: err.to_string(),
                                });
                                if self.mode == InferenceExecutionMode::HybridFallback {
                                    let local = local_summary(&node_snapshot, graph);
                                    self.cache.set(&cache_key, &local)?;
                                    (local, 0.55, 0, 0, "local")
                                } else {
                                    continue;
                                }
                            }
                        }
                    } else if self.mode == InferenceExecutionMode::HybridFallback {
                        let local = local_summary(&node_snapshot, graph);
                        self.cache.set(&cache_key, &local)?;
                        (local, 0.55, 0, 0, "local")
                    } else {
                        continue;
                    }
                } else if self.mode == InferenceExecutionMode::HybridFallback {
                    let local = local_summary(&node_snapshot, graph);
                    self.cache.set(&cache_key, &local)?;
                    (local, 0.55, 0, 0, "local")
                } else {
                    continue;
                }
            };

            if let Some(node) = graph.node_mut(&node_id) {
                node.summary = summary.clone();
                node.confidence = confidence;
                node.last_analyzed = Some(Utc::now());
            }

            self.stats.prompt_tokens += prompt_tokens;
            self.stats.completion_tokens += completion_tokens;
            processed += 1;
            on_progress(InferenceProgress::NodeDone {
                processed,
                total: total_nodes,
                node_id: node_snapshot.id,
                node_name: node_snapshot.name,
                summary,
                confidence,
                source,
            });
        }

        on_progress(InferenceProgress::Completed {
            stats: self.stats.clone(),
        });
        Ok(())
    }

    pub fn regenerate_node_local(
        &self,
        graph: &ArchitectureGraph,
        node_id: &str,
    ) -> Option<String> {
        graph.node(node_id).map(|node| local_summary(node, graph))
    }

    pub fn regenerate_node(
        &mut self,
        graph: &ArchitectureGraph,
        node_id: &str,
    ) -> Option<(String, f32, &'static str)> {
        let node = graph.node(node_id)?;
        let request = node_summary_request(graph, node, &self.purpose);
        if let Some(provider) = &self.provider {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(rt) = runtime {
                if let Ok(completion) = rt.block_on(provider.complete(&request)) {
                    return Some((completion.text, 0.86, "provider"));
                }
            }
        }
        if self.mode == InferenceExecutionMode::HybridFallback {
            Some((local_summary(node, graph), 0.55, "local"))
        } else {
            None
        }
    }
}

fn local_summary(node: &crate::domain::ArchitectureNode, graph: &ArchitectureGraph) -> String {
    let dep_count = node.dependencies.len();
    let dependent_count = node.dependents.len();
    match node.kind {
        NodeKind::System => format!(
            "Top-level system with {} architectural nodes across containers and components.",
            graph.nodes.len()
        ),
        NodeKind::Container => format!(
            "Container '{}' groups {} components and code units.",
            node.name,
            node.children.len()
        ),
        NodeKind::Component => format!(
            "Component '{}' encapsulates local behavior. Depends on {} component(s) and is used by {} component(s).",
            node.name, dep_count, dependent_count
        ),
        NodeKind::CodeUnit => format!("Code unit '{}' implements source-level logic.", node.name),
    }
}

fn digest_request(request: &CompletionRequest) -> u64 {
    let mut hash: u64 = 1469598103934665603;
    for b in request.system_prompt.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    for b in request.user_prompt.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash ^= match request.task {
        crate::infrastructure::PromptTask::MappingPolicy => 1,
        crate::infrastructure::PromptTask::MappingPolicyVerify => 2,
        crate::infrastructure::PromptTask::NodeSummary => 3,
    };
    hash = hash.wrapping_mul(1099511628211);
    hash ^= match request.response_format {
        crate::infrastructure::ResponseFormat::Text => 11,
        crate::infrastructure::ResponseFormat::JsonObject => 12,
    };
    hash = hash.wrapping_mul(1099511628211);
    hash ^= request.max_tokens as u64;
    hash = hash.wrapping_mul(1099511628211);
    hash ^= (request.temperature.to_bits()) as u64;
    hash = hash.wrapping_mul(1099511628211);
    hash
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use async_trait::async_trait;
    use tempfile::TempDir;

    use crate::domain::graph::ProvenanceSource;
    use crate::domain::{ArchitectureGraph, ArchitectureNode, NodeKind};
    use crate::infrastructure::{
        CompletionRequest, InferenceCache, LlmCompletion, LlmProvider, ModelInfo,
    };

    use super::*;

    #[derive(Clone)]
    struct FakeProvider;

    #[async_trait]
    impl LlmProvider for FakeProvider {
        async fn complete(
            &self,
            request: &CompletionRequest,
        ) -> crate::error::Result<LlmCompletion> {
            assert!(!request.system_prompt.is_empty());
            assert!(!request.user_prompt.is_empty());
            Ok(LlmCompletion {
                text: "fake summary".to_string(),
                prompt_tokens: 10,
                completion_tokens: 5,
            })
        }

        fn model_info(&self) -> ModelInfo {
            ModelInfo {
                provider: "fake".to_string(),
                model: "test".to_string(),
            }
        }
    }

    #[derive(Clone)]
    struct ErrorProvider;

    #[async_trait]
    impl LlmProvider for ErrorProvider {
        async fn complete(
            &self,
            _request: &CompletionRequest,
        ) -> crate::error::Result<LlmCompletion> {
            Err(crate::error::CanopyError::Llm(
                "provider failure".to_string(),
            ))
        }

        fn model_info(&self) -> ModelInfo {
            ModelInfo {
                provider: "error".to_string(),
                model: "error".to_string(),
            }
        }
    }

    fn sample_graph() -> ArchitectureGraph {
        let mut graph = ArchitectureGraph::new(
            "system:test".to_string(),
            ArchitectureNode::new(
                "system:test".to_string(),
                "test".to_string(),
                NodeKind::System,
                PathBuf::from("."),
                None,
            ),
        );
        let component = ArchitectureNode::new(
            "component:test".to_string(),
            "Auth".to_string(),
            NodeKind::Component,
            PathBuf::from("src/auth.rs"),
            Some("system:test".to_string()),
        );
        graph.add_node(component);
        graph
    }

    #[tokio::test]
    async fn infer_graph_uses_cache_and_provider() {
        let temp = TempDir::new().expect("temp");
        let cache = InferenceCache::open(&temp.path().join("cache.db")).expect("cache");
        let mut engine = InferenceEngine::new(
            Some(Box::new(FakeProvider)),
            cache,
            "Understand architecture".to_string(),
        );
        let mut graph = sample_graph();

        engine
            .infer_graph(&mut graph, "repo_hash")
            .await
            .expect("inference");
        assert_eq!(engine.stats.cache_misses, 2);
        assert!(graph
            .nodes
            .values()
            .any(|node| node.summary == "fake summary"));

        let cache = InferenceCache::open(&temp.path().join("cache.db")).expect("cache");
        let mut engine_again = InferenceEngine::new(
            Some(Box::new(FakeProvider)),
            cache,
            "Understand architecture".to_string(),
        );
        engine_again
            .infer_graph(&mut graph, "repo_hash")
            .await
            .expect("inference");
        assert!(engine_again.stats.cache_hits >= 1);
    }

    #[tokio::test]
    async fn human_provenance_is_not_overwritten() {
        let temp = TempDir::new().expect("temp");
        let cache = InferenceCache::open(&temp.path().join("cache.db")).expect("cache");
        let mut engine = InferenceEngine::new(
            Some(Box::new(FakeProvider)),
            cache,
            "Understand architecture".to_string(),
        );
        let mut graph = sample_graph();
        if let Some(component) = graph.node_mut("component:test") {
            component.summary = "human summary".to_string();
            component.provenance.source = ProvenanceSource::Human;
        }

        engine
            .infer_graph(&mut graph, "repo_hash")
            .await
            .expect("inference");

        let component = graph.node("component:test").expect("component");
        assert_eq!(component.summary, "human summary");
    }

    #[tokio::test]
    async fn provider_errors_fall_back_to_local_summary() {
        let temp = TempDir::new().expect("temp");
        let cache = InferenceCache::open(&temp.path().join("cache.db")).expect("cache");
        let mut engine = InferenceEngine::new_with_mode(
            Some(Box::new(ErrorProvider)),
            cache,
            "Understand architecture".to_string(),
            InferenceExecutionMode::HybridFallback,
        );
        let mut graph = sample_graph();

        engine
            .infer_graph(&mut graph, "repo_hash")
            .await
            .expect("inference should not fail");

        let component = graph.node("component:test").expect("component");
        assert!(!component.summary.is_empty());
        assert!((component.confidence - 0.55).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn strict_mode_does_not_fallback_to_local_summary() {
        let temp = TempDir::new().expect("temp");
        let cache = InferenceCache::open(&temp.path().join("cache.db")).expect("cache");
        let mut engine = InferenceEngine::new_with_mode(
            Some(Box::new(ErrorProvider)),
            cache,
            "Understand architecture".to_string(),
            InferenceExecutionMode::StrictModel,
        );
        let mut graph = sample_graph();

        engine
            .infer_graph(&mut graph, "repo_hash")
            .await
            .expect("inference should not fail");

        let component = graph.node("component:test").expect("component");
        assert!(component.summary.is_empty());
    }
}
