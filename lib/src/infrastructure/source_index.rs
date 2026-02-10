use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::error::{CanopyError, Result};

use super::is_first_pass_scope_file;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceIndexEntry {
    pub relative_path: String,
    pub modified_unix: i64,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceIndexSnapshot {
    pub generated_at: DateTime<Utc>,
    pub files: BTreeMap<String, SourceIndexEntry>,
}

impl SourceIndexSnapshot {
    pub fn build(repository_root: &Path) -> Self {
        let mut files = BTreeMap::new();
        let mut walker = WalkBuilder::new(repository_root);
        walker.hidden(true);
        walker.git_ignore(true);
        walker.git_exclude(true);
        walker.parents(true);

        for entry in walker.build() {
            let Ok(entry) = entry else {
                continue;
            };
            if !entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
            {
                continue;
            }
            let path = entry.path();
            if !is_first_pass_scope_file(path) {
                continue;
            }
            let Ok(relative) = path.strip_prefix(repository_root) else {
                continue;
            };
            let relative_path = relative.to_string_lossy().replace('\\', "/");
            let Ok(metadata) = fs::metadata(path) else {
                continue;
            };
            let modified_unix = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or_default();
            files.insert(
                relative_path.clone(),
                SourceIndexEntry {
                    relative_path,
                    modified_unix,
                    size: metadata.len(),
                },
            );
        }

        Self {
            generated_at: Utc::now(),
            files,
        }
    }

    pub fn changed_files_since(&self, previous: Option<&SourceIndexSnapshot>) -> Vec<PathBuf> {
        let Some(previous) = previous else {
            return self
                .files
                .keys()
                .map(PathBuf::from)
                .collect::<Vec<PathBuf>>();
        };

        let mut changed = BTreeSet::new();
        for (path, entry) in &self.files {
            match previous.files.get(path) {
                None => {
                    changed.insert(path.clone());
                }
                Some(previous_entry) => {
                    if previous_entry.modified_unix != entry.modified_unix
                        || previous_entry.size != entry.size
                    {
                        changed.insert(path.clone());
                    }
                }
            }
        }
        for path in previous.files.keys() {
            if !self.files.contains_key(path) {
                changed.insert(path.clone());
            }
        }

        changed.into_iter().map(PathBuf::from).collect()
    }
}

pub fn load_source_index(path: &Path) -> Result<Option<SourceIndexSnapshot>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|source| CanopyError::io(path, source))?;
    let snapshot = serde_json::from_slice::<SourceIndexSnapshot>(&bytes)?;
    Ok(Some(snapshot))
}

pub fn save_source_index(path: &Path, snapshot: &SourceIndexSnapshot) -> Result<()> {
    let payload = serde_json::to_vec_pretty(snapshot)?;
    fs::write(path, payload).map_err(|source| CanopyError::io(path, source))
}

pub struct SourceIndexDaemon {
    stop_flag: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl SourceIndexDaemon {
    pub fn start(repository_root: PathBuf, output_path: PathBuf, interval: Duration) -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let thread_flag = stop_flag.clone();
        let handle = thread::spawn(move || loop {
            if thread_flag.load(Ordering::SeqCst) {
                break;
            }
            let snapshot = SourceIndexSnapshot::build(&repository_root);
            let _ = save_source_index(&output_path, &snapshot);
            thread::sleep(interval);
        });
        Self {
            stop_flag,
            handle: Some(handle),
        }
    }
}

impl Drop for SourceIndexDaemon {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn computes_changed_files_between_snapshots() {
        let temp = TempDir::new().expect("temp");
        fs::create_dir_all(temp.path().join("src")).expect("src");
        fs::write(temp.path().join("src/main.rs"), "fn main() {}\n").expect("main");
        let before = SourceIndexSnapshot::build(temp.path());

        fs::write(
            temp.path().join("src/main.rs"),
            "fn main() { println!(\"x\"); }\n",
        )
        .expect("main update");
        fs::write(temp.path().join("src/new.rs"), "pub fn run() {}\n").expect("new");
        let after = SourceIndexSnapshot::build(temp.path());
        let changed = after.changed_files_since(Some(&before));
        let changed_paths = changed
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<String>>();
        assert!(changed_paths.iter().any(|path| path == "src/main.rs"));
        assert!(changed_paths.iter().any(|path| path == "src/new.rs"));
    }
}
