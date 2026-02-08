use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Repository {
    pub name: String,
    pub root: PathBuf,
    pub git_root: PathBuf,
    pub branch: Option<String>,
}

impl Repository {
    pub fn new(
        name: impl Into<String>,
        root: PathBuf,
        git_root: PathBuf,
        branch: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            root,
            git_root,
            branch,
        }
    }

    pub fn canopy_dir(&self) -> PathBuf {
        self.root.join(".canopy")
    }

    pub fn is_within(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }
}
