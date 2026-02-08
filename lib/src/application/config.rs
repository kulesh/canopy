use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub input_path: PathBuf,
    pub workspace_path: Option<PathBuf>,
    pub author: String,
    pub purpose: String,
}

impl AppConfig {
    pub fn new(
        input_path: PathBuf,
        workspace_path: Option<PathBuf>,
        author: String,
        purpose: String,
    ) -> Self {
        Self {
            input_path,
            workspace_path,
            author,
            purpose,
        }
    }
}
