use std::path::{Path, PathBuf};

pub fn project_root(manifest_path: &Path) -> PathBuf {
    manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn project_state_dir(manifest_path: &Path) -> PathBuf {
    project_root(manifest_path).join(".canopy-project")
}

pub fn project_runtime_state_path(manifest_path: &Path) -> PathBuf {
    project_state_dir(manifest_path).join("project_state.json")
}
