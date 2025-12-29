use crate::AosError;

/// Reserved prefixes for system-managed adapters
const RESERVED_ADAPTER_PREFIXES: &[&str] = &["system-", "internal-", "reserved-"];

pub fn validate_adapter_id(id: &str) -> Result<(), AosError> {
    // Check empty
    if id.is_empty() {
        return Err(AosError::Validation(
            "Adapter ID cannot be empty".to_string(),
        ));
    }

    // Check minimum length (at least 3 characters for meaningful IDs)
    if id.len() < 3 {
        return Err(AosError::Validation(
            "Adapter ID must be at least 3 characters".to_string(),
        ));
    }

    // Check maximum length
    if id.len() > 64 {
        return Err(AosError::Validation(
            "Adapter ID must be 64 characters or less".to_string(),
        ));
    }

    // Check allowed characters (alphanumeric, hyphens, underscores)
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AosError::Validation(
            "Adapter ID must contain only ASCII alphanumeric characters, hyphens, and underscores"
                .to_string(),
        ));
    }

    // Must start with alphanumeric character
    if let Some(first) = id.chars().next() {
        if !first.is_ascii_alphanumeric() {
            return Err(AosError::Validation(
                "Adapter ID must start with an alphanumeric character".to_string(),
            ));
        }
    }

    // Must end with alphanumeric character
    if let Some(last) = id.chars().last() {
        if !last.is_ascii_alphanumeric() {
            return Err(AosError::Validation(
                "Adapter ID must end with an alphanumeric character".to_string(),
            ));
        }
    }

    // No consecutive hyphens or underscores
    if id.contains("--") || id.contains("__") || id.contains("-_") || id.contains("_-") {
        return Err(AosError::Validation(
            "Adapter ID cannot contain consecutive hyphens or underscores".to_string(),
        ));
    }

    // Check reserved prefixes
    let lower_id = id.to_lowercase();
    for prefix in RESERVED_ADAPTER_PREFIXES {
        if lower_id.starts_with(prefix) {
            return Err(AosError::Validation(format!(
                "Adapter ID cannot start with reserved prefix '{}'",
                prefix
            )));
        }
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
        return Err(AosError::Validation(
            "Hash must start with 'b3:'".to_string(),
        ));
    }

    let hex_part = &hash[3..];
    if hex_part.len() != 64 {
        return Err(AosError::Validation(
            "B3 hash hex part must be 64 characters".to_string(),
        ));
    }

    hex::decode(hex_part)
        .map_err(|e| AosError::Validation(format!("Invalid hex in hash: {}", e)))?;

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
        return Err(AosError::Validation(
            "Cannot specify more than 100 file paths".to_string(),
        ));
    }

    for path in paths {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(AosError::Validation(
                "File path cannot be empty".to_string(),
            ));
        }
        if path.len() > 512 {
            return Err(AosError::Validation(
                "File path must be 512 characters or less".to_string(),
            ));
        }
        if std::path::Path::new(trimmed).is_absolute() {
            return Err(AosError::Validation(
                "File paths cannot be absolute".to_string(),
            ));
        }
        // Prevent path traversal attacks
        if trimmed.contains("..") {
            return Err(AosError::Validation(
                "File paths cannot contain '..'".to_string(),
            ));
        }
    }

    Ok(())
}
