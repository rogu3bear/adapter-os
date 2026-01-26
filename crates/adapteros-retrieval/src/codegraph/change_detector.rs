//! Change detection utilities for ephemeral adapters.
//!
//! This module inspects the Git history of a repository and synthesises
//! change descriptors that downstream components can translate into
//! ephemeral LoRA adapters.  The analysis is intentionally lightweight
//! yet deterministic so that the same set of commits always yields the
//! same change ordering and TTL recommendations.

use adapteros_core::{AosError, Result};
use git2::{Diff, DiffFormat, DiffOptions, Patch, Repository};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Description of a change that may warrant an ephemeral adapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DetectedChange {
    pub path: PathBuf,
    pub change_type: ChangeType,
    pub impacted_symbols: Vec<String>,
    pub impact_score: f32,
    pub suggested_rank: u8,
    pub ttl_hours: u64,
    pub commit_id: String,
}

/// Simplified change type derived from git delta status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// Analyse the latest commits (up to `max_commits`) and return change
/// descriptors ordered by commit recency.
pub fn detect_recent_changes(repo_path: &Path, max_commits: usize) -> Result<Vec<DetectedChange>> {
    let repo = Repository::open(repo_path).map_err(|e| AosError::Io(e.to_string()))?;
    let mut revwalk = repo.revwalk().map_err(|e| AosError::Io(e.to_string()))?;
    revwalk
        .push_head()
        .map_err(|e| AosError::Io(e.to_string()))?;
    let _ = revwalk.set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL);

    let mut changes = Vec::new();

    for (idx, oid_result) in revwalk.enumerate() {
        if idx >= max_commits {
            break;
        }
        let oid = oid_result.map_err(|e| AosError::Io(e.to_string()))?;
        let commit = repo
            .find_commit(oid)
            .map_err(|e| AosError::Io(e.to_string()))?;
        let parent_tree = if commit.parent_count() == 0 {
            None
        } else {
            Some(
                commit
                    .parent(0)
                    .map_err(|e| AosError::Io(e.to_string()))?
                    .tree()
                    .map_err(|e| AosError::Io(e.to_string()))?,
            )
        };
        let tree = commit.tree().map_err(|e| AosError::Io(e.to_string()))?;

        let mut opts = DiffOptions::new();
        opts.include_typechange(true);
        let diff = repo
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))
            .map_err(|e| AosError::Io(e.to_string()))?;

        collect_changes_from_diff(&diff, &mut changes, &commit.id().to_string());
    }

    changes.sort_by(|a, b| b.commit_id.cmp(&a.commit_id).then(a.path.cmp(&b.path)));
    debug!("detected_changes = {:?}", changes);
    Ok(changes)
}

fn collect_changes_from_diff(diff: &Diff, out: &mut Vec<DetectedChange>, commit_id: &str) {
    for (idx, delta) in diff.deltas().enumerate() {
        let change_type = match delta.status() {
            git2::Delta::Added => ChangeType::Added,
            git2::Delta::Deleted => ChangeType::Deleted,
            git2::Delta::Renamed => ChangeType::Renamed,
            _ => ChangeType::Modified,
        };

        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .unwrap_or_else(|| std::path::Path::new(""))
            .to_path_buf();

        let mut impacted_symbols: BTreeSet<String> = BTreeSet::new();
        let mut additions = 0u32;
        let mut deletions = 0u32;

        if let Ok(Some(patch)) = Patch::from_diff(diff, idx) {
            let hunk_count = patch.num_hunks();
            for hunk_idx in 0..hunk_count {
                if let Ok(line_total) = patch.num_lines_in_hunk(hunk_idx) {
                    for line_idx in 0..line_total {
                        if let Ok(line) = patch.line_in_hunk(hunk_idx, line_idx) {
                            match line.origin() {
                                '+' => additions += 1,
                                '-' => deletions += 1,
                                _ => {}
                            }
                            if let Ok(text) = std::str::from_utf8(line.content()) {
                                extract_symbols_from_line(text, &mut impacted_symbols);
                            }
                        }
                    }
                }
            }
        } else {
            // Fallback: print diff to capture impacted symbols even if patch failed.
            let _ = diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
                if let Ok(text) = std::str::from_utf8(line.content()) {
                    extract_symbols_from_line(text, &mut impacted_symbols);
                }
                true
            });
        }

        let symbol_count = impacted_symbols.len() as f32;
        let churn = additions as f32 + deletions as f32;
        let impact_score = ((symbol_count * 0.4) + (churn.min(200.0) / 200.0)).min(1.0);
        let suggested_rank = (8.0 - (impact_score * 4.0)).round().clamp(4.0, 8.0) as u8;
        let ttl_hours = 24 + ((impact_score * 48.0).round() as u64);

        out.push(DetectedChange {
            path,
            change_type,
            impacted_symbols: impacted_symbols.into_iter().collect(),
            impact_score: (impact_score * 100.0).round() / 100.0,
            suggested_rank,
            ttl_hours: ttl_hours.clamp(24, 72),
            commit_id: commit_id.to_string(),
        });
    }
}

fn extract_symbols_from_line(line: &str, out: &mut BTreeSet<String>) {
    const PATTERNS: &[&str] = &[
        "fn ",
        "def ",
        "class ",
        "pub fn ",
        "async fn ",
        "async def ",
        "function ",
        "const ",
    ];

    for pattern in PATTERNS {
        if let Some(idx) = line.find(pattern) {
            let start = idx + pattern.len();
            let name: String = line[start..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            if !name.is_empty() {
                out.insert(name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn commit_all(repo: &Repository, message: &str) {
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("tester", "tester@example.com").unwrap();

        let parents: Vec<_> = repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .map_or_else(Vec::new, |oid| vec![repo.find_commit(oid).unwrap()]);

        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            message,
            &tree,
            parent_refs.as_slice(),
        )
        .unwrap();
    }

    #[test]
    fn detects_function_change() {
        let dir = tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();

        write_file(&dir.path().join("src/lib.rs"), "pub fn original() {}\n");
        commit_all(&repo, "initial");

        write_file(
            &dir.path().join("src/lib.rs"),
            "pub fn original() {}\npub fn added() {}\n",
        );
        commit_all(&repo, "add function");

        let changes = detect_recent_changes(dir.path(), 2).unwrap();
        assert!(changes.iter().any(|c| {
            c.path == Path::new("src/lib.rs")
                && c.impacted_symbols.iter().any(|s| s == "added")
                && c.ttl_hours >= 24
        }));
    }
}
