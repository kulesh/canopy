use std::collections::VecDeque;

use async_trait::async_trait;
use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::domain::Repository;
use crate::error::{CanopyError, Result};
use crate::inference::prompt::{
    mapping_policy_repair_request, mapping_policy_request, mapping_policy_verification_request,
};
use crate::infrastructure::{
    collect_source_tree_snapshot, parse_policy_response, validate_policy_evidence, C4MappingPolicy,
    LlmProvider, SourceTreeSnapshot,
};

mod claude_sdk;
use claude_sdk::{ClaudeSdkExecutor, ClaudeSdkHarnessAdapter};

#[derive(Debug, Clone)]
pub struct HarnessConfig {
    pub max_attempts: usize,
    pub max_files_in_prompt: usize,
    pub enforce_model_verification: bool,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            max_files_in_prompt: 1_200,
            enforce_model_verification: true,
        }
    }
}

#[derive(Debug, Clone)]
pub enum HarnessProgressEvent {
    Phase { label: &'static str, percent: u8 },
    AttemptStart { current: usize, total: usize },
    ToolCall { tool: String, input_summary: String },
    ToolResult { tool: String, is_error: bool },
    Verification { percent: u8 },
    RepairRequested { reason: String, percent: u8 },
    Completed { provider: String, model: String },
}

#[async_trait(?Send)]
pub trait HarnessAdapter {
    async fn generate_mapping_policy(
        &self,
        repository: &Repository,
        purpose: &str,
    ) -> Result<Option<C4MappingPolicy>>;
}

pub async fn generate_mapping_policy(
    repository: &Repository,
    purpose: &str,
    provider: Option<&dyn LlmProvider>,
) -> Result<Option<C4MappingPolicy>> {
    generate_mapping_policy_with_progress(repository, purpose, provider, |_| {}).await
}

pub async fn generate_mapping_policy_with_progress<F>(
    repository: &Repository,
    purpose: &str,
    provider: Option<&dyn LlmProvider>,
    mut on_progress: F,
) -> Result<Option<C4MappingPolicy>>
where
    F: FnMut(HarnessProgressEvent),
{
    let sdk_adapter = ClaudeSdkHarnessAdapter::new();
    match sdk_adapter
        .generate_mapping_policy_with_progress(repository, purpose, &mut on_progress)
        .await
    {
        Ok(policy) => Ok(policy),
        Err(err) if provider.is_some() => {
            on_progress(HarnessProgressEvent::Phase {
                label: "Falling back to native harness",
                percent: 70,
            });
            warn!(
                repo = %repository.root.display(),
                error = %err,
                "claude sdk harness failed; falling back to native llm harness"
            );
            LlmHarnessAdapter::new(provider)
                .generate_mapping_policy_with_progress(repository, purpose, &mut on_progress)
                .await
        }
        Err(err) => Err(err),
    }
}

pub struct LlmHarnessAdapter<'a> {
    provider: Option<&'a dyn LlmProvider>,
    config: HarnessConfig,
}

impl<'a> LlmHarnessAdapter<'a> {
    pub fn new(provider: Option<&'a dyn LlmProvider>) -> Self {
        Self {
            provider,
            config: HarnessConfig::default(),
        }
    }

    pub fn with_config(mut self, config: HarnessConfig) -> Self {
        self.config = config;
        self
    }

