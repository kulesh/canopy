use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{CanopyError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    pub repositories: Vec<ProjectRepository>,
    #[serde(default)]
    pub active_repository_id: Option<String>,
    #[serde(default)]
    pub settings: ProjectSettings,
}

impl Project {
    pub fn validate(&self) -> Result<()> {
        if self.repositories.is_empty() {
            return Err(CanopyError::Validation(
                "project requires at least one repository".to_string(),
            ));
        }

        let mut ids = std::collections::BTreeSet::new();
        let mut paths = std::collections::BTreeSet::new();
        for repo in &self.repositories {
            if repo.id.trim().is_empty() {
                return Err(CanopyError::Validation(
                    "project repository id cannot be empty".to_string(),
                ));
            }
            if repo.name.trim().is_empty() {
                return Err(CanopyError::Validation(
                    "project repository name cannot be empty".to_string(),
                ));
            }
            if !ids.insert(repo.id.clone()) {
                return Err(CanopyError::Validation(format!(
                    "duplicate project repository id '{}'",
                    repo.id
                )));
            }

            let normalized_path = repo.path.to_string_lossy().replace('\\', "/");
            if !paths.insert(normalized_path.clone()) {
                return Err(CanopyError::Validation(format!(
                    "duplicate project repository path '{}'",
                    normalized_path
                )));
            }
        }

        if let Some(active_id) = &self.active_repository_id {
            if !self.repositories.iter().any(|repo| &repo.id == active_id) {
                return Err(CanopyError::Validation(format!(
                    "active repository id '{}' not found in project repositories",
                    active_id
                )));
            }
        }

        if self.settings.onboarding_concurrency == 0 {
            return Err(CanopyError::Validation(
                "project settings.onboarding_concurrency must be >= 1".to_string(),
            ));
        }
        if self.settings.incremental_max_files == 0 {
            return Err(CanopyError::Validation(
                "project settings.incremental_max_files must be >= 1".to_string(),
            ));
        }
        if self.settings.mapping_execution_mode == ProjectMappingExecutionMode::StrictModel
            && self.settings.policy_mode == ProjectPolicyMode::Off
        {
            return Err(CanopyError::Validation(
                "project settings.policy_mode=off is invalid when mapping_execution_mode=strict_model"
                    .to_string(),
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRepository {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectPolicyMode {
    #[default]
    Auto,
    Off,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRefinementMode {
    #[default]
    On,
    Off,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSourceIndexMode {
    #[default]
    Snapshot,
    Daemon,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectMappingExecutionMode {
    #[default]
    StrictModel,
    LegacyHybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSettings {
    #[serde(default)]
    pub mapping_execution_mode: ProjectMappingExecutionMode,
    #[serde(default)]
    pub policy_mode: ProjectPolicyMode,
    #[serde(default)]
    pub refinement_mode: ProjectRefinementMode,
    #[serde(default)]
    pub source_index_mode: ProjectSourceIndexMode,
    #[serde(default = "default_onboarding_concurrency")]
    pub onboarding_concurrency: usize,
    #[serde(default = "default_incremental_max_files")]
    pub incremental_max_files: usize,
    #[serde(default)]
    pub refine_cache_hit: bool,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            mapping_execution_mode: ProjectMappingExecutionMode::StrictModel,
            policy_mode: ProjectPolicyMode::Auto,
            refinement_mode: ProjectRefinementMode::On,
            source_index_mode: ProjectSourceIndexMode::Snapshot,
            onboarding_concurrency: default_onboarding_concurrency(),
            incremental_max_files: default_incremental_max_files(),
            refine_cache_hit: false,
        }
    }
}

fn default_onboarding_concurrency() -> usize {
    1
}

fn default_incremental_max_files() -> usize {
    120
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingPhase {
    NotStarted,
    Queued,
    Discovering,
    Policy,
    Mapping,
    Validating,
    Ready,
    Failed,
    Canceled,
}

impl OnboardingPhase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Ready | Self::Failed | Self::Canceled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectRepositoryState {
    pub repository_id: String,
    pub phase: OnboardingPhase,
    #[serde(default)]
    pub progress_percent: Option<u8>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub last_ready_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl ProjectRepositoryState {
    pub fn new(repository_id: impl Into<String>) -> Self {
        Self {
            repository_id: repository_id.into(),
            phase: OnboardingPhase::NotStarted,
            progress_percent: None,
            message: None,
            last_error: None,
            last_ready_at: None,
            updated_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProjectRuntimeState {
    #[serde(default)]
    pub repositories: BTreeMap<String, ProjectRepositoryState>,
}

impl ProjectRuntimeState {
    pub fn from_project(project: &Project) -> Self {
        let mut repositories = BTreeMap::new();
        for repository in &project.repositories {
            repositories.insert(
                repository.id.clone(),
                ProjectRepositoryState::new(repository.id.clone()),
            );
        }
        Self { repositories }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_project() -> Project {
        Project {
            name: "demo".to_string(),
            repositories: vec![
                ProjectRepository {
                    id: "repo-a".to_string(),
                    name: "repo-a".to_string(),
                    path: PathBuf::from("/tmp/repo-a"),
                    enabled: true,
                },
                ProjectRepository {
                    id: "repo-b".to_string(),
                    name: "repo-b".to_string(),
                    path: PathBuf::from("/tmp/repo-b"),
                    enabled: true,
                },
            ],
            active_repository_id: Some("repo-a".to_string()),
            settings: ProjectSettings::default(),
        }
    }

    #[test]
    fn validates_well_formed_project() {
        let project = sample_project();
        assert!(project.validate().is_ok());
    }

    #[test]
    fn rejects_duplicate_ids() {
        let mut project = sample_project();
        project.repositories[1].id = "repo-a".to_string();
        assert!(project.validate().is_err());
    }

    #[test]
    fn rejects_unknown_active_repository() {
        let mut project = sample_project();
        project.active_repository_id = Some("missing".to_string());
        assert!(project.validate().is_err());
    }

    #[test]
    fn builds_runtime_state_for_all_repositories() {
        let project = sample_project();
        let runtime = ProjectRuntimeState::from_project(&project);
        assert_eq!(runtime.repositories.len(), 2);
        assert!(runtime.repositories.contains_key("repo-a"));
        assert!(runtime.repositories.contains_key("repo-b"));
    }

    #[test]
    fn rejects_invalid_settings_values() {
        let mut project = sample_project();
        project.settings.onboarding_concurrency = 0;
        assert!(project.validate().is_err());

        project.settings.onboarding_concurrency = 1;
        project.settings.incremental_max_files = 0;
        assert!(project.validate().is_err());
    }

    #[test]
    fn defaults_settings_when_missing_from_manifest() {
        let project = toml::from_str::<Project>(
            r#"
name = "demo"

[[repositories]]
id = "repo-a"
name = "repo-a"
path = "/tmp/repo-a"
"#,
        )
        .expect("project parse");
        assert_eq!(
            project.settings.mapping_execution_mode,
            ProjectMappingExecutionMode::StrictModel
        );
        assert_eq!(project.settings.policy_mode, ProjectPolicyMode::Auto);
        assert_eq!(project.settings.refinement_mode, ProjectRefinementMode::On);
        assert_eq!(
            project.settings.source_index_mode,
            ProjectSourceIndexMode::Snapshot
        );
        assert_eq!(project.settings.onboarding_concurrency, 1);
        assert_eq!(project.settings.incremental_max_files, 120);
        assert!(!project.settings.refine_cache_hit);
    }

    #[test]
    fn rejects_strict_mode_with_policy_off() {
        let mut project = sample_project();
        project.settings.mapping_execution_mode = ProjectMappingExecutionMode::StrictModel;
        project.settings.policy_mode = ProjectPolicyMode::Off;
        assert!(project.validate().is_err());
    }
}
