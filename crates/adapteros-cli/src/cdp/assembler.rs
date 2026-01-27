//! CDP Assembler - Builds CommitDeltaPacks from git commits
//!
//! The `CdpAssembler` provides a builder pattern for constructing CDPs
//! from git repository commits, combining diff analysis, metadata extraction,
//! test results, and linter output into a complete package.

use crate::cdp::{
    diff_analyzer::DiffAnalyzer, metadata::MetadataExtractor, ChangedSymbol, CommitDeltaPack,
    CdpId, CdpMetadata, DiffSummary,
};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_worker::{LinterResult, TestResult};
use std::path::{Path, PathBuf};

/// Builder for constructing Commit Delta Packs
///
/// # Example
///
/// ```ignore
/// use adapteros_cli::cdp::CdpAssembler;
///
/// let cdp = CdpAssembler::new("/path/to/repo")
///     .from_commit("abc123")?
///     .with_test_results(test_results)
///     .with_linter_results(linter_results)
///     .build()?;
/// ```
pub struct CdpAssembler {
    repo_path: PathBuf,
    commit_sha: Option<String>,
    parent_sha: Option<String>,
    repo_id: Option<String>,
    diff_summary: Option<DiffSummary>,
    changed_symbols: Vec<ChangedSymbol>,
    test_results: Vec<TestResult>,
    linter_results: Vec<LinterResult>,
    metadata: Option<CdpMetadata>,
}

impl CdpAssembler {
    /// Create a new CDP assembler for a repository
    pub fn new<P: AsRef<Path>>(repo_path: P) -> Self {
        Self {
            repo_path: repo_path.as_ref().to_path_buf(),
            commit_sha: None,
            parent_sha: None,
            repo_id: None,
            diff_summary: None,
            changed_symbols: Vec::new(),
            test_results: Vec::new(),
            linter_results: Vec::new(),
            metadata: None,
        }
    }

    /// Configure the assembler from a specific commit
    ///
    /// This extracts metadata and analyzes the diff against the commit's parent.
    pub fn from_commit(mut self, commit_sha: &str) -> Result<Self> {
        self.commit_sha = Some(commit_sha.to_string());

        // Extract metadata
        let extractor = MetadataExtractor::new(&self.repo_path);
        self.metadata = Some(extractor.extract_for_commit(commit_sha)?);

        // Analyze diff
        let analyzer = DiffAnalyzer::new(&self.repo_path);
        let analysis = analyzer.analyze_commit(commit_sha)?;

        self.diff_summary = Some(analysis.summary);
        self.changed_symbols = analysis.changed_symbols;

        // Get parent SHA
        self.parent_sha = Some(self.get_parent_commit(commit_sha)?);

        // Set repo ID from remote URL or path
        self.repo_id = Some(self.derive_repo_id());

        Ok(self)
    }

    /// Configure the assembler from a commit range
    ///
    /// Useful for analyzing changes between two specific commits.
    pub fn from_commit_range(mut self, from_sha: &str, to_sha: &str) -> Result<Self> {
        self.parent_sha = Some(from_sha.to_string());
        self.commit_sha = Some(to_sha.to_string());

        // Extract metadata for the target commit
        let extractor = MetadataExtractor::new(&self.repo_path);
        self.metadata = Some(extractor.extract_for_commit(to_sha)?);

        // Analyze diff between commits
        let analyzer = DiffAnalyzer::new(&self.repo_path);
        let analysis = analyzer.analyze_commits(from_sha, to_sha)?;

        self.diff_summary = Some(analysis.summary);
        self.changed_symbols = analysis.changed_symbols;

        // Set repo ID from remote URL or path
        self.repo_id = Some(self.derive_repo_id());

        Ok(self)
    }

