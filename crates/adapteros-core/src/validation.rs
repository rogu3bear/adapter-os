use crate::adapter_type::AdapterType;
use crate::AosError;

/// Reserved prefixes for system-managed adapters
const RESERVED_ADAPTER_PREFIXES: &[&str] = &["system-", "internal-", "reserved-"];
const CODEBASE_ADAPTER_PREFIX: &str = "code.";
const CODEBASE_ADAPTER_MAX_LEN: usize = 128;
const CODEBASE_REPO_SLUG_MAX_LEN: usize = 64;
const CODEBASE_COMMIT_MIN_LEN: usize = 7;
const CODEBASE_COMMIT_MAX_LEN: usize = 40;

pub fn validate_adapter_id(id: &str) -> Result<(), AosError> {
    // Check empty
    if id.is_empty() {
        return Err(AosError::Validation(
            "Adapter ID cannot be empty".to_string(),
        ));
    }

    if id.starts_with(CODEBASE_ADAPTER_PREFIX) {
        return validate_codebase_adapter_id(id);
    }

    // Check maximum length
    if id.len() > 64 {
        return Err(AosError::Validation(format!(
            "Adapter ID must be 64 characters or less (got {} chars for '{}')",
            id.len(),
            if id.len() > 32 {
                format!("{}...", &id[..32])
            } else {
                id.to_string()
            }
        )));
    }

    // Check allowed characters (alphanumeric, hyphens, underscores)
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        let invalid_chars: Vec<char> = id
            .chars()
            .filter(|c| !c.is_ascii_alphanumeric() && *c != '-' && *c != '_')
            .collect();
        return Err(AosError::Validation(format!(
            "Adapter ID '{}' contains invalid characters: {:?}. Only ASCII alphanumeric, hyphens, and underscores are allowed",
            id, invalid_chars
        )));
    }

    // Must start with alphanumeric character
    if let Some(first) = id.chars().next() {
        if !first.is_ascii_alphanumeric() {
            return Err(AosError::Validation(format!(
                "Adapter ID '{}' must start with an alphanumeric character (got '{}')",
                id, first
            )));
        }
    }

    // Must end with alphanumeric character
    if let Some(last) = id.chars().last() {
        if !last.is_ascii_alphanumeric() {
            return Err(AosError::Validation(format!(
                "Adapter ID '{}' must end with an alphanumeric character (got '{}')",
                id, last
            )));
        }
    }

    // No consecutive hyphens or underscores
    if id.contains("--") || id.contains("__") || id.contains("-_") || id.contains("_-") {
        return Err(AosError::Validation(format!(
            "Adapter ID '{}' cannot contain consecutive hyphens or underscores",
            id
        )));
    }

    // Check reserved prefixes
    let lower_id = id.to_lowercase();
    for prefix in RESERVED_ADAPTER_PREFIXES {
        if lower_id.starts_with(prefix) {
            return Err(AosError::Validation(format!(
                "Adapter ID '{}' cannot start with reserved prefix '{}'",
                id, prefix
            )));
        }
    }

    Ok(())
}

/// Validate adapter IDs for codebase runs (code.<repo_slug>.<commit>).
pub fn validate_codebase_adapter_id(id: &str) -> Result<(), AosError> {
    if id.len() > CODEBASE_ADAPTER_MAX_LEN {
        return Err(AosError::Validation(format!(
            "Codebase adapter ID must be {} characters or less",
            CODEBASE_ADAPTER_MAX_LEN
        )));
    }

    let rest = id.strip_prefix(CODEBASE_ADAPTER_PREFIX).ok_or_else(|| {
        AosError::Validation("Codebase adapter ID must start with 'code.'".to_string())
    })?;
    let mut parts = rest.split('.');

    // Edge case: fail fast with specific error messages instead of using unwrap_or_default
    // which would mask the actual problem and defer to a generic error later
    let repo_slug = match parts.next() {
        Some(s) if !s.is_empty() => s,
        Some(_) => {
            return Err(AosError::Validation(
                "Missing repo slug: codebase adapter ID must follow code.<repo_slug>.<commit>"
                    .to_string(),
            ))
        }
        None => {
            return Err(AosError::Validation(
                "Missing repo slug: codebase adapter ID must follow code.<repo_slug>.<commit>"
                    .to_string(),
            ))
        }
    };

    let commit = match parts.next() {
        Some(s) if !s.is_empty() => s,
        Some(_) => {
            return Err(AosError::Validation(
                "Missing commit: codebase adapter ID must follow code.<repo_slug>.<commit>"
                    .to_string(),
            ))
        }
        None => {
            return Err(AosError::Validation(
                "Missing commit: codebase adapter ID must follow code.<repo_slug>.<commit>"
                    .to_string(),
            ))
        }
    };

    // Check for extra parts (only repo_slug.commit allowed after 'code.')
    if parts.next().is_some() {
        return Err(AosError::Validation(
            "Too many parts: codebase adapter ID must follow code.<repo_slug>.<commit>".to_string(),
        ));
    }

    validate_codebase_repo_slug(repo_slug)?;
    validate_codebase_commit(commit)?;

    Ok(())
}

