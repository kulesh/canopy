use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::{json, Value};

use crate::error::{CanopyError, Result};

#[derive(Debug, Clone)]
pub struct DiagnosticsLogger {
    path: PathBuf,
}

impl DiagnosticsLogger {
    pub fn open(canopy_dir: &Path) -> Result<Self> {
        let logs_dir = canopy_dir.join("logs");
        fs::create_dir_all(&logs_dir).map_err(|source| CanopyError::io(&logs_dir, source))?;
        Ok(Self {
            path: logs_dir.join("diagnostics.jsonl"),
        })
    }

    pub fn log(&self, event: &str, payload: Value) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|source| CanopyError::io(&self.path, source))?;
        let row = json!({
            "timestamp": Utc::now().to_rfc3339(),
            "event": event,
            "payload": payload,
        });
        writeln!(file, "{row}").map_err(|source| CanopyError::io(&self.path, source))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