    async fn generate_mapping_policy_with_progress(
        &self,
        repository: &Repository,
        purpose: &str,
        on_progress: &mut dyn FnMut(HarnessProgressEvent),
    ) -> Result<Option<C4MappingPolicy>> {
        let Some(provider) = self.provider else {
            return Ok(None);
        };

        on_progress(HarnessProgressEvent::Phase {
            label: "Collecting source tree snapshot",
            percent: 10,
        });
        let snapshot = collect_source_tree_snapshot(repository)?;
        if snapshot.files.is_empty() {
            return Ok(None);
        }

        let model_info = provider.model_info();
        let mut request =
            mapping_policy_request(&snapshot, purpose, self.config.max_files_in_prompt);
        let mut last_error = String::new();

        for attempt in 1..=self.config.max_attempts {
            on_progress(HarnessProgressEvent::AttemptStart {
                current: attempt,
                total: self.config.max_attempts,
            });
            on_progress(HarnessProgressEvent::Phase {
                label: "Generating mapping policy",
                percent: 30,
            });
            info!(
                repo = %repository.root.display(),
                attempt,
                max_attempts = self.config.max_attempts,
                "generating mapping policy"
            );

            let completion = provider.complete(&request).await?;
            let maybe_policy = parse_policy_response(purpose, &completion.text, Some(&model_info))
                .and_then(|policy| {
                    validate_policy_evidence(&repository.root, &snapshot, &policy)?;
                    Ok(policy)
                });

            let policy = match maybe_policy {
                Ok(policy) => policy,
                Err(err) => {
                    last_error = err.to_string();
                    if attempt == self.config.max_attempts {
                        return Err(err);
                    }
                    on_progress(HarnessProgressEvent::RepairRequested {
                        reason: last_error.clone(),
                        percent: 45,
                    });
                    warn!(
                        repo = %repository.root.display(),
                        attempt,
                        error = %last_error,
                        "mapping policy failed structural verification; requesting repair"
                    );
                    request = mapping_policy_repair_request(
                        &snapshot,
                        purpose,
                        self.config.max_files_in_prompt,
                        &completion.text,
                        &last_error,
                    );
                    continue;
                }
            };

            if !self.config.enforce_model_verification {
                on_progress(HarnessProgressEvent::Completed {
                    provider: model_info.provider,
                    model: model_info.model,
                });
                return Ok(Some(policy));
            }

            on_progress(HarnessProgressEvent::Verification { percent: 75 });
            match verify_policy_with_model(
                provider,
                &snapshot,
                purpose,
                &policy,
                self.config.max_files_in_prompt,
            )
            .await
            {
                Ok(verdict) if verdict.valid => {
                    on_progress(HarnessProgressEvent::Completed {
                        provider: model_info.provider,
                        model: model_info.model,
                    });
                    return Ok(Some(policy));
                }
                Ok(verdict) => {
                    let issues = summarize_issues(&verdict.issues);
                    last_error = format!("policy verifier rejected mapping: {issues}");
                    if attempt == self.config.max_attempts {
                        return Err(CanopyError::Validation(last_error));
                    }
                    on_progress(HarnessProgressEvent::RepairRequested {
                        reason: last_error.clone(),
                        percent: 82,
                    });
                    debug!(
                        repo = %repository.root.display(),
                        attempt,
                        issues = %issues,
                        "policy verifier requested mapping repair"
                    );
                    request = mapping_policy_repair_request(
                        &snapshot,
                        purpose,
                        self.config.max_files_in_prompt,
                        &completion.text,
                        &last_error,
                    );
                }
                Err(err) => {
                    last_error = format!("policy verification failed: {err}");
                    if attempt == self.config.max_attempts {
                        return Err(err);
                    }
                    on_progress(HarnessProgressEvent::RepairRequested {
                        reason: last_error.clone(),
                        percent: 82,
                    });
                    warn!(
                        repo = %repository.root.display(),
                        attempt,
                        error = %last_error,
                        "policy verifier failed; requesting repaired policy"
                    );
                    request = mapping_policy_repair_request(
                        &snapshot,
                        purpose,
                        self.config.max_files_in_prompt,
                        &completion.text,
                        &last_error,
                    );
                }
            }
        }

        Err(CanopyError::Llm(format!(
            "mapping policy generation exhausted attempts: {last_error}"
        )))
    }
}

#[async_trait(?Send)]
impl<'a> HarnessAdapter for LlmHarnessAdapter<'a> {
    async fn generate_mapping_policy(
        &self,
        repository: &Repository,
        purpose: &str,
    ) -> Result<Option<C4MappingPolicy>> {
        let mut noop = |_| {};
        self.generate_mapping_policy_with_progress(repository, purpose, &mut noop)
            .await
    }
}

async fn verify_policy_with_executor(
    executor: &dyn ClaudeSdkExecutor,
    repository: &Repository,
    snapshot: &SourceTreeSnapshot,
    purpose: &str,
    policy: &C4MappingPolicy,
    max_files_in_prompt: usize,
    on_progress: &mut dyn FnMut(HarnessProgressEvent),
) -> Result<PolicyVerificationVerdict> {
    let request =
        mapping_policy_verification_request(snapshot, purpose, policy, max_files_in_prompt);
    let response = executor.complete(&request, repository, on_progress).await?;
    parse_verification_response(&response)
}

