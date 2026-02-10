use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::{Project, ProjectRuntimeState};
use crate::error::{CanopyError, Result};

use super::project_paths::{project_root, project_runtime_state_path, project_state_dir};

#[derive(Debug, Clone)]
pub struct ProjectStore {
    manifest_path: PathBuf,
}

impl ProjectStore {
    pub fn new(manifest_path: impl AsRef<Path>) -> Self {
        Self {
            manifest_path: manifest_path.as_ref().to_path_buf(),
        }
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    pub fn load_project(&self) -> Result<Project> {
        let body = fs::read_to_string(&self.manifest_path)
            .map_err(|source| CanopyError::io(&self.manifest_path, source))?;
        let mut project = toml::from_str::<Project>(&body)?;
        self.resolve_repository_paths(&mut project)?;
        project.validate()?;
        Ok(project)
    }

    pub fn load_runtime_state(&self) -> Result<Option<ProjectRuntimeState>> {
        let path = project_runtime_state_path(&self.manifest_path);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(|source| CanopyError::io(&path, source))?;
        let state = serde_json::from_slice::<ProjectRuntimeState>(&bytes)?;
        Ok(Some(state))
    }

    pub fn save_runtime_state(&self, state: &ProjectRuntimeState) -> Result<()> {
        let dir = project_state_dir(&self.manifest_path);
        fs::create_dir_all(&dir).map_err(|source| CanopyError::io(&dir, source))?;
        let path = project_runtime_state_path(&self.manifest_path);
        let payload = serde_json::to_vec_pretty(state)?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let tmp_path =
            path.with_extension(format!("json.tmp.{}.{}", std::process::id(), timestamp));
        fs::write(&tmp_path, payload).map_err(|source| CanopyError::io(&tmp_path, source))?;
        fs::rename(&tmp_path, &path).map_err(|source| CanopyError::io(&path, source))
    }

    fn resolve_repository_paths(&self, project: &mut Project) -> Result<()> {
        let root = project_root(&self.manifest_path);
        for repository in &mut project.repositories {
            if repository.path.is_relative() {
                repository.path = root.join(&repository.path);
            }
            repository.path = fs::canonicalize(&repository.path)
                .map_err(|source| CanopyError::io(&repository.path, source))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::domain::OnboardingPhase;

    #[test]
    fn loads_project_and_resolves_relative_paths() {
        let dir = TempDir::new().expect("temp");
        let repo = dir.path().join("repo-a");
        fs::create_dir_all(&repo).expect("repo");
        fs::write(
            dir.path().join("project.toml"),
            r#"
name = "demo"
active_repository_id = "repo-a"

[[repositories]]
id = "repo-a"
name = "repo-a"
path = "repo-a"
"#,
        )
        .expect("manifest");

        let store = ProjectStore::new(dir.path().join("project.toml"));
        let project = store.load_project().expect("project");
        assert_eq!(project.repositories.len(), 1);
        assert!(project.repositories[0].path.is_absolute());
    }

    #[test]
    fn saves_and_loads_runtime_state() {
        let dir = TempDir::new().expect("temp");
        let manifest = dir.path().join("project.toml");
        fs::write(&manifest, "name = \"demo\"\n").expect("manifest");

        let store = ProjectStore::new(&manifest);
        let mut state = ProjectRuntimeState::default();
        state.repositories.insert(
            "repo-a".to_string(),
            crate::domain::ProjectRepositoryState {
                repository_id: "repo-a".to_string(),
                phase: OnboardingPhase::Queued,
                progress_percent: Some(10),
                message: Some("queued".to_string()),
                last_error: None,
                last_ready_at: None,
                updated_at: chrono::Utc::now(),
            },
        );

        store.save_runtime_state(&state).expect("save");
        let restored = store
            .load_runtime_state()
            .expect("load")
            .expect("some state");
        assert_eq!(restored.repositories.len(), 1);
        assert_eq!(
            restored.repositories["repo-a"].phase,
            crate::domain::OnboardingPhase::Queued
        );
    }
}
