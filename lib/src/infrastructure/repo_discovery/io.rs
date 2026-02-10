use std::fs;
use std::path::Path;

use crate::error::{CanopyError, Result};

pub(super) fn read_source_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| CanopyError::io(path, source))
}
