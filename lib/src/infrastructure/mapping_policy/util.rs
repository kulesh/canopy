use std::fs;
use std::path::Path;

use crate::error::{CanopyError, Result};

pub fn read_purpose_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| CanopyError::io(path, source))
}

pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string()
}
