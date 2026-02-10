use crate::domain::OnboardingPhase;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectCommand {
    QueueAllEnabled,
    QueueRepository { repository_id: String },
    RetryRepository { repository_id: String },
    CancelRepository { repository_id: String },
    SwitchActiveRepository { repository_id: String },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectEvent {
    RepositoryQueued {
        repository_id: String,
        message: Option<String>,
    },
    RepositoryPhase {
        repository_id: String,
        phase: OnboardingPhase,
        progress_percent: Option<u8>,
        message: Option<String>,
    },
    RepositoryReady {
        repository_id: String,
        nodes: usize,
        message: Option<String>,
    },
    RepositoryFailed {
        repository_id: String,
        error: String,
    },
    RepositoryCanceled {
        repository_id: String,
        message: Option<String>,
    },
    ActiveRepositoryChanged {
        repository_id: String,
    },
}
