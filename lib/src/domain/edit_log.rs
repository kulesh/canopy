use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::graph::Provenance;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HumanEdit {
    pub component_path: String,
    pub field: String,
    pub before: String,
    pub after: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditLogEntry {
    pub timestamp: DateTime<Utc>,
    pub author: String,
    pub component_path: String,
    pub field: String,
    pub before: String,
    pub after: String,
    pub reason: Option<String>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Jsonl,
}

impl ExportFormat {
    pub fn output_path(self, base_path: PathBuf) -> PathBuf {
        match self {
            Self::Json => base_path.with_extension("json"),
            Self::Jsonl => base_path.with_extension("jsonl"),
        }
    }
}
