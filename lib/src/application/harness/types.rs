use async_trait::async_trait;

use crate::domain::Repository;
use crate::error::Result;
use crate::infrastructure::C4MappingPolicy;

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