    /// Configure the assembler from uncommitted changes
    ///
    /// Creates a CDP for staged/unstaged changes in the working directory.
    pub fn from_uncommitted(mut self) -> Result<Self> {
        // Use HEAD as parent
        self.parent_sha = Some(self.get_head_sha()?);
        // Use a synthetic commit SHA for uncommitted changes
        self.commit_sha = Some(format!("uncommitted-{}", chrono::Utc::now().timestamp()));

        // Create synthetic metadata
        self.metadata = Some(CdpMetadata::new(
            self.get_git_user_email().unwrap_or_else(|_| "unknown".to_string()),
            "Uncommitted changes".to_string(),
            chrono::Utc::now(),
            self.get_current_branch().unwrap_or_else(|_| "unknown".to_string()),
            self.repo_path.clone(),
        ));

        // Analyze uncommitted diff
        let analyzer = DiffAnalyzer::new(&self.repo_path);
        let analysis = analyzer.analyze_uncommitted()?;

        self.diff_summary = Some(analysis.summary);
        self.changed_symbols = analysis.changed_symbols;

        // Set repo ID
        self.repo_id = Some(self.derive_repo_id());

        Ok(self)
    }

    /// Add test results to the CDP
    pub fn with_test_results(mut self, results: Vec<TestResult>) -> Self {
        self.test_results = results;
        self
    }

    /// Add linter results to the CDP
    pub fn with_linter_results(mut self, results: Vec<LinterResult>) -> Self {
        self.linter_results = results;
        self
    }

    /// Override the repository ID
    pub fn with_repo_id(mut self, repo_id: String) -> Self {
        self.repo_id = Some(repo_id);
        self
    }

    /// Add additional changed symbols
    pub fn with_additional_symbols(mut self, symbols: Vec<ChangedSymbol>) -> Self {
        self.changed_symbols.extend(symbols);
        self
    }

    /// Build the final CommitDeltaPack
    pub fn build(self) -> Result<CommitDeltaPack> {
        let commit_sha = self.commit_sha.ok_or_else(|| {
            AosError::validation("No commit specified. Call from_commit() first.")
        })?;

        let parent_sha = self.parent_sha.ok_or_else(|| {
            AosError::validation("No parent commit available.")
        })?;

        let repo_id = self.repo_id.ok_or_else(|| {
            AosError::validation("No repository ID available.")
        })?;

        let diff_summary = self.diff_summary.unwrap_or_default();
        let metadata = self.metadata.ok_or_else(|| {
            AosError::validation("No metadata available.")
        })?;

        // Create the CDP
        let mut cdp = CommitDeltaPack::new(
            repo_id,
            commit_sha,
            parent_sha,
            diff_summary,
            self.changed_symbols,
            self.test_results,
            self.linter_results,
            metadata,
        );

        // Compute content hash for determinism
        cdp.content_hash = Self::compute_content_hash(&cdp);

        Ok(cdp)
    }

    /// Get parent commit SHA
    fn get_parent_commit(&self, commit_sha: &str) -> Result<String> {
        use std::process::Command;

        let output = Command::new("git")
            .arg("rev-parse")
            .arg(format!("{}^", commit_sha))
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| AosError::Git(format!("Failed to get parent commit: {}", e)))?;

        if !output.status.success() {
            // Might be the initial commit with no parent
            return Ok("0000000000000000000000000000000000000000".to_string());
        }

