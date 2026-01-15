//! Repository slug normalization utilities.

/// Normalize a repository name to a URL-safe slug.
///
/// This function ensures consistent slug formatting by:
/// - Trimming leading/trailing whitespace
/// - Converting to lowercase
/// - Replacing non-alphanumeric characters with underscores
/// - Collapsing consecutive underscores to single underscores
/// - Trimming leading/trailing underscores
/// - Truncating to max 64 characters (without breaking words)
///
/// # Examples
///
/// ```
/// use adapteros_normalization::normalize_repo_slug;
///
/// assert_eq!(normalize_repo_slug("adapterOS-Core"), "adapteros_core");
/// assert_eq!(normalize_repo_slug("My Awesome Repo!"), "my_awesome_repo");
/// assert_eq!(normalize_repo_slug("__weird__"), "weird");
/// assert_eq!(normalize_repo_slug(""), "repo");
/// ```
pub fn normalize_repo_slug(input: &str) -> String {
    const MAX_SLUG_LENGTH: usize = 64;

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "repo".to_string();
    }

    let mut slug = trimmed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();

    // Collapse consecutive underscores
    while slug.contains("__") {
        slug = slug.replace("__", "_");
    }

    // Trim leading/trailing underscores
    let trimmed_slug = slug.trim_matches('_');
    if trimmed_slug.is_empty() {
        return "repo".to_string();
    }

    // Truncate to max length, ensuring we don't cut in the middle of a word
    let mut result = trimmed_slug.to_string();
    if result.len() > MAX_SLUG_LENGTH {
        result.truncate(MAX_SLUG_LENGTH);
        // Remove trailing underscore if truncation created one
        result = result.trim_end_matches('_').to_string();
        if result.is_empty() {
            return "repo".to_string();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_repo_slug_handles_symbols() {
        assert_eq!(normalize_repo_slug("adapterOS-Core"), "adapteros_core");
        assert_eq!(normalize_repo_slug("__weird__"), "weird");
    }

    #[test]
    fn normalize_repo_slug_handles_case() {
        assert_eq!(normalize_repo_slug("MyRepo"), "myrepo");
        assert_eq!(normalize_repo_slug("MY_REPO"), "my_repo");
        assert_eq!(normalize_repo_slug("My-Awesome-Repo"), "my_awesome_repo");
    }

    #[test]
    fn normalize_repo_slug_handles_special_chars() {
        assert_eq!(normalize_repo_slug("repo@v1.0.0"), "repo_v1_0_0");
        assert_eq!(normalize_repo_slug("my.repo.name"), "my_repo_name");
        assert_eq!(normalize_repo_slug("repo#123"), "repo_123");
        assert_eq!(normalize_repo_slug("my repo name"), "my_repo_name");
    }

    #[test]
    fn normalize_repo_slug_collapses_underscores() {
        assert_eq!(normalize_repo_slug("repo___name"), "repo_name");
        assert_eq!(normalize_repo_slug("a--b--c"), "a_b_c");
        assert_eq!(
            normalize_repo_slug("__leading_trailing__"),
            "leading_trailing"
        );
    }

    #[test]
    fn normalize_repo_slug_trims_whitespace() {
        assert_eq!(normalize_repo_slug("  myrepo  "), "myrepo");
        assert_eq!(normalize_repo_slug("\t\nrepo\n\t"), "repo");
    }

    #[test]
    fn normalize_repo_slug_handles_empty_input() {
        assert_eq!(normalize_repo_slug(""), "repo");
        assert_eq!(normalize_repo_slug("   "), "repo");
        assert_eq!(normalize_repo_slug("___"), "repo");
        assert_eq!(normalize_repo_slug("---"), "repo");
    }

    #[test]
    fn normalize_repo_slug_truncates_long_names() {
        let long_name = "a".repeat(100);
        let result = normalize_repo_slug(&long_name);
        assert!(result.len() <= 64);
        assert_eq!(result.len(), 64);
    }
}
