use async_trait::async_trait;
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

use super::parsing::{parse_verification_response, summarize_issues, PolicyVerificationVerdict};
use super::{HarnessAdapter, HarnessConfig, HarnessProgressEvent};

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

    pub(crate) async fn generate_mapping_policy_with_progress(
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
