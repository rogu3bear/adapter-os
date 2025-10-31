//! Commit Delta Pack (CDP) core types and functionality
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_git::{ChangedSymbol, DiffSummary};
use adapteros_lora_worker::{LinterResult, TestResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Unique identifier for a Commit Delta Pack
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CdpId(String);

impl CdpId {
    /// Create a new CDP identifier from repo and commit identifiers.
    pub fn new(repo_id: impl AsRef<str>, commit_sha: impl AsRef<str>) -> Self {
        let normalized = format!("{}::{}", repo_id.as_ref(), commit_sha.as_ref());
        let hash = B3Hash::hash(normalized.as_bytes()).to_hex();
        Self(format!("cdp_{}", hash))
    }

    /// Construct a CDP identifier from a pre-existing string.
    pub fn from_raw(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrow the underlying identifier string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the identifier and return the owned string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for CdpId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for CdpId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<CdpId> for String {
    fn from(value: CdpId) -> Self {
        value.0
    }
}

impl From<&CdpId> for String {
    fn from(value: &CdpId) -> Self {
        value.0.clone()
    }
}

/// Metadata for a Commit Delta Pack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpMetadata {
    pub repo_id: String,
    pub commit_sha: String,
    pub timestamp: DateTime<Utc>,
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committer_email: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_files: Vec<String>,
}

impl CdpMetadata {
    /// Construct metadata with required commit fields.
    pub fn new(
        repo_id: impl Into<String>,
        commit_sha: impl Into<String>,
        timestamp: DateTime<Utc>,
        author: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            repo_id: repo_id.into(),
            commit_sha: commit_sha.into(),
            timestamp,
            author: author.into(),
            author_email: None,
            committer: None,
            committer_email: None,
            message: message.into(),
            branch: String::new(),
            remote_url: None,
            changed_files: Vec::new(),
        }
    }

    /// Attach the author email address if available.
    pub fn with_author_email(mut self, email: impl Into<String>) -> Self {
        self.author_email = Some(email.into());
        self
    }

    /// Attach committer metadata if available.
    pub fn with_committer(
        mut self,
        name: impl Into<String>,
        email: Option<impl Into<String>>,
    ) -> Self {
        self.committer = Some(name.into());
        self.committer_email = email.map(|e| e.into());
        self
    }

    /// Record the branch associated with the commit.
    pub fn with_branch(mut self, branch: impl Into<String>) -> Self {
        self.branch = branch.into();
        self
    }

    /// Attach the remote URL for the repository if present.
    pub fn with_remote_url(mut self, remote_url: Option<impl Into<String>>) -> Self {
        self.remote_url = remote_url.map(|url| url.into());
        self
    }

    /// Attach the list of files changed in the commit.
    pub fn with_changed_files<I, S>(mut self, files: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.changed_files = files.into_iter().map(|f| f.into()).collect();
        self
    }

    /// Return the first line of the commit message for summary displays.
    pub fn short_description(&self) -> String {
        self.message
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string()
    }
}

/// Extract metadata from git repository
pub struct MetadataExtractor {
    repo_path: PathBuf,
}

impl MetadataExtractor {
    pub fn new(repo_path: &Path) -> Self {
        Self {
            repo_path: repo_path.to_path_buf(),
        }
    }

    /// Extract metadata for the provided commit.
    pub fn extract_for_commit(
        &self,
        repo_id: &str,
        commit_sha: &str,
        parent_sha: Option<&str>,
    ) -> Result<CdpMetadata> {
        let timestamp = self.read_timestamp(commit_sha)?;
        let author = self.read_field(commit_sha, "%an")?;
        let author_email = self.optional_field(commit_sha, "%ae")?;
        let committer = self.optional_field(commit_sha, "%cn")?;
        let committer_email = self.optional_field(commit_sha, "%ce")?;
        let message = self.read_message(commit_sha)?;
        let branch = self.find_branch(commit_sha)?;
        let remote_url = self.remote_url()?;
        let changed_files = self.changed_files(commit_sha, parent_sha)?;

        let mut metadata = CdpMetadata::new(repo_id, commit_sha, timestamp, author, message)
            .with_branch(branch.unwrap_or_default())
            .with_remote_url(remote_url)
            .with_changed_files(changed_files);

        if let Some(email) = author_email {
            metadata = metadata.with_author_email(email);
        }

        if let Some(name) = committer {
            metadata = metadata.with_committer(name, committer_email);
        }

        Ok(metadata)
    }

    fn read_timestamp(&self, commit_sha: &str) -> Result<DateTime<Utc>> {
        let raw = self.git(&["show", "-s", "--format=%ct", commit_sha])?;
        let seconds: i64 = raw
            .trim()
            .parse()
            .map_err(|e| AosError::Git(format!("Invalid commit timestamp: {e}")))?;

        DateTime::<Utc>::from_timestamp(seconds, 0)
            .ok_or_else(|| AosError::Git("Commit timestamp out of range".to_string()))
    }

    fn read_field(&self, commit_sha: &str, format: &str) -> Result<String> {
        Ok(self
            .git(&["show", "-s", &format!("--format={}", format), commit_sha])?
            .trim()
            .to_string())
    }

    fn optional_field(&self, commit_sha: &str, format: &str) -> Result<Option<String>> {
        let value = self.read_field(commit_sha, format)?;
        if value.is_empty() {
            Ok(None)
        } else {
            Ok(Some(value))
        }
    }

    fn read_message(&self, commit_sha: &str) -> Result<String> {
        Ok(self
            .git(&["show", "-s", "--format=%B", commit_sha])?
            .trim_end()
            .to_string())
    }

    fn find_branch(&self, commit_sha: &str) -> Result<Option<String>> {
        if let Some(out) = self.git_optional(&[
            "for-each-ref",
            &format!("--contains={}", commit_sha),
            "--format=%(refname:short)",
            "refs/heads",
        ])? {
            for line in out.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    return Ok(Some(trimmed.to_string()));
                }
            }
            Ok(None)
        } else {
            Ok(None)
        }
    }

    fn remote_url(&self) -> Result<Option<String>> {
        self.git_optional(&["remote", "get-url", "origin"])
            .map(|opt| {
                opt.map(|url| url.trim().to_string())
                    .filter(|url| !url.is_empty())
            })
    }

    fn changed_files(&self, commit_sha: &str, parent_sha: Option<&str>) -> Result<Vec<String>> {
        let args = if let Some(parent) = parent_sha {
            vec!["diff", "--name-only", parent, commit_sha]
        } else {
            vec![
                "diff-tree",
                "--no-commit-id",
                "--name-only",
                "-r",
                commit_sha,
            ]
        };

        let output = self.git(&args)?;
        let mut files: Vec<String> = output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| line.to_string())
            .collect();
        files.sort();
        files.dedup();
        Ok(files)
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| AosError::Git(format!("Failed to run git {:?}: {e}", args)))?;

        if !output.status.success() {
            return Err(AosError::Git(format!(
                "Git command {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        String::from_utf8(output.stdout)
            .map_err(|e| AosError::Git(format!("Invalid UTF-8 from git output: {e}")))
    }

    fn git_optional(&self, args: &[&str]) -> Result<Option<String>> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| AosError::Git(format!("Failed to run git {:?}: {e}", args)))?;

        if !output.status.success() {
            return Ok(None);
        }

        Ok(Some(String::from_utf8(output.stdout).map_err(|e| {
            AosError::Git(format!("Invalid UTF-8 from git output: {e}"))
        })?))
    }
}

