//! Repository identifier normalization utilities.

/// Normalize a repository identifier to a canonical form.
///
/// This function ensures consistent repo identifiers by:
/// - Trimming leading/trailing whitespace
/// - Converting to lowercase for case-insensitive matching
/// - Removing trailing slashes
/// - Collapsing multiple consecutive slashes to single slashes
/// - Stripping common URL schemes (https://, http://, git://, ssh://)
/// - Converting git SSH format (git@host:path) to standard path format
/// - Removing `.git` suffix from URLs
///
/// The `repo:` prefix is preserved if present, as it indicates a locally-derived
/// repository identifier rather than a URL-based one.
///
/// # Examples
///
/// ```
/// use adapteros_core::normalize_repo_id;
///
/// assert_eq!(normalize_repo_id("https://github.com/org/repo"), "github.com/org/repo");
/// assert_eq!(normalize_repo_id("git@github.com:org/repo.git"), "github.com/org/repo");
/// assert_eq!(normalize_repo_id("GitHub.com/Org/Repo"), "github.com/org/repo");
/// assert_eq!(normalize_repo_id(""), "repo");
/// ```
pub fn normalize_repo_id(repo_id: &str) -> String {
    let trimmed = repo_id.trim();
    if trimmed.is_empty() {
        return "repo".to_string();
    }

    let mut normalized = trimmed.to_lowercase();

    // Strip common URL schemes
    for scheme in &["https://", "http://", "git://", "ssh://"] {
        if let Some(stripped) = normalized.strip_prefix(scheme) {
            normalized = stripped.to_string();
            break;
        }
    }

    // Handle git@ SSH format: git@github.com:org/repo -> github.com/org/repo
    if let Some(stripped) = normalized.strip_prefix("git@") {
        normalized = stripped.to_string();
        // Convert first colon to slash (git@github.com:org/repo -> github.com/org/repo)
        if let Some(colon_pos) = normalized.find(':') {
            let before_colon = &normalized[..colon_pos];
            let after_colon = &normalized[colon_pos + 1..];
            // Only convert if before colon looks like a domain (contains a dot)
            if before_colon.contains('.') {
                normalized = format!("{}/{}", before_colon, after_colon);
            }
        }
    }

    // Remove .git suffix
    if let Some(stripped) = normalized.strip_suffix(".git") {
        normalized = stripped.to_string();
    }

    // Handle repo: prefix specially - preserve it but normalize the rest
    if let Some(rest) = normalized.strip_prefix("repo:") {
        let normalized_rest = normalize_path_segments(rest);
        if normalized_rest.is_empty() {
            return "repo".to_string();
        }
        return format!("repo:{}", normalized_rest);
    }

    // Normalize path segments (collapse slashes, remove trailing)
    let result = normalize_path_segments(&normalized);
    if result.is_empty() {
        "repo".to_string()
    } else {
        result
    }
}

/// Normalize path segments by collapsing multiple slashes and removing trailing slashes.
///
/// # Examples
///
/// ```
/// use adapteros_core::normalize_path_segments;
///
/// assert_eq!(normalize_path_segments("a//b///c"), "a/b/c");
/// assert_eq!(normalize_path_segments("/leading/"), "leading");
/// assert_eq!(normalize_path_segments(""), "");
/// ```
pub fn normalize_path_segments(path: &str) -> String {
    path.split('/')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_repo_id_handles_case() {
        assert_eq!(
            normalize_repo_id("GitHub.com/Org/Repo"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn normalize_repo_id_removes_trailing_slash() {
        assert_eq!(
            normalize_repo_id("github.com/org/repo/"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn normalize_repo_id_collapses_slashes() {
        assert_eq!(
            normalize_repo_id("github.com//org///repo"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn normalize_repo_id_strips_https() {
        assert_eq!(
            normalize_repo_id("https://github.com/org/repo"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn normalize_repo_id_strips_http() {
        assert_eq!(
            normalize_repo_id("http://github.com/org/repo"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn normalize_repo_id_handles_git_ssh() {
        assert_eq!(
            normalize_repo_id("git@github.com:org/repo"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn normalize_repo_id_removes_git_suffix() {
        assert_eq!(
            normalize_repo_id("github.com/org/repo.git"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn normalize_repo_id_preserves_repo_prefix() {
        assert_eq!(normalize_repo_id("repo:my-project"), "repo:my-project");
    }

    #[test]
    fn normalize_repo_id_handles_empty() {
        assert_eq!(normalize_repo_id(""), "repo");
        assert_eq!(normalize_repo_id("   "), "repo");
    }

    #[test]
    fn normalize_path_segments_works() {
        assert_eq!(normalize_path_segments("a//b///c"), "a/b/c");
        assert_eq!(normalize_path_segments("/leading/"), "leading");
        assert_eq!(normalize_path_segments(""), "");
    }
}
