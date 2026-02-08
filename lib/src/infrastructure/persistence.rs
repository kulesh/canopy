use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use super::mapping_policy::C4MappingPolicy;
use crate::domain::{ArchitectureGraph, EditLogEntry, ExportFormat, QueryAnswer};
use crate::error::{CanopyError, Result};

#[derive(Debug, Clone)]
pub struct PersistenceStore {
    pub canopy_dir: PathBuf,
}

impl PersistenceStore {
    pub fn new(repo_root: &Path) -> Result<Self> {
        let canopy_dir = repo_root.join(".canopy");
        fs::create_dir_all(&canopy_dir).map_err(|source| CanopyError::io(&canopy_dir, source))?;
        Ok(Self { canopy_dir })
    }

    pub fn c4_model_path(&self) -> PathBuf {
        self.canopy_dir.join("c4_model.json")
    }

    pub fn edit_log_path(&self) -> PathBuf {
        self.canopy_dir.join("edit_log.jsonl")
    }

    pub fn query_history_path(&self) -> PathBuf {
        self.canopy_dir.join("query_history.jsonl")
    }

    pub fn mapping_policy_path(&self) -> PathBuf {
        self.canopy_dir.join("mapping_policy.json")
    }

    pub fn save_graph(&self, graph: &ArchitectureGraph) -> Result<()> {
        let path = self.c4_model_path();
        let payload = serde_json::to_vec_pretty(graph)?;
        fs::write(&path, payload).map_err(|source| CanopyError::io(path, source))
    }

    pub fn load_graph(&self) -> Result<Option<ArchitectureGraph>> {
        let path = self.c4_model_path();
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(|source| CanopyError::io(path, source))?;
        let graph = serde_json::from_slice::<ArchitectureGraph>(&bytes)?;
        Ok(Some(graph))
    }

    pub fn append_edit(&self, edit: &EditLogEntry) -> Result<()> {
        let path = self.edit_log_path();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|source| CanopyError::io(path.clone(), source))?;
        let line = serde_json::to_string(edit)?;
        writeln!(file, "{line}").map_err(|source| CanopyError::io(path, source))
    }

    pub fn read_edits(&self) -> Result<Vec<EditLogEntry>> {
        let path = self.edit_log_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(&path).map_err(|source| CanopyError::io(path.clone(), source))?;
        let reader = BufReader::new(file);

        let mut entries = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(|source| CanopyError::io(path.clone(), source))?;
            if line.trim().is_empty() {
                continue;
            }
            entries.push(serde_json::from_str::<EditLogEntry>(&line)?);
        }

        Ok(entries)
    }

    pub fn append_query(&self, query: &QueryAnswer) -> Result<()> {
        let path = self.query_history_path();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|source| CanopyError::io(path.clone(), source))?;
        let line = serde_json::to_string(query)?;
        writeln!(file, "{line}").map_err(|source| CanopyError::io(path, source))
    }

    pub fn save_mapping_policy(&self, policy: &C4MappingPolicy) -> Result<()> {
        let path = self.mapping_policy_path();
        let payload = serde_json::to_vec_pretty(policy)?;
        fs::write(&path, payload).map_err(|source| CanopyError::io(path, source))
    }

    pub fn load_mapping_policy(&self) -> Result<Option<C4MappingPolicy>> {
        let path = self.mapping_policy_path();
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(|source| CanopyError::io(path, source))?;
        let policy = serde_json::from_slice::<C4MappingPolicy>(&bytes)?;
        Ok(Some(policy))
    }

    pub fn read_queries(&self) -> Result<Vec<QueryAnswer>> {
        let path = self.query_history_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(&path).map_err(|source| CanopyError::io(path.clone(), source))?;
        let reader = BufReader::new(file);

        let mut entries = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(|source| CanopyError::io(path.clone(), source))?;
            if line.trim().is_empty() {
                continue;
            }
            entries.push(serde_json::from_str::<QueryAnswer>(&line)?);
        }

        Ok(entries)
    }

    pub fn export_edit_log(&self, format: ExportFormat) -> Result<PathBuf> {
        let edits = self.read_edits()?;
        let path = format.output_path(self.canopy_dir.join("edit_log_export"));
        match format {
            ExportFormat::Json => {
                let payload = serde_json::to_vec_pretty(&edits)?;
                fs::write(&path, payload)
                    .map_err(|source| CanopyError::io(path.clone(), source))?;
            }
            ExportFormat::Jsonl => {
                let mut file =
                    File::create(&path).map_err(|source| CanopyError::io(path.clone(), source))?;
                for edit in edits {
                    writeln!(file, "{}", serde_json::to_string(&edit)?)
                        .map_err(|source| CanopyError::io(path.clone(), source))?;
                }
            }
        }
        Ok(path)
    }
}
