use std::collections::HashMap;

use async_trait::async_trait;
use claude_agent_sdk_rs::{
    query_stream, ClaudeAgentOptions, ContentBlock, Message, PermissionMode, SettingSource,
    SystemPrompt,
};
use futures::StreamExt;
use serde_json::{json, Value};
use tracing::debug;

use crate::domain::Repository;
use crate::error::{CanopyError, Result};
use crate::inference::prompt::{mapping_policy_repair_request, mapping_policy_request};
use crate::infrastructure::{
    collect_source_tree_snapshot, parse_policy_response, validate_policy_evidence, C4MappingPolicy,
    CompletionRequest, ModelInfo, PromptTask, ResponseFormat,
};

use super::{
    extract_json_object, summarize_issues, verify_policy_with_executor, HarnessAdapter,
    HarnessConfig, HarnessProgressEvent,
};

#[async_trait(?Send)]
pub(crate) trait ClaudeSdkExecutor: Send + Sync {
    async fn complete(
        &self,
        request: &CompletionRequest,
        repository: &Repository,
        on_progress: &mut dyn FnMut(HarnessProgressEvent),
    ) -> Result<String>;
    fn model_info(&self) -> ModelInfo;
}

struct ClaudeSdkRuntimeExecutor;

#[async_trait(?Send)]
impl ClaudeSdkExecutor for ClaudeSdkRuntimeExecutor {
    async fn complete(
        &self,
        request: &CompletionRequest,
        repository: &Repository,
        on_progress: &mut dyn FnMut(HarnessProgressEvent),
    ) -> Result<String> {
        let mut options = ClaudeAgentOptions {
            cwd: Some(repository.root.clone()),
            system_prompt: Some(SystemPrompt::Text(request.system_prompt.clone())),
            max_turns: Some(16),
            permission_mode: Some(PermissionMode::BypassPermissions),
            setting_sources: Some(vec![
                SettingSource::User,
                SettingSource::Project,
                SettingSource::Local,
            ]),
            ..Default::default()
        };

        if let Some(schema) = output_schema_for_request(request) {
            options.output_format = Some(json!({"type": "json_schema", "schema": schema}));
        }

        let mut stream = query_stream(request.user_prompt.clone(), Some(options))
            .await
            .map_err(|err| CanopyError::Llm(format!("claude sdk query failed: {err}")))?;
        let mut messages = Vec::new();
        let mut tool_names_by_id: HashMap<String, String> = HashMap::new();

        while let Some(message_result) = stream.next().await {
            let message = message_result
                .map_err(|err| CanopyError::Llm(format!("claude sdk stream failed: {err}")))?;
            emit_tool_progress_from_message(&message, &mut tool_names_by_id, on_progress);
            messages.push(message);
        }

        extract_response_text_from_messages(&messages, &request.response_format)
    }

    fn model_info(&self) -> ModelInfo {
        ModelInfo {
            provider: "claude-agent-sdk-rs".to_string(),
            model: "default".to_string(),
        }
    }
}

pub struct ClaudeSdkHarnessAdapter {
    executor: Box<dyn ClaudeSdkExecutor>,
    config: HarnessConfig,
}

impl Default for ClaudeSdkHarnessAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeSdkHarnessAdapter {
    pub fn new() -> Self {
        Self {
            executor: Box::new(ClaudeSdkRuntimeExecutor),
            config: HarnessConfig::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_executor(executor: Box<dyn ClaudeSdkExecutor>) -> Self {
        Self {
            executor,
            config: HarnessConfig::default(),
        }
    }

    pub(super) async fn generate_mapping_policy_with_progress(
        &self,
        repository: &Repository,
        purpose: &str,
        on_progress: &mut dyn FnMut(HarnessProgressEvent),
    ) -> Result<Option<C4MappingPolicy>> {
        on_progress(HarnessProgressEvent::Phase {
            label: "Collecting source tree snapshot",
            percent: 10,
        });
        let snapshot = collect_source_tree_snapshot(repository)?;
        if snapshot.files.is_empty() {
            return Ok(None);
        }

        let model_info = self.executor.model_info();
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

            let completion_text = self
                .executor
                .complete(&request, repository, on_progress)
                .await?;
            let maybe_policy = parse_policy_response(purpose, &completion_text, Some(&model_info))
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
                    request = mapping_policy_repair_request(
                        &snapshot,
                        purpose,
                        self.config.max_files_in_prompt,
                        &completion_text,
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
            match verify_policy_with_executor(
                self.executor.as_ref(),
                repository,
                &snapshot,
                purpose,
                &policy,
                self.config.max_files_in_prompt,
                on_progress,
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
                    request = mapping_policy_repair_request(
                        &snapshot,
                        purpose,
                        self.config.max_files_in_prompt,
                        &completion_text,
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
                    request = mapping_policy_repair_request(
                        &snapshot,
                        purpose,
                        self.config.max_files_in_prompt,
                        &completion_text,
                        &last_error,
                    );
                }
            }
        }

        Err(CanopyError::Llm(format!(
            "claude sdk mapping policy generation exhausted attempts: {last_error}"
        )))
    }
}

#[async_trait(?Send)]
impl HarnessAdapter for ClaudeSdkHarnessAdapter {
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

fn output_schema_for_request(request: &CompletionRequest) -> Option<Value> {
    if request.response_format != ResponseFormat::JsonObject {
        return None;
    }

    match request.task {
        PromptTask::MappingPolicy => Some(mapping_policy_schema()),
        PromptTask::MappingPolicyVerify => Some(mapping_policy_verification_schema()),
        PromptTask::NodeSummary => None,
    }
}

fn mapping_policy_schema() -> Value {
    json!({
        "type": "object",
        "required": ["mappings", "contributions", "dependencies"],
        "additionalProperties": false,
        "properties": {
            "notes": {"type": "string"},
            "mappings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["file", "include", "confidence", "rationale"],
                    "additionalProperties": false,
                    "properties": {
                        "file": {"type": "string", "minLength": 1},
                        "include": {"type": "boolean"},
                        "container": {"type": "string"},
                        "component": {"type": "string"},
                        "confidence": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                        "rationale": {"type": "string", "minLength": 1}
                    }
                }
            },
            "contributions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["file", "container", "component", "confidence", "rationale", "evidence"],
                    "additionalProperties": false,
                    "properties": {
                        "file": {"type": "string", "minLength": 1},
                        "container": {"type": "string", "minLength": 1},
                        "component": {"type": "string", "minLength": 1},
                        "confidence": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                        "rationale": {"type": "string", "minLength": 1},
                        "evidence": {
                            "type": "array",
                            "minItems": 1,
                            "items": {
                                "type": "object",
                                "required": ["file", "start_line", "end_line", "reason"],
                                "additionalProperties": false,
                                "properties": {
                                    "file": {"type": "string", "minLength": 1},
                                    "start_line": {"type": "integer", "minimum": 1},
                                    "end_line": {"type": "integer", "minimum": 1},
                                    "excerpt": {"type": "string"},
                                    "reason": {"type": "string", "minLength": 1}
                                }
                            }
                        }
                    }
                }
            },
            "dependencies": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["from", "to", "confidence", "rationale"],
                    "additionalProperties": false,
                    "properties": {
                        "from": {
                            "type": "object",
                            "required": ["container", "component"],
                            "additionalProperties": false,
                            "properties": {
                                "container": {"type": "string", "minLength": 1},
                                "component": {"type": "string", "minLength": 1}
                            }
                        },
                        "to": {
                            "type": "object",
                            "required": ["container", "component"],
                            "additionalProperties": false,
                            "properties": {
                                "container": {"type": "string", "minLength": 1},
                                "component": {"type": "string", "minLength": 1}
                            }
                        },
                        "confidence": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                        "rationale": {"type": "string", "minLength": 1}
                    }
                }
            },
            "semantic_asts": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["file", "summary"],
                    "additionalProperties": false,
                    "properties": {
                        "file": {"type": "string", "minLength": 1},
                        "language": {"type": "string"},
                        "summary": {"type": "string", "minLength": 1},
                        "nodes": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["kind", "name", "start_line", "end_line", "summary"],
                                "additionalProperties": false,
                                "properties": {
                                    "kind": {"type": "string", "minLength": 1},
                                    "name": {"type": "string", "minLength": 1},
                                    "start_line": {"type": "integer", "minimum": 1},
                                    "end_line": {"type": "integer", "minimum": 1},
                                    "summary": {"type": "string", "minLength": 1}
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

fn mapping_policy_verification_schema() -> Value {
    json!({
        "type": "object",
        "required": ["valid", "issues"],
        "additionalProperties": false,
        "properties": {
            "valid": {"type": "boolean"},
            "issues": {
                "type": "array",
                "items": {"type": "string"}
            }
        }
    })
}

fn extract_response_text_from_messages(
    messages: &[Message],
    response_format: &ResponseFormat,
) -> Result<String> {
    for message in messages {
        if let Message::Result(result) = message {
            if let Some(structured) = &result.structured_output {
                return serde_json::to_string(structured).map_err(CanopyError::from);
            }
        }
    }

    let mut text_parts = Vec::new();
    for message in messages {
        match message {
            Message::Assistant(msg) => {
                for block in &msg.message.content {
                    if let ContentBlock::Text(text) = block {
                        text_parts.push(text.text.clone());
                    }
                }
            }
            Message::Result(result) => {
                if let Some(text) = &result.result {
                    text_parts.push(text.clone());
                }
            }
            _ => {}
        }
    }

    if text_parts.is_empty() {
        return Err(CanopyError::Llm(
            "claude sdk produced no text or structured output".to_string(),
        ));
    }

    let merged = text_parts.join("\n");
    match response_format {
        ResponseFormat::Text => Ok(merged.trim().to_string()),
        ResponseFormat::JsonObject => extract_json_object(&merged).ok_or_else(|| {
            CanopyError::Validation("claude sdk response did not contain valid JSON".to_string())
        }),
    }
}

fn emit_tool_progress_from_message(
    message: &Message,
    tool_names_by_id: &mut HashMap<String, String>,
    on_progress: &mut dyn FnMut(HarnessProgressEvent),
) {
    match message {
        Message::Assistant(assistant) => {
            for block in &assistant.message.content {
                match block {
                    ContentBlock::ToolUse(tool_use) => {
                        let tool = tool_use.name.clone();
                        tool_names_by_id.insert(tool_use.id.clone(), tool.clone());
                        on_progress(HarnessProgressEvent::ToolCall {
                            tool,
                            input_summary: summarize_json(&tool_use.input, 140),
                        });
                    }
                    ContentBlock::ToolResult(tool_result) => {
                        let tool = tool_names_by_id
                            .get(&tool_result.tool_use_id)
                            .cloned()
                            .unwrap_or_else(|| format!("tool:{}", tool_result.tool_use_id));
                        on_progress(HarnessProgressEvent::ToolResult {
                            tool,
                            is_error: tool_result.is_error.unwrap_or(false),
                        });
                    }
                    _ => {}
                }
            }
        }
        Message::StreamEvent(stream_event) => {
            let label = stream_event
                .event
                .get("subtype")
                .or_else(|| stream_event.event.get("type"))
                .and_then(|value| value.as_str())
                .unwrap_or("stream_event");
            on_progress(HarnessProgressEvent::Phase {
                label: "Model activity",
                percent: 35,
            });
            debug!(event = label, "claude sdk stream event");
        }
        _ => {}
    }
}

fn summarize_json(value: &Value, max_chars: usize) -> String {
    let rendered = value.to_string().replace('\n', " ");
    clip_single_line(&rendered, max_chars)
}

fn clip_single_line(input: &str, max_chars: usize) -> String {
    let trimmed = input.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let clipped: String = trimmed.chars().take(max_chars).collect();
    format!("{clipped}...")
}