/// A complete Commit Delta Pack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDeltaPack {
    pub cdp_id: CdpId,
    pub repo_id: String,
    pub commit_sha: String,
    pub parent_sha: String,
    pub diff_summary: DiffSummary,
    pub changed_symbols: Vec<ChangedSymbol>,
    pub metadata: CdpMetadata,
    pub test_results: Vec<TestResult>,
    pub linter_results: Vec<LinterResult>,
    pub content_hash: B3Hash,
}

impl CommitDeltaPack {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo_id: String,
        commit_sha: String,
        parent_sha: String,
        diff_summary: DiffSummary,
        changed_symbols: Vec<ChangedSymbol>,
        metadata: CdpMetadata,
        test_results: Vec<TestResult>,
        linter_results: Vec<LinterResult>,
    ) -> Result<Self> {
        let cdp_id = CdpId::new(&repo_id, &commit_sha);
        let mut pack = Self {
            cdp_id,
            repo_id,
            commit_sha,
            parent_sha,
            diff_summary,
            changed_symbols,
            metadata,
            test_results,
            linter_results,
            content_hash: B3Hash::hash(&[]),
        };

        pack.content_hash = pack.compute_content_hash()?;
        Ok(pack)
    }

    /// Recompute and return the deterministic content hash for the CDP.
    pub fn compute_content_hash(&self) -> Result<B3Hash> {
        let view = CommitDeltaPackHashView {
            cdp_id: self.cdp_id.as_str(),
            repo_id: &self.repo_id,
            commit_sha: &self.commit_sha,
            parent_sha: &self.parent_sha,
            diff_summary: &self.diff_summary,
            changed_symbols: &self.changed_symbols,
            metadata: &self.metadata,
            test_results: &self.test_results,
            linter_results: &self.linter_results,
        };

        let bytes = serde_json::to_vec(&view)?;
        Ok(B3Hash::hash(&bytes))
    }

    /// Check if any tests failed for this commit delta.
    pub fn has_test_failures(&self) -> bool {
        self.test_results
            .iter()
            .any(|result| result.failed > 0 || !result.failures.is_empty())
    }

    /// Check if any linter errors were reported for this commit delta.
    pub fn has_linter_issues(&self) -> bool {
        self.linter_results
            .iter()
            .any(|result| !result.errors.is_empty())
    }

    /// Total number of linter issues (errors + warnings).
    pub fn total_linter_issues(&self) -> usize {
        self.linter_results
            .iter()
            .map(|result| result.errors.len() + result.warnings.len())
            .sum()
    }
}

