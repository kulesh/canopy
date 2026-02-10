use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::Path;

use ignore::WalkBuilder;

use crate::domain::Repository;
use crate::error::Result;

use super::types::SourceTreeSnapshot;
use super::util::normalize_path;

const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "py", "pyi", "js", "mjs", "cjs", "ts", "tsx", "jsx", "go", "java", "kt", "scala", "c",
    "cpp", "h", "hpp", "cc", "hh", "cs", "swift", "rb", "php", "lua", "sql", "sh", "bash", "zsh",
    "fish", "ps1",
];

pub fn collect_source_tree_snapshot(repository: &Repository) -> Result<SourceTreeSnapshot> {
    let mut walker = WalkBuilder::new(&repository.root);
    walker.hidden(true);
    walker.git_ignore(true);
    walker.git_exclude(true);
    walker.parents(true);

    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();

    for entry in walker.build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        let path = entry.path();
        if !is_first_pass_scope_file(path) {
            continue;
        }

        let rel = match path.strip_prefix(&repository.root) {
            Ok(rel) => normalize_path(&rel.to_string_lossy()),
            Err(_) => continue,
        };
        files.insert(rel.clone());

        let mut parent = Path::new(&rel).parent();
        while let Some(dir) = parent {
            if dir.as_os_str().is_empty() {
                break;
            }
            directories.insert(normalize_path(&dir.to_string_lossy()));
            parent = dir.parent();
        }
    }

    Ok(SourceTreeSnapshot {
        repository_name: repository.name.clone(),
        files: files.into_iter().collect(),
        directories: directories.into_iter().collect(),
    })
}

pub fn is_source_file(path: &Path) -> bool {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| SOURCE_EXTENSIONS.contains(&ext))
        .unwrap_or(false)
    {
        return true;
    }

    is_shebang_script(path)
}

pub fn is_first_pass_scope_file(path: &Path) -> bool {
    is_source_file(path)
}

pub fn is_first_pass_excluded_path(path: &Path) -> bool {
    let _ = path;
    false
}

fn is_shebang_script(path: &Path) -> bool {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return false,
    };
    if !metadata.is_file() || metadata.len() > 1_048_576 {
        return false;
    }

    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut buf = [0u8; 256];
    let read = match file.read(&mut buf) {
        Ok(read) => read,
        Err(_) => return false,
    };
    if read < 3 || &buf[..2] != b"#!" {
        return false;
    }

    let head = String::from_utf8_lossy(&buf[..read]).to_lowercase();
    [
        "python", "bash", "sh", "zsh", "fish", "node", "ruby", "perl", "php", "pwsh",
    ]
    .iter()
    .any(|interpreter| head.contains(interpreter))
}

#[cfg(test)]
mod tests {
    use super::is_first_pass_excluded_path;
    use std::path::Path;

    #[test]
    fn strict_scope_does_not_preemptively_exclude_tests_or_fixtures() {
        assert!(!is_first_pass_excluded_path(Path::new(
            "src/tests/user_test.rs"
        )));
        assert!(!is_first_pass_excluded_path(Path::new(
            "backend/__tests__/auth.ts"
        )));
    }

    #[test]
    fn strict_scope_keeps_implementation_paths() {
        assert!(!is_first_pass_excluded_path(Path::new(
            "src/domain/auth.rs"
        )));
        assert!(!is_first_pass_excluded_path(Path::new(
            "backend/catsyphon/api/app.py"
        )));
    }
}
