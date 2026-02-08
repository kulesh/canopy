use std::env;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

use crate::error::{CanopyError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelInfo {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmCompletion {
    pub text: String,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
}

#[derive(Debug, Clone)]
pub struct ProviderSelection {
    pub provider_name: Option<String>,
    pub model: Option<String>,
    pub warning: Option<String>,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<LlmCompletion>;
    fn model_info(&self) -> ModelInfo;
}

pub fn provider_from_env() -> (Option<Box<dyn LlmProvider>>, ProviderSelection) {
    let anthropic_key = env::var("ANTHROPIC_API_KEY").ok().filter(|v| !v.is_empty());
    let openai_key = env::var("OPENAI_API_KEY").ok().filter(|v| !v.is_empty());

    if let Some(key) = anthropic_key {
        let model = env::var("CANOPY_ANTHROPIC_MODEL")
            .ok()
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());
        let provider = AnthropicProvider::new(key, model.clone());
        let selection = ProviderSelection {
            provider_name: Some("anthropic".to_string()),
            model: Some(model),
            warning: None,
        };
        return (Some(Box::new(provider)), selection);
    }

    if let Some(key) = openai_key {
        let model = env::var("CANOPY_OPENAI_MODEL")
            .ok()
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "gpt-4.1-mini".to_string());
        let provider = OpenAiProvider::new(key, model.clone());
        let selection = ProviderSelection {
            provider_name: Some("openai".to_string()),
            model: Some(model),
            warning: None,
        };
        return (Some(Box::new(provider)), selection);
    }

    (
        None,
        ProviderSelection {
            provider_name: None,
            model: None,
            warning: Some(
                "No AI provider configured. Set ANTHROPIC_API_KEY or OPENAI_API_KEY to enable summaries and semantic queries."
                    .to_string(),
            ),
        },
    )
}

#[derive(Clone)]
struct AnthropicProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    fn new(api_key: String, model: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .connect_timeout(Duration::from_secs(4))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            api_key,
            model,
            client,
        }
    }
}

#[derive(Clone)]
struct OpenAiProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl OpenAiProvider {
    fn new(api_key: String, model: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .connect_timeout(Duration::from_secs(4))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            api_key,
            model,
            client,
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, prompt: &str) -> Result<LlmCompletion> {
        let mut candidates = vec![self.model.clone()];
        for fallback in [
            "claude-sonnet-4-20250514",
            "claude-3-7-sonnet-20250219",
            "claude-3-5-sonnet-20241022",
        ] {
            if !candidates.iter().any(|m| m == fallback) {
                candidates.push(fallback.to_string());
            }
        }

        let mut last_error = None;
        for model in candidates {
            let req = AnthropicRequest {
                model: model.clone(),
                max_tokens: 512,
                messages: vec![AnthropicMessage {
                    role: "user".to_string(),
                    content: prompt.to_string(),
                }],
            };

            let mut attempts = 0u8;
            loop {
                attempts += 1;
                let response = self
                    .client
                    .post("https://api.anthropic.com/v1/messages")
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", "2023-06-01")
                    .json(&req)
                    .send()
                    .await;

                match response {
                    Ok(resp) if resp.status().is_success() => {
                        let payload = resp.json::<AnthropicResponse>().await?;
                        let text = payload
                            .content
                            .iter()
                            .map(|c| c.text.clone())
                            .collect::<Vec<_>>()
                            .join("\n");

                        return Ok(LlmCompletion {
                            text,
                            prompt_tokens: payload.usage.input_tokens,
                            completion_tokens: payload.usage.output_tokens,
                        });
                    }
                    Ok(resp) if resp.status().as_u16() == 429 && attempts < 3 => {
                        sleep(Duration::from_millis(200u64 * 2u64.pow(attempts as u32))).await;
                        continue;
                    }
                    Ok(resp) => {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        let model_missing = status.as_u16() == 404
                            && body.contains("not_found_error")
                            && body.contains("model");
                        if model_missing {
                            last_error =
                                Some(format!("anthropic model unavailable ({model}): {body}"));
                            break;
                        }
                        return Err(CanopyError::Llm(format!(
                            "anthropic request failed ({status}): {body}"
                        )));
                    }
                    Err(err) if attempts < 3 => {
                        if err.is_timeout() {
                            sleep(Duration::from_millis(200u64 * 2u64.pow(attempts as u32))).await;
                            continue;
                        }
                        return Err(err.into());
                    }
                    Err(err) => return Err(err.into()),
                }
            }
        }

        Err(CanopyError::Llm(last_error.unwrap_or_else(|| {
            "anthropic request failed after trying fallback models".to_string()
        })))
    }

    fn model_info(&self) -> ModelInfo {
        ModelInfo {
            provider: "anthropic".to_string(),
            model: self.model.clone(),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn complete(&self, prompt: &str) -> Result<LlmCompletion> {
        let req = OpenAiRequest {
            model: self.model.clone(),
            messages: vec![OpenAiMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
        };

        let mut attempts = 0u8;
        loop {
            attempts += 1;
            let response = self
                .client
                .post("https://api.openai.com/v1/chat/completions")
                .bearer_auth(&self.api_key)
                .json(&req)
                .send()
                .await;

            match response {
                Ok(resp) if resp.status().is_success() => {
                    let payload = resp.json::<OpenAiResponse>().await?;
                    let text = payload
                        .choices
                        .first()
                        .map(|c| c.message.content.clone())
                        .unwrap_or_default();
                    return Ok(LlmCompletion {
                        text,
                        prompt_tokens: payload.usage.prompt_tokens,
                        completion_tokens: payload.usage.completion_tokens,
                    });
                }
                Ok(resp) if resp.status().as_u16() == 429 && attempts < 3 => {
                    sleep(Duration::from_millis(200u64 * 2u64.pow(attempts as u32))).await;
                    continue;
                }
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(CanopyError::Llm(format!(
                        "openai request failed ({status}): {body}"
                    )));
                }
                Err(err) if attempts < 3 => {
                    if err.is_timeout() {
                        sleep(Duration::from_millis(200u64 * 2u64.pow(attempts as u32))).await;
                        continue;
                    }
                    return Err(err.into());
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    fn model_info(&self) -> ModelInfo {
        ModelInfo {
            provider: "openai".to_string(),
            model: self.model.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    text: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: usize,
    output_tokens: usize,
}

#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
}

#[derive(Debug, Serialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessageOut,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessageOut {
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: usize,
    completion_tokens: usize,
}