#[derive(Serialize)]
struct CommitDeltaPackHashView<'a> {
    cdp_id: &'a str,
    repo_id: &'a str,
    commit_sha: &'a str,
    parent_sha: &'a str,
    diff_summary: &'a DiffSummary,
    changed_symbols: &'a [ChangedSymbol],
    metadata: &'a CdpMetadata,
    test_results: &'a [TestResult],
    linter_results: &'a [LinterResult],
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn cdp_id_is_deterministic() {
        let id1 = CdpId::new("repo", "abc123");
        let id2 = CdpId::new("repo", "abc123");
        let id3 = CdpId::new("repo", "def456");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert!(id1.as_str().starts_with("cdp_"));
    }

    #[test]
    fn commit_delta_pack_hash_deterministic() {
        let diff_summary = DiffSummary::new();
        let metadata = CdpMetadata::new("repo", "abc123", Utc::now(), "author", "message");

        let pack_a = CommitDeltaPack::new(
            "repo".to_string(),
            "abc123".to_string(),
            "def456".to_string(),
            diff_summary.clone(),
            Vec::new(),
            metadata.clone(),
            Vec::new(),
            Vec::new(),
        )
        .expect("cdp creation should succeed");

        let pack_b = CommitDeltaPack::new(
            "repo".to_string(),
            "abc123".to_string(),
            "def456".to_string(),
            diff_summary,
            Vec::new(),
            metadata,
            Vec::new(),
            Vec::new(),
        )
        .expect("cdp creation should succeed");

        assert_eq!(pack_a.content_hash, pack_b.content_hash);
        assert!(!pack_a.has_test_failures());
        assert!(!pack_a.has_linter_issues());
    }

    #[test]
    fn metadata_extractor_reads_commit_information() -> Result<()> {
        let dir = tempdir().expect("tempdir");
        let repo_path = dir.path();

        run_git(repo_path, &["init", "--initial-branch=main"])?;
        run_git(repo_path, &["config", "user.email", "tester@example.com"])?;
        run_git(repo_path, &["config", "user.name", "Test User"])?;

        fs::write(repo_path.join("README.md"), "hello world")?;
        run_git(repo_path, &["add", "README.md"])?;
        run_git(repo_path, &["commit", "-m", "Initial commit"])?;

        fs::write(repo_path.join("README.md"), "hello world!\n")?;
        run_git(repo_path, &["add", "README.md"])?;
        run_git(repo_path, &["commit", "-m", "Update README"])?;

        let head = git_output(repo_path, &["rev-parse", "HEAD"])?;
        let parent = git_output(repo_path, &["rev-parse", "HEAD^"])?;

        let extractor = MetadataExtractor::new(repo_path);
        let metadata =
            extractor.extract_for_commit("example/repo", head.trim(), Some(parent.trim()))?;

        assert_eq!(metadata.repo_id, "example/repo");
        assert_eq!(metadata.commit_sha, head.trim());
        assert_eq!(metadata.author, "Test User");
        assert_eq!(metadata.branch, "main");
        assert!(metadata
            .changed_files
            .iter()
            .any(|f| f.ends_with("README.md")));
        assert_eq!(metadata.short_description(), "Update README");

        Ok(())
    }

    fn run_git(dir: &Path, args: &[&str]) -> Result<()> {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .map_err(|e| AosError::Git(format!("Failed to run git {:?}: {e}", args)))?;

        if status.success() {
            Ok(())
        } else {
            Err(AosError::Git(format!("Git command {:?} failed", args)))
        }
    }

    fn git_output(dir: &Path, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .map_err(|e| AosError::Git(format!("Failed to run git {:?}: {e}", args)))?;

        if !output.status.success() {
            return Err(AosError::Git(format!(
                "Git command {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        String::from_utf8(output.stdout)
            .map_err(|e| AosError::Git(format!("Invalid UTF-8 from git output: {e}")))
    }
}