fn validate_codebase_repo_slug(slug: &str) -> Result<(), AosError> {
    if slug.is_empty() {
        return Err(AosError::Validation(
            "Codebase repo slug cannot be empty".to_string(),
        ));
    }
    if slug.len() > CODEBASE_REPO_SLUG_MAX_LEN {
        return Err(AosError::Validation(format!(
            "Codebase repo slug must be {} characters or less",
            CODEBASE_REPO_SLUG_MAX_LEN
        )));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(AosError::Validation(
            "Codebase repo slug must contain only lowercase letters, numbers, and underscores"
                .to_string(),
        ));
    }
    if slug.starts_with('_') || slug.ends_with('_') {
        return Err(AosError::Validation(
            "Codebase repo slug cannot start or end with '_'".to_string(),
        ));
    }
    if slug.contains("__") {
        return Err(AosError::Validation(
            "Codebase repo slug cannot contain consecutive underscores".to_string(),
        ));
    }
    Ok(())
}

fn validate_codebase_commit(commit: &str) -> Result<(), AosError> {
    if commit.len() < CODEBASE_COMMIT_MIN_LEN || commit.len() > CODEBASE_COMMIT_MAX_LEN {
        return Err(AosError::Validation(format!(
            "Codebase commit must be {}-{} hex characters",
            CODEBASE_COMMIT_MIN_LEN, CODEBASE_COMMIT_MAX_LEN
        )));
    }
    if !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AosError::Validation(
            "Codebase commit must be hexadecimal".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_name(name: &str) -> Result<(), AosError> {
    if name.is_empty() {
        return Err(AosError::Validation("Name cannot be empty".to_string()));
    }

    if name.len() > 128 {
        return Err(AosError::Validation(
            "Name must be 128 characters or less".to_string(),
        ));
    }

    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
    {
        return Err(AosError::Validation(
            "Name must contain only alphanumeric characters, spaces, hyphens, and underscores"
                .to_string(),
        ));
    }

    Ok(())
}

pub fn validate_hash_b3(hash: &str) -> Result<(), AosError> {
    if !hash.starts_with("b3:") {
        return Err(AosError::Validation(format!(
            "Hash must start with 'b3:' (got '{}')",
            hash
        )));
    }

    let hex_part = &hash[3..];
    if hex_part.len() != 64 {
        return Err(AosError::Validation(format!(
            "B3 hash hex part must be 64 characters (got {} chars in '{}')",
            hex_part.len(),
            hash
        )));
    }

    hex::decode(hex_part)
        .map_err(|e| AosError::Validation(format!("Invalid hex in hash '{}': {}", hash, e)))?;

    Ok(())
}

pub fn validate_repo_id(repo_id: &str) -> Result<(), AosError> {
    if repo_id.is_empty() {
        return Err(AosError::Validation(
            "Repository ID cannot be empty".to_string(),
        ));
    }

    if repo_id.len() > 256 {
        return Err(AosError::Validation(
            "Repository ID must be 256 characters or less".to_string(),
        ));
    }

    // Allow alphanumeric, hyphens, underscores, forward slashes (for org/repo), and dots
    if !repo_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.')
    {
        return Err(AosError::Validation(
            "Repository ID must contain only alphanumeric characters, hyphens, underscores, forward slashes, and dots"
                .to_string(),
        ));
    }

    Ok(())
}

pub fn validate_description(description: &str) -> Result<(), AosError> {
    if description.len() > 1024 {
        return Err(AosError::Validation(
            "Description must be 1024 characters or less".to_string(),
        ));
    }

    Ok(())
}

pub fn validate_file_paths(paths: &[String]) -> Result<(), AosError> {
    if paths.is_empty() {
        return Err(AosError::Validation(
            "File paths cannot be empty".to_string(),
        ));
    }

    if paths.len() > 100 {
        return Err(AosError::Validation(format!(
            "Cannot specify more than 100 file paths (got {})",
            paths.len()
        )));
    }

    for path in paths {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(AosError::Validation(
                "File path cannot be empty".to_string(),
            ));
        }
        if path.len() > 512 {
            return Err(AosError::Validation(format!(
                "File path must be 512 characters or less (got {} chars for '{}')",
                path.len(),
                if path.len() > 64 {
                    format!("{}...", &path[..64])
                } else {
                    path.clone()
                }
            )));
        }
        if std::path::Path::new(trimmed).is_absolute() {
            return Err(AosError::Validation(format!(
                "File paths cannot be absolute (got '{}')",
                trimmed
            )));
        }
        // Prevent path traversal attacks
        if trimmed.contains("..") {
            return Err(AosError::Validation(format!(
                "File paths cannot contain '..' (got '{}')",
                trimmed
            )));
        }
    }

    Ok(())
}

// =============================================================================
// Codebase Adapter Validation
// =============================================================================

/// Validate codebase adapter registration requirements.
///
/// Codebase adapters have special requirements:
/// - Must declare explicit `base_adapter_id` pointing to a core adapter
/// - Session binding is optional at creation but required for activation
///
/// # Arguments
///
/// * `adapter_type` - The adapter type being validated
/// * `base_adapter_id` - The base adapter ID (required for codebase type)
///
/// # Returns
///
/// Returns `Ok(())` if validation passes, `Err(AosError::Validation)` otherwise.
pub fn validate_codebase_adapter_registration(
    adapter_type: AdapterType,
    base_adapter_id: Option<&str>,
) -> Result<(), AosError> {
    if adapter_type == AdapterType::Codebase && base_adapter_id.is_none() {
        return Err(AosError::Validation(
            "Codebase adapters must declare base_adapter_id (core adapter baseline)".to_string(),
        ));
    }

    // Standard adapters should not have base_adapter_id (it's reserved for codebase)
    if adapter_type == AdapterType::Standard && base_adapter_id.is_some() {
        return Err(AosError::Validation(
            "Standard adapters should not have base_adapter_id (reserved for codebase adapters)"
                .to_string(),
        ));
    }

    Ok(())
}

/// Validate session binding requirements for codebase adapters.
///
/// Only codebase adapters can be bound to sessions for exclusive access.
///
/// # Arguments
///
/// * `adapter_type` - The adapter type being validated
/// * `session_id` - The session ID to bind to (optional)
///
/// # Returns
///
/// Returns `Ok(())` if validation passes, `Err(AosError::Validation)` otherwise.
pub fn validate_session_binding(
    adapter_type: AdapterType,
    session_id: Option<&str>,
) -> Result<(), AosError> {
    if session_id.is_some() && !adapter_type.can_bind_to_session() {
        return Err(AosError::Validation(format!(
            "Only codebase adapters can be bound to sessions, got type '{}'",
            adapter_type
        )));
    }

    Ok(())
}

/// Validate that a codebase adapter can be activated.
///
/// Activation requires:
/// - Adapter must be of codebase type
/// - Must have base_adapter_id set
/// - Should have session binding for streaming context
///
/// # Arguments
///
/// * `adapter_type` - The adapter type being validated
/// * `base_adapter_id` - The base adapter ID
/// * `session_id` - The session ID (optional but recommended)
///
/// # Returns
///
/// Returns `Ok(())` if validation passes, `Err(AosError::Validation)` otherwise.
pub fn validate_codebase_activation(
    adapter_type: AdapterType,
    base_adapter_id: Option<&str>,
    session_id: Option<&str>,
) -> Result<(), AosError> {
    if adapter_type != AdapterType::Codebase {
        return Ok(()); // Non-codebase adapters skip this validation
    }

    if base_adapter_id.is_none() {
        return Err(AosError::Validation(
            "Codebase adapter cannot be activated without base_adapter_id".to_string(),
        ));
    }

    if session_id.is_none() {
        tracing::warn!(
            "Codebase adapter activation without session binding; \
             consider binding to a session for proper stream scoping"
        );
    }

    Ok(())
}

/// Validate versioning threshold for auto-versioning.
///
/// # Arguments
///
/// * `threshold` - The versioning threshold (number of activations)
///
/// # Returns
///
/// Returns `Ok(())` if valid, `Err(AosError::Validation)` otherwise.
pub fn validate_versioning_threshold(threshold: Option<i32>) -> Result<(), AosError> {
    if let Some(t) = threshold {
        if t < 1 {
            return Err(AosError::Validation(
                "Versioning threshold must be at least 1".to_string(),
            ));
        }
        if t > 10000 {
            return Err(AosError::Validation(
                "Versioning threshold cannot exceed 10000".to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod codebase_validation_tests {
    use super::*;

    #[test]
    fn test_validate_codebase_registration_requires_base() {
        // Codebase without base_adapter_id should fail
        let result = validate_codebase_adapter_registration(AdapterType::Codebase, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("base_adapter_id"));

        // Codebase with base_adapter_id should pass
        let result =
            validate_codebase_adapter_registration(AdapterType::Codebase, Some("core-adapter"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_standard_cannot_have_base() {
        // Standard with base_adapter_id should fail
        let result =
            validate_codebase_adapter_registration(AdapterType::Standard, Some("core-adapter"));
        assert!(result.is_err());

        // Standard without base_adapter_id should pass
        let result = validate_codebase_adapter_registration(AdapterType::Standard, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_core_skips_base_check() {
        // Core adapters can have or not have base_adapter_id
        let result = validate_codebase_adapter_registration(AdapterType::Core, None);
        assert!(result.is_ok());

        let result = validate_codebase_adapter_registration(AdapterType::Core, Some("other-core"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_session_binding_only_codebase() {
        // Codebase can bind to session
        let result = validate_session_binding(AdapterType::Codebase, Some("session-123"));
        assert!(result.is_ok());

        // Standard cannot bind to session
        let result = validate_session_binding(AdapterType::Standard, Some("session-123"));
        assert!(result.is_err());

        // Core cannot bind to session
        let result = validate_session_binding(AdapterType::Core, Some("session-123"));
        assert!(result.is_err());

        // None session is always valid
        let result = validate_session_binding(AdapterType::Standard, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_versioning_threshold() {
        assert!(validate_versioning_threshold(None).is_ok());
        assert!(validate_versioning_threshold(Some(1)).is_ok());
        assert!(validate_versioning_threshold(Some(100)).is_ok());
        assert!(validate_versioning_threshold(Some(10000)).is_ok());
        assert!(validate_versioning_threshold(Some(0)).is_err());
        assert!(validate_versioning_threshold(Some(-1)).is_err());
        assert!(validate_versioning_threshold(Some(10001)).is_err());
    }

    #[test]
    fn test_validate_codebase_adapter_id_specific_errors() {
        // Test missing repo slug (empty after code.)
        let result = validate_codebase_adapter_id("code..abc123f");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Missing repo slug"),
            "Expected 'Missing repo slug' error, got: {}",
            err_msg
        );

        // Test missing commit
        let result = validate_codebase_adapter_id("code.myrepo.");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Missing commit"),
            "Expected 'Missing commit' error, got: {}",
            err_msg
        );

        // Test only repo slug, no commit at all
        let result = validate_codebase_adapter_id("code.myrepo");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Missing commit"),
            "Expected 'Missing commit' error, got: {}",
            err_msg
        );

        // Test too many parts
        let result = validate_codebase_adapter_id("code.repo.abc123f.extra");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Too many parts"),
            "Expected 'Too many parts' error, got: {}",
            err_msg
        );

        // Valid codebase adapter ID should pass
        let result = validate_codebase_adapter_id("code.myrepo.abc123f");
        assert!(result.is_ok());
    }
}
