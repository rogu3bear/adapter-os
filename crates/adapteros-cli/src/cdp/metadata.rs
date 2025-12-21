//! CDP metadata extraction and management

use adapteros_core::{AosError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Metadata for a Commit Delta Pack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpMetadata {
    /// Commit author email
    pub author: String,
    /// Commit message
    pub message: String,
    /// Commit timestamp
    pub timestamp: DateTime<Utc>,
    /// Branch name
    pub branch: String,
    /// Remote repository URL (if available)
    pub remote_url: Option<String>,
    /// Repository root path
    pub repo_path: PathBuf,
    /// Commit author name (if available)
    pub author_name: Option<String>,
    /// Commit committer email (if different from author)
    pub committer: Option<String>,
    /// Commit committer name (if available)
    pub committer_name: Option<String>,
}

impl CdpMetadata {
    /// Create new CDP metadata
    pub fn new(
        author: String,
        message: String,
        timestamp: DateTime<Utc>,
        branch: String,
        repo_path: PathBuf,
    ) -> Self {
        Self {
            author,
            message,
            timestamp,
            branch,
            remote_url: None,
            repo_path,
            author_name: None,
            committer: None,
            committer_name: None,
        }
    }

    /// Set remote URL
    pub fn with_remote_url(mut self, remote_url: String) -> Self {
        self.remote_url = Some(remote_url);
        self
    }

    /// Set author name
    pub fn with_author_name(mut self, author_name: String) -> Self {
        self.author_name = Some(author_name);
        self
    }

    /// Set committer information
    pub fn with_committer(mut self, committer: String, committer_name: Option<String>) -> Self {
        self.committer = Some(committer);
        self.committer_name = committer_name;
        self
    }

    /// Get display name for author (name if available, otherwise email)
    pub fn author_display_name(&self) -> &str {
        self.author_name.as_deref().unwrap_or(&self.author)
    }

    /// Get display name for committer (name if available, otherwise email)
    pub fn committer_display_name(&self) -> Option<&str> {
        if let Some(ref committer_name) = self.committer_name {
            Some(committer_name)
        } else if let Some(ref committer) = self.committer {
            Some(committer)
        } else {
            None
        }
    }

    /// Check if author and committer are different
    pub fn has_different_committer(&self) -> bool {
        if let Some(ref committer) = self.committer {
            committer != &self.author
        } else {
            false
        }
    }

    /// Extract repository name from remote URL
    pub fn repo_name(&self) -> Option<String> {
        self.remote_url.as_ref().and_then(|url| {
            // Extract repo name from common git URL formats
            if let Some(stripped) = url.strip_suffix(".git") {
                url = stripped;
            }
            
            if url.contains("github.com") || url.contains("gitlab.com") || url.contains("bitbucket.org") {
                url.split('/').last().map(|s| s.to_string())
            } else {
                None
            }
        })
    }

    /// Get a short description of the commit
    pub fn short_description(&self) -> String {
        // Take first line of commit message, truncate if too long
        let first_line = self.message.lines().next().unwrap_or("");
        if first_line.len() > 80 {
            format!("{}...", &first_line[..77])
        } else {
            first_line.to_string()
        }
    }
}

/// Extract metadata from git repository
pub struct MetadataExtractor {
    repo_path: PathBuf,
}

impl MetadataExtractor {
    /// Create a new metadata extractor
    pub fn new<P: AsRef<std::path::Path>>(repo_path: P) -> Self {
        Self {
            repo_path: repo_path.as_ref().to_path_buf(),
        }
    }

    /// Extract metadata for a specific commit
    pub fn extract_for_commit(&self, commit_sha: &str) -> Result<CdpMetadata> {
        use std::process::Command;

        // Get commit information using git log
        let output = Command::new("git")
            .arg("log")
            .arg("-1")
            .arg("--pretty=format:%H|%an|%ae|%ad|%cn|%ce|%cd|%s")
            .arg("--date=iso")
            .arg(commit_sha)
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| AosError::Other(format!("Failed to run git log: {}", e)))?;

        if !output.status.success() {
            return Err(AosError::Other(format!(
                "Git log failed for commit {}: {}",
                commit_sha,
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let log_line = String::from_utf8(output.stdout)
            .map_err(|e| AosError::Other(format!("Invalid git log output: {}", e)))?;

        self.parse_git_log_line(&log_line, commit_sha)
    }

    /// Parse git log output line
    fn parse_git_log_line(&self, log_line: &str, commit_sha: &str) -> Result<CdpMetadata> {
        let parts: Vec<&str> = log_line.split('|').collect();
        if parts.len() != 8 {
            return Err(AosError::Other(format!(
                "Invalid git log format for commit {}: expected 8 parts, got {}",
                commit_sha,
                parts.len()
            )));
        }

        let [hash, author_name, author_email, author_date, committer_name, committer_email, committer_date, message] = 
            [parts[0], parts[1], parts[2], parts[3], parts[4], parts[5], parts[6], parts[7]];

        // Parse timestamps
        let timestamp = DateTime::parse_from_rfc3339(author_date)
            .map_err(|e| AosError::Other(format!("Invalid author date: {}", e)))?
            .with_timezone(&Utc);

        // Get current branch
        let branch = self.get_current_branch()?;

        // Get remote URL
        let remote_url = self.get_remote_url().ok();

        let mut metadata = CdpMetadata::new(
            author_email.to_string(),
            message.to_string(),
            timestamp,
            branch,
            self.repo_path.clone(),
        )
        .with_author_name(author_name.to_string())
        .with_remote_url(remote_url.unwrap_or_default());

        // Set committer if different from author
        if committer_email != author_email {
            metadata = metadata.with_committer(
                committer_email.to_string(),
                Some(committer_name.to_string()),
            );
        }

        Ok(metadata)
    }

    /// Get current branch name
    fn get_current_branch(&self) -> Result<String> {
        use std::process::Command;

        let output = Command::new("git")
            .arg("branch")
            .arg("--show-current")
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| AosError::Other(format!("Failed to get current branch: {}", e)))?;

        if !output.status.success() {
            return Err(AosError::Other(format!(
                "Failed to get current branch: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let branch = String::from_utf8(output.stdout)
            .map_err(|e| AosError::Other(format!("Invalid branch output: {}", e)))?
            .trim()
            .to_string();

        if branch.is_empty() {
            Ok("detached".to_string())
        } else {
            Ok(branch)
        }
    }

    /// Get remote URL
    fn get_remote_url(&self) -> Result<String> {
        use std::process::Command;

        let output = Command::new("git")
            .arg("remote")
            .arg("get-url")
            .arg("origin")
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| AosError::Other(format!("Failed to get remote URL: {}", e)))?;

        if !output.status.success() {
            return Err(AosError::Other(format!(
                "Failed to get remote URL: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(String::from_utf8(output.stdout)
            .map_err(|e| AosError::Other(format!("Invalid remote URL output: {}", e)))?
            .trim()
            .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_metadata_creation() {
        let temp_dir = TempDir::new_in(".").unwrap();
        let metadata = CdpMetadata::new(
            "test@example.com".to_string(),
            "Test commit message".to_string(),
            Utc::now(),
            "main".to_string(),
            temp_dir.path().to_path_buf(),
        );

        assert_eq!(metadata.author, "test@example.com");
        assert_eq!(metadata.message, "Test commit message");
        assert_eq!(metadata.branch, "main");
        assert_eq!(metadata.author_display_name(), "test@example.com");
    }

    #[test]
    fn test_metadata_with_author_name() {
        let temp_dir = TempDir::new_in(".").unwrap();
        let metadata = CdpMetadata::new(
            "test@example.com".to_string(),
            "Test commit message".to_string(),
            Utc::now(),
            "main".to_string(),
            temp_dir.path().to_path_buf(),
        )
        .with_author_name("Test Author".to_string());

        assert_eq!(metadata.author_display_name(), "Test Author");
    }

    #[test]
    fn test_metadata_with_committer() {
        let temp_dir = TempDir::new_in(".").unwrap();
        let metadata = CdpMetadata::new(
            "author@example.com".to_string(),
            "Test commit message".to_string(),
            Utc::now(),
            "main".to_string(),
            temp_dir.path().to_path_buf(),
        )
        .with_committer("committer@example.com".to_string(), Some("Test Committer".to_string()));

        assert!(metadata.has_different_committer());
        assert_eq!(metadata.committer_display_name(), Some("Test Committer"));
    }

    #[test]
    fn test_short_description() {
        let temp_dir = TempDir::new_in(".").unwrap();
        let metadata = CdpMetadata::new(
            "test@example.com".to_string(),
            "This is a very long commit message that should be truncated when displayed in short format".to_string(),
            Utc::now(),
            "main".to_string(),
            temp_dir.path().to_path_buf(),
        );

        let short = metadata.short_description();
        assert!(short.len() <= 80);
        assert!(short.ends_with("..."));
    }

    #[test]
    fn test_repo_name_extraction() {
        let temp_dir = TempDir::new_in(".").unwrap();
        let metadata = CdpMetadata::new(
            "test@example.com".to_string(),
            "Test commit".to_string(),
            Utc::now(),
            "main".to_string(),
            temp_dir.path().to_path_buf(),
        )
        .with_remote_url("https://github.com/user/repo-name.git".to_string());

        assert_eq!(metadata.repo_name(), Some("repo-name".to_string()));
    }
}