async fn verify_policy_with_model(
    provider: &dyn LlmProvider,
    snapshot: &SourceTreeSnapshot,
    purpose: &str,
    policy: &C4MappingPolicy,
    max_files_in_prompt: usize,
) -> Result<PolicyVerificationVerdict> {
    let request =
        mapping_policy_verification_request(snapshot, purpose, policy, max_files_in_prompt);
    let completion = provider.complete(&request).await?;
    parse_verification_response(&completion.text)
}

#[derive(Debug, Deserialize)]
struct PolicyVerificationVerdict {
    valid: bool,
    #[serde(default)]
    issues: Vec<String>,
}

fn summarize_issues(issues: &[String]) -> String {
    if issues.is_empty() {
        return "no issues provided".to_string();
    }
    let mut list: VecDeque<String> = issues.iter().cloned().collect();
    list.make_contiguous().sort();
    list.into_iter().take(3).collect::<Vec<_>>().join(" | ")
}

fn parse_verification_response(raw: &str) -> Result<PolicyVerificationVerdict> {
    let json = extract_json_object(raw).ok_or_else(|| {
        CanopyError::Validation(
            "policy verification response did not contain valid JSON".to_string(),
        )
    })?;
    let verdict: PolicyVerificationVerdict = serde_json::from_str(&json)?;
    Ok(verdict)
}

