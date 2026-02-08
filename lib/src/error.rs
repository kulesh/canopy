use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, CanopyError>;

#[derive(Debug, Error)]
pub enum CanopyError {
    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("toml error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("git error: {0}")]
    Git(#[from] git2::Error),

    #[error("llm provider error: {0}")]
    Llm(String),

    #[error("model validation failed: {0}")]
    Validation(String),

    #[error("operation canceled")]
    Canceled,
}

impl CanopyError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
