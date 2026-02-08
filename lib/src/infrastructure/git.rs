use std::collections::BTreeMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use git2::{DiffOptions, Repository};

use crate::error::Result;

#[derive(Debug, Clone, Default)]
pub struct GitSignals {
    pub recent_changes: BTreeMap<String, DateTime<Utc>>,
    pub churn_count: BTreeMap<String, usize>,
    pub blame_author: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct BranchDiffSummary {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
}

pub fn collect_git_signals(repo_root: &Path, max_commits: usize) -> Result<GitSignals> {
    let repo = Repository::discover(repo_root)?;
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;

    let mut signals = GitSignals::default();

    for oid_result in revwalk.take(max_commits) {
        let oid = match oid_result {
            Ok(oid) => oid,
            Err(_) => continue,
        };
        let commit = match repo.find_commit(oid) {
            Ok(commit) => commit,
            Err(_) => continue,
        };

        let timestamp =
            DateTime::<Utc>::from_timestamp(commit.time().seconds(), 0).unwrap_or_else(Utc::now);

        let tree = match commit.tree() {
            Ok(tree) => tree,
            Err(_) => continue,
        };

        let parent_tree = if commit.parent_count() > 0 {
            commit.parent(0).ok().and_then(|p| p.tree().ok())
        } else {
            None
        };

        let mut diff_opts = DiffOptions::new();
        let diff =
            repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut diff_opts))?;

        for delta in diff.deltas() {
            if let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) {
                let rel = path.to_string_lossy().to_string();
                signals
                    .churn_count
                    .entry(rel.clone())
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
                signals
                    .recent_changes
                    .entry(rel)
                    .and_modify(|ts| {
                        if timestamp > *ts {
                            *ts = timestamp;
                        }
                    })
                    .or_insert(timestamp);
            }
        }
    }

    for rel in signals.recent_changes.keys() {
        let path = Path::new(rel);
        if let Ok(Some(author)) = blame_for_file(repo_root, path) {
            signals.blame_author.insert(rel.clone(), author);
        }
    }

    Ok(signals)
}

pub fn blame_for_file(repo_root: &Path, relative_file: &Path) -> Result<Option<String>> {
    let repo = Repository::discover(repo_root)?;
    let blame = match repo.blame_file(relative_file, None) {
        Ok(blame) => blame,
        Err(_) => return Ok(None),
    };
    let first = blame.iter().next();
    Ok(first.and_then(|h| h.final_signature().name().map(ToString::to_string)))
}

pub fn compare_branches(
    repo_root: &Path,
    from_branch: &str,
    to_branch: &str,
) -> Result<BranchDiffSummary> {
    let repo = Repository::discover(repo_root)?;
    let from_obj = repo.revparse_single(from_branch)?;
    let to_obj = repo.revparse_single(to_branch)?;
    let from_tree = from_obj.peel_to_tree()?;
    let to_tree = to_obj.peel_to_tree()?;

    let diff = repo.diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)?;

    let mut summary = BranchDiffSummary::default();
    for delta in diff.deltas() {
        use git2::Delta;
        match delta.status() {
            Delta::Added => summary.added += 1,
            Delta::Deleted => summary.removed += 1,
            _ => summary.modified += 1,
        }
    }

    Ok(summary)
}