fn extract_json_object(raw: &str) -> Option<String> {
    let stripped = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    if stripped.starts_with('{') && stripped.ends_with('}') {
        return Some(stripped.to_string());
    }

    let start = stripped.find('{')?;
    let end = stripped.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(stripped[start..=end].to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::fs;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use tempfile::TempDir;

    use crate::infrastructure::{CompletionRequest, LlmCompletion, ModelInfo, PromptTask};

    use super::*;

    #[derive(Clone)]
    struct ScriptedProvider {
        steps: Arc<Mutex<VecDeque<(PromptTask, String)>>>,
    }

    impl ScriptedProvider {
        fn new(steps: Vec<(PromptTask, String)>) -> Self {
            Self {
                steps: Arc::new(Mutex::new(VecDeque::from(steps))),
            }
        }

        fn remaining_steps(&self) -> usize {
            self.steps.lock().expect("lock").len()
        }
    }

    #[async_trait]
    impl LlmProvider for ScriptedProvider {
        async fn complete(&self, request: &CompletionRequest) -> Result<LlmCompletion> {
            let mut steps = self.steps.lock().expect("lock");
            let text = if let Some(index) = steps.iter().position(|(task, _)| *task == request.task)
            {
                steps.remove(index).map(|(_, text)| text).ok_or_else(|| {
                    CanopyError::Llm("unable to remove scripted response step".to_string())
                })?
            } else {
                fallback_script_response(request.task.clone())
            };
            Ok(LlmCompletion {
                text,
                prompt_tokens: 10,
                completion_tokens: 8,
            })
        }

        fn model_info(&self) -> ModelInfo {
            ModelInfo {
                provider: "scripted".to_string(),
                model: "test".to_string(),
            }
        }
    }

    #[derive(Clone)]
    struct ScriptedSdkExecutor {
        steps: Arc<Mutex<VecDeque<(PromptTask, String)>>>,
    }

    impl ScriptedSdkExecutor {
        fn new(steps: Vec<(PromptTask, String)>) -> Self {
            Self {
                steps: Arc::new(Mutex::new(VecDeque::from(steps))),
            }
        }
    }

    #[async_trait(?Send)]
    impl ClaudeSdkExecutor for ScriptedSdkExecutor {
        async fn complete(
            &self,
            request: &CompletionRequest,
            _repository: &Repository,
            _on_progress: &mut dyn FnMut(HarnessProgressEvent),
        ) -> Result<String> {
            let mut steps = self.steps.lock().expect("lock");
            let response = if let Some(index) =
                steps.iter().position(|(task, _)| *task == request.task)
            {
                steps
                    .remove(index)
                    .map(|(_, response)| response)
                    .ok_or_else(|| {
                        CanopyError::Llm("unable to remove scripted sdk response step".to_string())
                    })?
            } else {
                fallback_script_response(request.task.clone())
            };
            Ok(response)
        }

        fn model_info(&self) -> ModelInfo {
            ModelInfo {
                provider: "claude-agent-sdk-rs".to_string(),
                model: "scripted".to_string(),
            }
        }
    }

    fn make_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().expect("temp");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(dir.path().join("src/main.rs"), "mod api; fn main() {}").expect("main");
        fs::write(dir.path().join("src/api.rs"), "pub fn run() {}").expect("api");
        std::process::Command::new("git")
            .arg("init")
            .current_dir(dir.path())
            .output()
            .expect("git init");
        let repository = crate::infrastructure::discover_repository(dir.path()).expect("discover");
        (dir, repository)
    }

    fn fallback_script_response(task: PromptTask) -> String {
        match task {
            PromptTask::MappingPolicy => r#"{
                "mappings":[
                    {"file":"src/main.rs","include":true,"container":"src","component":"entrypoint","confidence":0.95,"rationale":"entrypoint"},
                    {"file":"src/api.rs","include":true,"container":"src","component":"api_runtime","confidence":0.95,"rationale":"api behavior"}
                ],
                "contributions":[
                    {"file":"src/main.rs","container":"src","component":"entrypoint","confidence":0.95,"rationale":"entrypoint behavior","evidence":[{"file":"src/main.rs","start_line":1,"end_line":1,"excerpt":"fn main","reason":"main function orchestrates startup"}]},
                    {"file":"src/api.rs","container":"src","component":"api_runtime","confidence":0.95,"rationale":"api behavior","evidence":[{"file":"src/api.rs","start_line":1,"end_line":1,"excerpt":"fn run","reason":"runtime API behavior"}]}
                ],
                "semantic_asts":[]
            }"#
            .to_string(),
            PromptTask::MappingPolicyVerify => r#"{"valid":true,"issues":[]}"#.to_string(),
            PromptTask::NodeSummary => "Fallback summary".to_string(),
        }
    }

    #[tokio::test]
    async fn returns_none_without_provider() {
        let (_dir, repository) = make_repo();
        let harness = LlmHarnessAdapter::new(None);
        let policy = harness
            .generate_mapping_policy(&repository, "Understand architecture")
            .await
            .expect("ok");
        assert!(policy.is_none());
    }

    #[tokio::test]
    async fn repairs_structural_policy_failure_then_succeeds() {
        let (_dir, repository) = make_repo();
        let provider = ScriptedProvider::new(vec![
            (
                PromptTask::MappingPolicy,
                r#"{
                    "mappings":[
                        {"file":"src/main.rs","include":true,"container":"app","component":"entrypoint","confidence":0.9,"rationale":"entrypoint only"}
                    ]
                }"#
                .to_string(),
            ),
            (
                PromptTask::MappingPolicy,
                r#"{
                    "mappings":[
                        {"file":"src/main.rs","include":true,"container":"app","component":"entrypoint","confidence":0.92,"rationale":"entrypoint"},
                        {"file":"src/api.rs","include":true,"container":"app","component":"api_runtime","confidence":0.95,"rationale":"api behavior"}
                    ],
                    "contributions":[
                        {"file":"src/main.rs","container":"app","component":"entrypoint","confidence":0.92,"rationale":"entrypoint behavior","evidence":[{"file":"src/main.rs","start_line":1,"end_line":1,"excerpt":"fn main","reason":"main function orchestrates startup"}]},
                        {"file":"src/api.rs","container":"app","component":"api_runtime","confidence":0.95,"rationale":"api behavior","evidence":[{"file":"src/api.rs","start_line":1,"end_line":1,"excerpt":"fn run","reason":"runtime API behavior"}]}
                    ],
                    "semantic_asts":[]
                }"#
                .to_string(),
            ),
            (
                PromptTask::MappingPolicyVerify,
                r#"{"valid":true,"issues":[]}"#.to_string(),
            ),
        ]);

        let harness = LlmHarnessAdapter::new(Some(&provider));
        let policy = harness
            .generate_mapping_policy(&repository, "Understand architecture")
            .await
            .expect("policy")
            .expect("some policy");

        assert_eq!(policy.mappings.len(), 2);
        assert_eq!(provider.remaining_steps(), 0);
    }

    #[tokio::test]
    async fn verifier_rejection_triggers_repair() {
        let (_dir, repository) = make_repo();
        let provider = ScriptedProvider::new(vec![
            (
                PromptTask::MappingPolicy,
                r#"{
                    "mappings":[
                        {"file":"src/main.rs","include":true,"container":"app","component":"entrypoint","confidence":0.92,"rationale":"entrypoint"},
                        {"file":"src/api.rs","include":true,"container":"app","component":"api_runtime","confidence":0.93,"rationale":"api behavior"}
                    ],
                    "contributions":[
                        {"file":"src/main.rs","container":"app","component":"entrypoint","confidence":0.92,"rationale":"entrypoint behavior","evidence":[{"file":"src/main.rs","start_line":1,"end_line":1,"excerpt":"fn main","reason":"main function orchestrates startup"}]},
                        {"file":"src/api.rs","container":"app","component":"api_runtime","confidence":0.93,"rationale":"api behavior","evidence":[{"file":"src/api.rs","start_line":1,"end_line":1,"excerpt":"fn run","reason":"runtime API behavior"}]}
                    ],
                    "semantic_asts":[]
                }"#
                .to_string(),
            ),
            (
                PromptTask::MappingPolicyVerify,
                r#"{"valid":false,"issues":["container should be src"]}"#.to_string(),
            ),
            (
                PromptTask::MappingPolicy,
                r#"{
                    "mappings":[
                        {"file":"src/main.rs","include":true,"container":"src","component":"entrypoint","confidence":0.95,"rationale":"entrypoint"},
                        {"file":"src/api.rs","include":true,"container":"src","component":"api_runtime","confidence":0.95,"rationale":"api behavior"}
                    ],
                    "contributions":[
                        {"file":"src/main.rs","container":"src","component":"entrypoint","confidence":0.95,"rationale":"entrypoint behavior","evidence":[{"file":"src/main.rs","start_line":1,"end_line":1,"excerpt":"fn main","reason":"main function orchestrates startup"}]},
                        {"file":"src/api.rs","container":"src","component":"api_runtime","confidence":0.95,"rationale":"api behavior","evidence":[{"file":"src/api.rs","start_line":1,"end_line":1,"excerpt":"fn run","reason":"runtime API behavior"}]}
                    ],
                    "semantic_asts":[]
                }"#
                .to_string(),
            ),
            (
                PromptTask::MappingPolicyVerify,
                r#"{"valid":true,"issues":[]}"#.to_string(),
            ),
        ]);

        let harness = LlmHarnessAdapter::new(Some(&provider));
        let policy = harness
            .generate_mapping_policy(&repository, "Understand architecture")
            .await
            .expect("policy")
            .expect("some policy");

        assert!(policy
            .mappings
            .iter()
            .all(|rule| rule.container.as_deref() == Some("src")));
        assert_eq!(provider.remaining_steps(), 0);
    }

    #[tokio::test]
    async fn sdk_harness_uses_executor_and_verifier() {
        let (_dir, repository) = make_repo();
        let executor = ScriptedSdkExecutor::new(vec![
            (
                PromptTask::MappingPolicy,
                r#"{
                    "mappings":[
                        {"file":"src/main.rs","include":true,"container":"src","component":"entrypoint","confidence":0.95,"rationale":"entrypoint"},
                        {"file":"src/api.rs","include":true,"container":"src","component":"api_runtime","confidence":0.95,"rationale":"api behavior"}
                    ],
                    "contributions":[
                        {"file":"src/main.rs","container":"src","component":"entrypoint","confidence":0.95,"rationale":"entrypoint behavior","evidence":[{"file":"src/main.rs","start_line":1,"end_line":1,"excerpt":"fn main","reason":"main function orchestrates startup"}]},
                        {"file":"src/api.rs","container":"src","component":"api_runtime","confidence":0.95,"rationale":"api behavior","evidence":[{"file":"src/api.rs","start_line":1,"end_line":1,"excerpt":"fn run","reason":"runtime API behavior"}]}
                    ],
                    "semantic_asts":[]
                }"#
                .to_string(),
            ),
            (
                PromptTask::MappingPolicyVerify,
                r#"{"valid":true,"issues":[]}"#.to_string(),
            ),
        ]);

        let harness = ClaudeSdkHarnessAdapter::with_executor(Box::new(executor));
        let policy = harness
            .generate_mapping_policy(&repository, "Understand architecture")
            .await
            .expect("policy")
            .expect("some policy");

        assert_eq!(policy.provider.as_deref(), Some("claude-agent-sdk-rs"));
        assert_eq!(policy.mappings.len(), 2);
    }

    #[test]
    fn extracts_json_object_from_markdown() {
        let raw = "```json\n{\"valid\":true,\"issues\":[]}\n```";
        let parsed = extract_json_object(raw).expect("json");
        assert_eq!(parsed, "{\"valid\":true,\"issues\":[]}");
    }
}
