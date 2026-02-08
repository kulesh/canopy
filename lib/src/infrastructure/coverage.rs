use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::{CanopyError, Result};

#[derive(Debug, Clone, Default)]
pub struct CoverageStats {
    pub hit_lines: usize,
    pub total_lines: usize,
}

impl CoverageStats {
    pub fn percent(&self) -> f32 {
        if self.total_lines == 0 {
            0.0
        } else {
            (self.hit_lines as f32 / self.total_lines as f32) * 100.0
        }
    }
}

pub type CoverageMap = BTreeMap<String, CoverageStats>;

pub fn parse_lcov(path: &Path) -> Result<CoverageMap> {
    let contents = fs::read_to_string(path).map_err(|source| CanopyError::io(path, source))?;
    let mut map = CoverageMap::new();

    let mut current_file: Option<String> = None;
    for line in contents.lines() {
        if let Some(file) = line.strip_prefix("SF:") {
            current_file = Some(file.to_string());
            map.entry(file.to_string()).or_default();
            continue;
        }

        if let Some(exec) = line.strip_prefix("DA:") {
            if let Some(file) = &current_file {
                let mut parts = exec.split(',');
                let _line_no = parts.next();
                let hits = parts
                    .next()
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                let stats = map.entry(file.clone()).or_default();
                stats.total_lines += 1;
                if hits > 0 {
                    stats.hit_lines += 1;
                }
            }
        }
    }

    Ok(map)
}
