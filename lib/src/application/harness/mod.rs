use tracing::warn;

use crate::domain::Repository;
use crate::error::Result;
use crate::inference::prompt::mapping_policy_verification_request;
use crate::infrastructure::{C4MappingPolicy, LlmProvider, SourceTreeSnapshot};

mod claude_sdk;
mod llm_adapter;
mod parsing;
mod types;

use claude_sdk::{ClaudeSdkExecutor, ClaudeSdkHarnessAdapter};
pub use llm_adapter::LlmHarnessAdapter;
use parsing::PolicyVerificationVerdict;
pub use types::{HarnessAdapter, HarnessConfig, HarnessProgressEvent};

pub(crate) use parsing::{extract_json_object, summarize_issues};

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

pub(crate) async fn verify_policy_with_executor(
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
    parsing::parse_verification_response(&response)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::fs;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use tempfile::TempDir;

    use crate::error::CanopyError;
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
                "dependencies":[],
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
                    "dependencies":[],
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
                    "dependencies":[],
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
                    "dependencies":[],
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
                    "dependencies":[],
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