        Ok(String::from_utf8(output.stdout)
            .map_err(|e| AosError::Git(format!("Invalid parent commit output: {}", e)))?
            .trim()
            .to_string())
    }

    /// Get HEAD SHA
    fn get_head_sha(&self) -> Result<String> {
        use std::process::Command;

        let output = Command::new("git")
            .arg("rev-parse")
            .arg("HEAD")
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| AosError::Git(format!("Failed to get HEAD: {}", e)))?;

        if !output.status.success() {
            return Err(AosError::Git(format!(
                "Failed to get HEAD: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(String::from_utf8(output.stdout)
            .map_err(|e| AosError::Git(format!("Invalid HEAD output: {}", e)))?
            .trim()
            .to_string())
    }

    /// Get current branch name
    fn get_current_branch(&self) -> Result<String> {
        use std::process::Command;

        let output = Command::new("git")
            .arg("branch")
            .arg("--show-current")
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| AosError::Git(format!("Failed to get branch: {}", e)))?;

        if !output.status.success() {
            return Ok("detached".to_string());
        }

        let branch = String::from_utf8(output.stdout)
            .map_err(|e| AosError::Git(format!("Invalid branch output: {}", e)))?
            .trim()
            .to_string();

        if branch.is_empty() {
            Ok("detached".to_string())
        } else {
            Ok(branch)
        }
    }

    /// Get git user email
    fn get_git_user_email(&self) -> Result<String> {
        use std::process::Command;

        let output = Command::new("git")
            .arg("config")
            .arg("user.email")
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| AosError::Git(format!("Failed to get user email: {}", e)))?;

        if !output.status.success() {
            return Err(AosError::Git("No git user email configured".to_string()));
        }

        Ok(String::from_utf8(output.stdout)
            .map_err(|e| AosError::Git(format!("Invalid email output: {}", e)))?
            .trim()
            .to_string())
    }

    /// Derive repository ID from remote URL or path
    fn derive_repo_id(&self) -> String {
        use std::process::Command;

        // Try to get remote URL first
        if let Ok(output) = Command::new("git")
            .arg("remote")
            .arg("get-url")
            .arg("origin")
            .current_dir(&self.repo_path)
            .output()
        {
            if output.status.success() {
                if let Ok(url) = String::from_utf8(output.stdout) {
                    let url = url.trim();
                    // Extract repo identifier from URL
                    if let Some(repo_name) = Self::extract_repo_name_from_url(url) {
                        return repo_name;
                    }
                }
            }
        }

        // Fall back to directory name
        self.repo_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown-repo".to_string())
    }

    /// Extract repository name from git URL
    fn extract_repo_name_from_url(url: &str) -> Option<String> {
        // Handle SSH format: git@github.com:owner/repo.git
        if url.contains('@') && url.contains(':') {
            let parts: Vec<&str> = url.split(':').collect();
            if parts.len() >= 2 {
                let path = parts[1].trim_end_matches(".git");
                return Some(path.replace('/', "-"));
            }
        }

        // Handle HTTPS format: https://github.com/owner/repo.git
        if url.starts_with("http") {
            let path = url
                .trim_end_matches(".git")
                .split('/')
                .skip(3) // Skip protocol, empty, and domain
                .collect::<Vec<_>>()
                .join("-");
            if !path.is_empty() {
                return Some(path);
            }
        }

        None
    }

    /// Compute content hash for determinism verification
    fn compute_content_hash(cdp: &CommitDeltaPack) -> B3Hash {
        // Create a deterministic representation of the CDP content
        let mut hasher_input = Vec::new();

        // Include key fields in the hash
        hasher_input.extend(cdp.repo_id.as_bytes());
        hasher_input.extend(cdp.commit_sha.as_bytes());
        hasher_input.extend(cdp.parent_sha.as_bytes());

        // Include diff summary stats
        hasher_input.extend(&cdp.diff_summary.lines_added.to_le_bytes());
        hasher_input.extend(&cdp.diff_summary.lines_removed.to_le_bytes());
        hasher_input.extend(&(cdp.diff_summary.total_files() as u64).to_le_bytes());

        // Include symbol count
        hasher_input.extend(&(cdp.changed_symbols.len() as u64).to_le_bytes());

        // Include test/linter result counts
        hasher_input.extend(&(cdp.test_results.len() as u64).to_le_bytes());
        hasher_input.extend(&(cdp.linter_results.len() as u64).to_le_bytes());

        B3Hash::hash(&hasher_input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_test_repo() -> Result<TempDir> {
        let temp_dir = TempDir::with_prefix("aos-cdp-test-")?;
        let repo_path = temp_dir.path();

        // Initialize git repo
        Command::new("git")
            .arg("init")
            .current_dir(repo_path)
            .output()?;

        // Configure git user
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()?;

        Ok(temp_dir)
    }

    fn create_commit(repo_path: &Path, filename: &str, content: &str, message: &str) -> Result<String> {
        // Create parent directories if needed
        let file_path = repo_path.join(filename);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write file
        std::fs::write(&file_path, content)?;

        // Stage file
        Command::new("git")
            .args(["add", filename])
            .current_dir(repo_path)
            .output()?;

        // Commit
        Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(repo_path)
            .output()?;

        // Get commit SHA
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo_path)
            .output()?;

        String::from_utf8(output.stdout)
            .map(|s| s.trim().to_string())
            .map_err(|e| AosError::Git(format!("Invalid commit output: {}", e)))
    }

    #[test]
    fn test_assembler_creation() {
        let assembler = CdpAssembler::new("/tmp/test-repo");
        assert_eq!(assembler.repo_path, PathBuf::from("/tmp/test-repo"));
        assert!(assembler.commit_sha.is_none());
        assert!(assembler.test_results.is_empty());
    }

    #[test]
    fn test_assembler_with_test_results() {
        let test_result = TestResult {
            framework: adapteros_lora_worker::TestFramework::CargoTest,
            passed: 10,
            failed: 2,
            ignored: 1,
            duration_ms: 1500,
            failures: Vec::new(),
        };

        let assembler = CdpAssembler::new("/tmp/test-repo")
            .with_test_results(vec![test_result]);

        assert_eq!(assembler.test_results.len(), 1);
        assert_eq!(assembler.test_results[0].passed, 10);
    }

    #[test]
    fn test_assembler_with_linter_results() {
        let linter_result = LinterResult {
            linter: adapteros_lora_worker::LinterType::Clippy,
            errors: Vec::new(),
            warnings: Vec::new(),
            duration_ms: 500,
        };

        let assembler = CdpAssembler::new("/tmp/test-repo")
            .with_linter_results(vec![linter_result]);

        assert_eq!(assembler.linter_results.len(), 1);
    }

    #[test]
    fn test_extract_repo_name_from_url() {
        // SSH format
        assert_eq!(
            CdpAssembler::extract_repo_name_from_url("git@github.com:owner/repo.git"),
            Some("owner-repo".to_string())
        );

        // HTTPS format
        assert_eq!(
            CdpAssembler::extract_repo_name_from_url("https://github.com/owner/repo.git"),
            Some("owner-repo".to_string())
        );

        // Without .git suffix
        assert_eq!(
            CdpAssembler::extract_repo_name_from_url("https://github.com/owner/repo"),
            Some("owner-repo".to_string())
        );
    }

    #[test]
    fn test_build_requires_commit() {
        let assembler = CdpAssembler::new("/tmp/test-repo");
        let result = assembler.build();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No commit specified"));
    }

    #[test]
    #[ignore = "requires fix for MetadataExtractor date parsing (uses iso format but parses as rfc3339)"]
    fn test_full_assembly_with_git_repo() -> Result<()> {
        let temp_dir = init_test_repo()?;
        let repo_path = temp_dir.path();

        // Create initial commit
        create_commit(repo_path, "README.md", "# Test Repo", "Initial commit")?;

        // Create second commit with Rust code
        let rust_code = r#"
pub fn hello() -> String {
    "Hello, world!".to_string()
}
"#;
        let commit_sha = create_commit(repo_path, "src/lib.rs", rust_code, "Add hello function")?;

        // Assemble CDP
        let cdp = CdpAssembler::new(repo_path)
            .from_commit(&commit_sha)?
            .with_repo_id("test-repo".to_string())
            .build()?;

        assert_eq!(cdp.repo_id, "test-repo");
        assert_eq!(cdp.commit_sha, commit_sha);
        assert!(!cdp.diff_summary.is_empty());

        Ok(())
    }

    #[test]
    fn test_from_uncommitted_changes() -> Result<()> {
        let temp_dir = init_test_repo()?;
        let repo_path = temp_dir.path();

        // Create initial commit
        create_commit(repo_path, "README.md", "# Test Repo", "Initial commit")?;

        // Create uncommitted changes
        std::fs::write(repo_path.join("new_file.rs"), "fn main() {}")?;

        // Assemble CDP from uncommitted changes
        let cdp = CdpAssembler::new(repo_path)
            .from_uncommitted()?
            .with_repo_id("test-repo".to_string())
            .build()?;

        assert_eq!(cdp.repo_id, "test-repo");
        assert!(cdp.commit_sha.starts_with("uncommitted-"));
        assert_eq!(cdp.metadata.message, "Uncommitted changes");

        Ok(())
    }
}
