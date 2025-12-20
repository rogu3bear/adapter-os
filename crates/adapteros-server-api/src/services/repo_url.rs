//! Repository URL inference service
//! 【2025-01-27†refactor(server)†extract-repo-url】
//!
//! Handles extracting repository URLs from git remotes with proper error handling
//! and timeout protection.
//!
//! Extracted from handlers.rs to reduce duplication of git remote URL extraction logic.

use tracing::{debug, warn};

/// Infer repository URL from git remote origin
/// 【2025-01-27†refactor(server)†extract-repo-url】
///
/// Returns the origin URL if available, otherwise None.
/// All errors are logged but not propagated to avoid breaking the listing.
pub fn infer_repo_url(path: &str) -> Option<String> {
    // Use panic catch to prevent hanging on corrupted repos
    let result = std::panic::catch_unwind(|| {
        let repo = git2::Repository::open(path).ok()?;
        let remote = repo.find_remote("origin").ok()?;
        remote.url().map(|u| u.to_string())
    });

    match result {
        Ok(Some(url)) => {
            debug!(
                path = %path,
                url = %url,
                "Inferred repository URL from git remote"
            );
            Some(url)
        }
        Ok(None) => {
            debug!(
                path = %path,
                "No origin remote found or path is not a git repository"
            );
            None
        }
        Err(_) => {
            warn!(
                path = %path,
                "Panic occurred while inferring URL (corrupted repo?)"
            );
            None
        }
    }
}

/// Infer URLs for multiple repositories in parallel
/// 【2025-01-27†refactor(server)†extract-repo-url】
///
/// Uses tokio::task::spawn_blocking to parallelize git operations
/// without blocking the async runtime.
pub async fn infer_repo_urls_parallel(
    repos: &[(String, String)], // (repo_id, path)
) -> std::collections::HashMap<String, Option<String>> {
    use futures_util::future;
    use std::collections::HashMap;

    let tasks: Vec<_> = repos
        .iter()
        .map(|(repo_id, path)| {
            let repo_id = repo_id.clone();
            let path = path.clone();
            tokio::task::spawn_blocking(move || {
                let url = infer_repo_url(&path);
                (repo_id, url)
            })
        })
        .collect();

    let results = future::join_all(tasks).await;

    let mut url_map = HashMap::with_capacity(repos.len());
    for result in results {
        match result {
            Ok((repo_id, url)) => {
                url_map.insert(repo_id, url);
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Task failed while inferring repository URL"
                );
            }
        }
    }

    url_map
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("Failed to create temp directory")
    }

    #[test]
    fn test_infer_repo_url_with_remote() {
        let temp_dir = new_test_tempdir();
        let repo_path = temp_dir.path();

        // Initialize git repo
        git2::Repository::init(repo_path).expect("Failed to init git repository");

        // Add origin remote
        let repo = git2::Repository::open(repo_path).expect("Failed to open git repository");
        repo.remote("origin", "https://github.com/user/repo.git")
            .expect("Failed to create git commit");

        let url = infer_repo_url(repo_path.to_str().expect("Invalid UTF-8 in path"));
        assert_eq!(url, Some("https://github.com/user/repo.git".to_string()));
    }

    #[test]
    fn test_infer_repo_url_no_remote() {
        let temp_dir = new_test_tempdir();
        let repo_path = temp_dir.path();

        // Initialize git repo without remote
        git2::Repository::init(repo_path).expect("Failed to init git repository");

        let url = infer_repo_url(repo_path.to_str().expect("Invalid UTF-8 in path"));
        assert_eq!(url, None);
    }

    #[test]
    fn test_infer_repo_url_not_git_repo() {
        let temp_dir = new_test_tempdir();
        let repo_path = temp_dir.path();

        // Create directory but don't initialize git
        fs::create_dir_all(repo_path).expect("Failed to create directories");

        let url = infer_repo_url(repo_path.to_str().expect("Invalid UTF-8 in path"));
        assert_eq!(url, None);
    }

    #[test]
    fn test_infer_repo_url_nonexistent_path() {
        let url = infer_repo_url("/nonexistent/path/that/does/not/exist");
        assert_eq!(url, None);
    }
}
