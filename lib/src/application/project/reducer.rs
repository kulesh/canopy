use chrono::Utc;

use crate::application::project::events::ProjectEvent;
use crate::domain::{OnboardingPhase, ProjectRepositoryState, ProjectRuntimeState};

pub fn reduce_event(runtime_state: &mut ProjectRuntimeState, event: &ProjectEvent) {
    match event {
        ProjectEvent::RepositoryQueued {
            repository_id,
            message,
        } => {
            let state = entry_mut(runtime_state, repository_id);
            state.phase = OnboardingPhase::Queued;
            state.progress_percent = Some(0);
            state.message = message.clone();
            state.last_error = None;
            state.updated_at = Utc::now();
        }
        ProjectEvent::RepositoryPhase {
            repository_id,
            phase,
            progress_percent,
            message,
        } => {
            let state = entry_mut(runtime_state, repository_id);
            state.phase = *phase;
            state.progress_percent = *progress_percent;
            state.message = message.clone();
            if *phase != OnboardingPhase::Failed {
                state.last_error = None;
            }
            state.updated_at = Utc::now();
        }
        ProjectEvent::RepositoryReady {
            repository_id,
            message,
            ..
        } => {
            let state = entry_mut(runtime_state, repository_id);
            state.phase = OnboardingPhase::Ready;
            state.progress_percent = Some(100);
            state.message = message.clone();
            state.last_error = None;
            state.last_ready_at = Some(Utc::now());
            state.updated_at = Utc::now();
        }
        ProjectEvent::RepositoryFailed {
            repository_id,
            error,
        } => {
            let state = entry_mut(runtime_state, repository_id);
            state.phase = OnboardingPhase::Failed;
            state.progress_percent = None;
            state.message = Some("onboarding failed".to_string());
            state.last_error = Some(error.clone());
            state.updated_at = Utc::now();
        }
        ProjectEvent::RepositoryCanceled {
            repository_id,
            message,
        } => {
            let state = entry_mut(runtime_state, repository_id);
            state.phase = OnboardingPhase::Canceled;
            state.progress_percent = None;
            state.message = message.clone();
            state.updated_at = Utc::now();
        }
        ProjectEvent::ActiveRepositoryChanged { .. } => {}
    }
}

fn entry_mut<'a>(
    runtime_state: &'a mut ProjectRuntimeState,
    repository_id: &str,
) -> &'a mut ProjectRepositoryState {
    runtime_state
        .repositories
        .entry(repository_id.to_string())
        .or_insert_with(|| ProjectRepositoryState::new(repository_id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_to_ready_and_records_timestamp() {
        let mut runtime_state = ProjectRuntimeState::default();
        reduce_event(
            &mut runtime_state,
            &ProjectEvent::RepositoryReady {
                repository_id: "repo-a".to_string(),
                nodes: 42,
                message: Some("ready".to_string()),
            },
        );

        let state = runtime_state
            .repositories
            .get("repo-a")
            .expect("repo state present");
        assert_eq!(state.phase, OnboardingPhase::Ready);
        assert_eq!(state.progress_percent, Some(100));
        assert!(state.last_ready_at.is_some());
    }

    #[test]
    fn transitions_to_failed_with_error() {
        let mut runtime_state = ProjectRuntimeState::default();
        reduce_event(
            &mut runtime_state,
            &ProjectEvent::RepositoryFailed {
                repository_id: "repo-a".to_string(),
                error: "mapping failed".to_string(),
            },
        );

        let state = runtime_state
            .repositories
            .get("repo-a")
            .expect("repo state present");
        assert_eq!(state.phase, OnboardingPhase::Failed);
        assert_eq!(state.last_error.as_deref(), Some("mapping failed"));
    }
}
