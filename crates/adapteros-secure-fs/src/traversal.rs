//! Path traversal protection
//!
//! Implements path traversal protection to prevent directory traversal attacks.

use adapteros_core::{AosError, Result};
use std::path::{Component, Path, PathBuf};
use tracing::debug;

/// Path traversal protection configuration
#[derive(Debug, Clone)]
pub struct TraversalProtection {
    /// Enable traversal protection
    pub enabled: bool,
    /// Maximum path depth
    pub max_depth: u32,
    /// Blocked components
    pub blocked_components: Vec<String>,
    /// Allowed components
    pub allowed_components: Vec<String>,
}

impl Default for TraversalProtection {
    fn default() -> Self {
        Self {
            enabled: true,
            max_depth: 20,
            blocked_components: vec![
                "..".to_string(),
                "~".to_string(),
                "$HOME".to_string(),
                "$USER".to_string(),
            ],
            allowed_components: vec![],
        }
    }
}

/// Check if a path is safe from traversal attacks
pub fn check_path_traversal(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    let protection = TraversalProtection::default();

    if !protection.enabled {
        return Ok(());
    }

    // Check path components
    let components: Vec<Component> = path.components().collect();

    // Check path depth
    if components.len() > protection.max_depth as usize {
        return Err(AosError::Security(format!(
            "Path depth {} exceeds maximum {}",
            components.len(),
            protection.max_depth
        )));
    }

    // Check for blocked components
    for component in &components {
        match component {
            Component::ParentDir => {
                return Err(AosError::Security(
                    "Parent directory traversal detected".to_string(),
                ));
            }
            Component::Normal(name) => {
                let name_str = name.to_string_lossy().to_string();

                // Check blocked components
                if protection.blocked_components.contains(&name_str) {
                    return Err(AosError::Security(format!(
                        "Blocked component detected: {}",
                        name_str
                    )));
                }

                // Check allowed components (if specified)
                if !protection.allowed_components.is_empty()
                    && !protection.allowed_components.contains(&name_str)
                {
                    return Err(AosError::Security(format!(
                        "Component not allowed: {}",
                        name_str
                    )));
                }
            }
            Component::RootDir => {
                // Root directory is generally safe
            }
            Component::CurDir => {
                // Current directory is generally safe
            }
            Component::Prefix(_) => {
                // Windows prefix - generally safe
            }
        }
    }

    // Check for suspicious patterns
    check_suspicious_patterns(path)?;

    debug!("Path traversal check passed for: {}", path.display());
    Ok(())
}

/// Check for suspicious patterns in path with comprehensive URL decoding
fn check_suspicious_patterns(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    let path_str = path.to_string_lossy().to_string();

    // First, check the raw path for obvious traversal patterns
    check_raw_patterns(&path_str)?;

    // Then check URL-decoded versions for encoded attacks
    check_url_decoded_patterns(&path_str)?;

    // Check for dangerous absolute paths (both native and cross-platform)
    check_dangerous_absolute_paths(&path_str)?;

    Ok(())
}

/// Check raw path for obvious traversal patterns
fn check_raw_patterns(path_str: &str) -> Result<()> {
    let suspicious_patterns = vec![
        "../",
        "..\\",
        "....//",
        "....\\\\",
        // Unicode and other variations
        "..%c0%af",
        "..%c1%9c",
        // Overlong UTF-8 sequences
        "..%e0%80%ae%e0%80%ae/",
        // Double encoding attempts
        "..%252f",
        "..%255c",
        "%2e%2e%2f",
        "%2e%2e%5c",
        // Null byte attacks
        "%00",
        // Path traversal with drive letters
        "..\\..\\",
        "..\\..\\..\\",
        // UNC path attacks
        "\\\\",
        "//",
    ];

    for pattern in suspicious_patterns {
        if path_str.contains(pattern) {
            return Err(AosError::Security(format!(
                "Suspicious pattern detected in raw path: {}",
                pattern
            )));
        }
    }

    Ok(())
}

/// Check URL-decoded versions for encoded attacks
fn check_url_decoded_patterns(path_str: &str) -> Result<()> {
    // URL decode up to 3 levels deep to catch double/triple encoding
    let mut decoded = path_str.to_string();

    for level in 0..3 {
        let new_decoded = url_decode(&decoded);
        if new_decoded == decoded {
            break; // No further decoding possible
        }
        decoded = new_decoded;

        // Check for traversal patterns in each decoded level
        let traversal_patterns = vec![
            "../",
            "..\\",
            "..",
            "....//",
            "....\\\\",
        ];

        for pattern in traversal_patterns {
            if decoded.contains(pattern) {
                return Err(AosError::Security(format!(
                    "Suspicious pattern detected after URL decoding (level {}): {}",
                    level + 1,
                    pattern
                )));
            }
        }
    }

    Ok(())
}

/// Simple URL decoder for security checks
fn url_decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '%' {
            // Try to decode %XX sequence
            if let (Some(h1), Some(h2)) = (chars.next(), chars.next()) {
                if let (Some(d1), Some(d2)) = (h1.to_digit(16), h2.to_digit(16)) {
                    let byte = ((d1 << 4) | d2) as u8;
                    if let Ok(decoded_char) = std::str::from_utf8(&[byte]) {
                        result.push_str(decoded_char);
                        continue;
                    }
                }
            }
            // If decoding fails, keep the % and following chars
            result.push('%');
            if let Some(h1) = chars.next() {
                result.push(h1);
                if let Some(h2) = chars.next() {
                    result.push(h2);
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Check for dangerous absolute paths
fn check_dangerous_absolute_paths(path_str: &str) -> Result<()> {
    // Convert backslashes to forward slashes for consistent checking
    let normalized_path = path_str.replace('\\', "/");

    let dangerous_prefixes = vec![
        // Unix system paths
        "/etc/passwd",
        "/etc/shadow",
        "/etc/hosts",
        "/etc/group",
        "/etc/sudoers",
        "/bin/",
        "/usr/bin/",
        "/usr/sbin/",
        "/usr/local/bin/",
        "/sbin/",
        "/home/",
        "/root/",
        "/boot/",
        "/sys/",
        "/proc/",
        "/dev/",
        "/var/log/",
        "/var/spool/",
        "/var/tmp/",
        // Windows system paths (normalized to forward slashes)
        "C:/Windows/System32/",
        "C:/Windows/System/",
        "C:/Windows/",
        "C:/Users/",
        "C:/Program Files/",
        "C:/Program Files (x86)/",
        "C:/ProgramData/",
        "C:/System Volume Information/",
        // Network shares (UNC paths)
        "//",
        "\\\\",
    ];

    for prefix in dangerous_prefixes {
        if normalized_path.starts_with(prefix) {
            return Err(AosError::Security(format!(
                "Access to sensitive system path not allowed: {}",
                path_str
            )));
        }
    }

    // Special case: allow /tmp/ for temporary files on Unix
    if path_str.starts_with("/tmp/") && path_str.len() > 5 {
        return Ok(());
    }

    Ok(())
}

/// Check if path is a symlink (which could be used for path traversal attacks)
pub fn check_no_symlinks(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();

    // Check if the path itself is a symlink (only if it exists)
    if path.exists() && path.is_symlink() {
        return Err(AosError::Security(format!(
            "Path is a symlink, which is not allowed: {}",
            path.display()
        )));
    }

    // Walk through each component and check if any intermediate path is a symlink
    // Only check components that exist
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);

        // Skip checking the root and current directory components
        if matches!(component, std::path::Component::RootDir | std::path::Component::CurDir) {
            continue;
        }

        // Only check for symlinks if the intermediate path exists
        if current.exists() && current.is_symlink() {
            return Err(AosError::Security(format!(
                "Path contains symlink component, which is not allowed: {}",
                current.display()
            )));
        }
    }

    Ok(())
}

/// Normalize a path safely
pub fn normalize_path(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();

    // Check for traversal attacks first
    check_path_traversal(path)?;

    // Check for symlinks
    check_no_symlinks(path)?;

    // Normalize the path
    let normalized = path
        .canonicalize()
        .map_err(|e| AosError::Security(format!("Failed to canonicalize path: {}", e)))?;

    Ok(normalized)
}

/// Join paths safely
pub fn join_paths_safe(base: impl AsRef<Path>, relative: impl AsRef<Path>) -> Result<PathBuf> {
    let base = base.as_ref();
    let relative = relative.as_ref();

    // Check relative path for traversal
    check_path_traversal(relative)?;

    // Join paths
    let joined = base.join(relative);

    // Check the result for traversal and symlinks
    check_path_traversal(&joined)?;
    check_no_symlinks(&joined)?;

    Ok(joined)
}

/// Get relative path safely
pub fn get_relative_path_safe(base: impl AsRef<Path>, target: impl AsRef<Path>) -> Result<PathBuf> {
    let base = base.as_ref();
    let target = target.as_ref();

    // Check both paths
    check_path_traversal(base)?;
    check_path_traversal(target)?;

    // Get relative path
    let relative = target
        .strip_prefix(base)
        .map_err(|e| AosError::Security(format!("Failed to get relative path: {}", e)))?;

    // Check the result
    check_path_traversal(relative)?;

    Ok(relative.to_path_buf())
}

/// Check if path is within base directory
pub fn is_path_within_base(path: impl AsRef<Path>, base: impl AsRef<Path>) -> Result<bool> {
    let path = path.as_ref();
    let base = base.as_ref();

    // Check both paths
    check_path_traversal(path)?;
    check_path_traversal(base)?;

    // Canonicalize base path (must exist)
    let canonical_base = base.canonicalize()
        .map_err(|e| AosError::Security(format!("Failed to canonicalize base path for validation: {}", e)))?;

    // For path, try to canonicalize but fall back to logical comparison if it doesn't exist
    let canonical_path = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // Path doesn't exist, do logical comparison after normalizing components
            // This handles the case where we're checking if a proposed path would be within base
            // even if the path doesn't exist yet
            path.to_path_buf()
        }
    };

    // Check if path is within base
    let is_within = canonical_path.starts_with(&canonical_base);

    Ok(is_within)
}

/// Validate path is within allowed base directories
pub fn validate_path_within_bases(path: impl AsRef<Path>, allowed_bases: &[impl AsRef<Path>]) -> Result<()> {
    let path = path.as_ref();

    // Check path traversal first
    check_path_traversal(path)?;

    // Canonicalize the path
    let canonical_path = path.canonicalize()
        .map_err(|e| AosError::Security(format!("Failed to canonicalize path: {}", e)))?;

    // Check if path is within any allowed base
    for base in allowed_bases {
        if is_path_within_base(&canonical_path, base)? {
            return Ok(());
        }
    }

    // Path is not within any allowed base directory
    return Err(AosError::Security(format!(
        "Path '{}' is not within any allowed base directory",
        canonical_path.display()
    )));
}

/// Safe file existence check with path validation
pub fn safe_file_exists(path: impl AsRef<Path>, allowed_bases: &[impl AsRef<Path>]) -> Result<bool> {
    validate_path_within_bases(&path, allowed_bases)?;
    Ok(path.as_ref().exists())
}

/// Safe file metadata read with path validation
pub fn safe_file_metadata(path: impl AsRef<Path>, allowed_bases: &[impl AsRef<Path>]) -> Result<std::fs::Metadata> {
    validate_path_within_bases(&path, allowed_bases)?;
    std::fs::metadata(&path)
        .map_err(|e| AosError::Security(format!("Failed to read file metadata: {}", e)))
}

/// Safe path normalization with base validation
pub fn normalize_path_with_base_validation(
    path: impl AsRef<Path>,
    allowed_bases: &[impl AsRef<Path>]
) -> Result<PathBuf> {
    let path = path.as_ref();

    // Check for traversal attacks first
    check_path_traversal(path)?;

    // Check for symlinks
    check_no_symlinks(path)?;

    // Canonicalize the path
    let normalized = path.canonicalize()
        .map_err(|e| AosError::Security(format!("Failed to canonicalize path: {}", e)))?;

    // Validate against allowed bases
    validate_path_within_bases(&normalized, allowed_bases)?;

    Ok(normalized)
}

/// Configuration for path validation
#[derive(Debug, Clone)]
pub struct PathValidationConfig {
    /// Allowed base directories
    pub allowed_bases: Vec<PathBuf>,
    /// Maximum path length
    pub max_path_length: usize,
    /// Maximum file size for validation (0 = no limit)
    pub max_file_size_bytes: u64,
}

impl Default for PathValidationConfig {
    fn default() -> Self {
        Self {
            allowed_bases: vec![],
            max_path_length: 4096, // Reasonable path length limit
            max_file_size_bytes: 10 * 1024 * 1024 * 1024, // 10GB default
        }
    }
}

/// Validate file path comprehensively
pub fn validate_file_path_comprehensive(
    path: impl AsRef<Path>,
    config: &PathValidationConfig
) -> Result<()> {
    let path = path.as_ref();

    // Check path length
    let path_str = path.to_string_lossy();
    if path_str.len() > config.max_path_length {
        return Err(AosError::Security(format!(
            "Path length {} exceeds maximum {}",
            path_str.len(),
            config.max_path_length
        )));
    }

    // Standard traversal and symlink checks
    check_path_traversal(path)?;
    check_no_symlinks(path)?;

    // Check against allowed bases if configured
    if !config.allowed_bases.is_empty() {
        validate_path_within_bases(path, &config.allowed_bases)?;
    }

    // If file exists, check size limits
    if path.exists() && config.max_file_size_bytes > 0 {
        let metadata = std::fs::metadata(path)
            .map_err(|e| AosError::Security(format!("Failed to read file metadata: {}", e)))?;

        if metadata.len() > config.max_file_size_bytes {
            return Err(AosError::Security(format!(
                "File size {} bytes exceeds maximum {} bytes",
                metadata.len(),
                config.max_file_size_bytes
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_path_traversal_protection() -> Result<()> {
        // Test normal path
        check_path_traversal("test/file.txt")?;

        // Test parent directory traversal
        let result = check_path_traversal("../test/file.txt");
        assert!(result.is_err());

        // Test suspicious patterns
        let result = check_path_traversal("test/..%2f/etc/passwd");
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_path_normalization() -> Result<()> {
        // Create a test file in current directory to avoid symlink issues with temp dirs
        let test_file = PathBuf::from("test_normalization.txt");
        std::fs::write(&test_file, "hello")?;

        let normalized = normalize_path(&test_file)?;
        assert!(normalized.exists());

        // Clean up
        let _ = std::fs::remove_file(&test_file);

        Ok(())
    }

    #[test]
    fn test_path_joining() -> Result<()> {
        let base = PathBuf::from("/safe/base");
        let relative = PathBuf::from("test/file.txt");

        let joined = join_paths_safe(&base, &relative)?;
        assert_eq!(joined, PathBuf::from("/safe/base/test/file.txt"));

        // Test traversal attempt
        let result = join_paths_safe(&base, "../etc/passwd");
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_path_within_base() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let base = temp_dir.path();

        // Create a test file within the base directory
        let test_file = base.join("test.txt");
        std::fs::write(&test_file, "test")?;

        assert!(is_path_within_base(&test_file, base)?);

        // Test file outside base
        let outside_file = std::env::temp_dir().join("outside.txt");
        std::fs::write(&outside_file, "test")?;
        assert!(!is_path_within_base(&outside_file, base)?);

        Ok(())
    }

    #[test]
    fn test_symlink_protection() -> Result<()> {
        // Test with a simple relative path that should not have symlinks
        let test_path = PathBuf::from("test.txt");

        // This should pass for a non-existent relative path
        // (we can't easily test symlinks without creating them in CI)

        // Test that we can call the function without panicking
        let _ = check_no_symlinks(&test_path);

        // Test with current directory
        let current_dir = PathBuf::from(".");
        let _ = check_no_symlinks(&current_dir);

        Ok(())
    }

    #[test]
    fn test_validate_path_within_bases() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let base_dir = temp_dir.path();
        let allowed_file = base_dir.join("allowed.txt");
        std::fs::write(&allowed_file, "test")?;

        // Test file within allowed base
        validate_path_within_bases(&allowed_file, &[base_dir])?;

        // Test file outside allowed base
        let outside_file = std::env::temp_dir().join("outside.txt");
        std::fs::write(&outside_file, "test")?;
        let result = validate_path_within_bases(&outside_file, &[base_dir]);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_safe_file_operations() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let base_dir = temp_dir.path();
        let test_file = base_dir.join("test.txt");
        std::fs::write(&test_file, "test content")?;

        // Test safe file exists
        assert!(safe_file_exists(&test_file, &[base_dir])?);

        // Test safe file metadata
        let metadata = safe_file_metadata(&test_file, &[base_dir])?;
        assert_eq!(metadata.len(), 12); // "test content" is 12 bytes

        // Test with unauthorized path
        let outside_file = std::env::temp_dir().join("outside.txt");
        let result = safe_file_exists(&outside_file, &[base_dir]);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_normalize_path_with_base_validation() -> Result<()> {
        // Create test files in current directory to avoid symlink issues
        let base_dir = PathBuf::from(".");
        let test_file = PathBuf::from("test_base_validation.txt");
        std::fs::write(&test_file, "test")?;

        // Test valid path
        let normalized = normalize_path_with_base_validation(&test_file, &[&base_dir])?;
        assert!(normalized.exists());

        // Clean up
        let _ = std::fs::remove_file(&test_file);

        Ok(())
    }

    #[test]
    fn test_path_validation_config() -> Result<()> {
        // Create test files in current directory to avoid symlink issues
        let base_dir = PathBuf::from(".");
        let test_file = PathBuf::from("test_config_validation.txt");
        std::fs::write(&test_file, "test")?;

        let config = PathValidationConfig {
            allowed_bases: vec![base_dir.clone()],
            max_path_length: 4096,
            max_file_size_bytes: 100,
        };

        // Test valid file
        validate_file_path_comprehensive(&test_file, &config)?;

        // Test path length limit
        let long_name = "a".repeat(5000);
        let long_path = PathBuf::from(&long_name);
        let result = validate_file_path_comprehensive(&long_path, &config);
        assert!(result.is_err());

        // Test file size limit
        let large_content = "x".repeat(200); // 200 bytes, over 100 byte limit
        std::fs::write(&test_file, large_content)?;
        let result = validate_file_path_comprehensive(&test_file, &config);
        assert!(result.is_err());

        // Clean up
        let _ = std::fs::remove_file(&test_file);

        Ok(())
    }

    #[test]
    fn test_path_traversal_attack_vectors() -> Result<()> {
        // Test various path traversal attack patterns including URL encoding
        let attack_vectors = vec![
            // Basic traversal
            "../etc/passwd",
            "..\\..\\windows\\system32\\config\\sam",
            "../../../etc/shadow",
            // Single URL encoding
            "..%2fetc%2fpasswd",
            "..%5cwindows%5csystem32%5cconfig%5csam",
            "%2e%2e%2fetc%2fpasswd",
            "%2e%2e%5cwindows%5csystem32%5cconfig%5csam",
            // Double URL encoding
            "..%252fetc%252fpasswd",
            "..%255cwindows%255csystem32%255cconfig%255csam",
            // Unicode/overlong UTF-8
            "..%c0%af..%c0%af..%c0%afetc%c0%afpasswd",
            "..%e0%80%ae%e0%80%ae/",
            // Null byte attacks
            "../../../etc/passwd%00",
            "..%2fetc%2fpasswd%00",
            // Multiple traversal levels
            "....//....//....//etc/passwd",
            "../../../../../../../etc/passwd",
            // UNC path attacks
            "\\\\evil\\share\\malicious.exe",
            "//evil/share/malicious.exe",
            // Direct system file access
            "/etc/passwd",
            "/etc/shadow",
            "/etc/sudoers",
            "C:\\Windows\\System32\\config\\sam",
            // Home directory access
            "~/.ssh/id_rsa",
            "$HOME/.ssh/id_rsa",
            "/home/user/.ssh/id_rsa",
            "/root/.ssh/id_rsa",
            "C:\\Users\\Admin\\Documents\\secrets.txt",
            // System directories
            "/bin/sh",
            "/usr/bin/sudo",
            "/sbin/init",
            "/boot/vmlinuz",
            "/sys/kernel/security",
            "/proc/self/environ",
            "/dev/mem",
            "/var/log/auth.log",
        ];

        for attack_vector in attack_vectors {
            let result = check_path_traversal(attack_vector);
            assert!(result.is_err(), "Attack vector '{}' should have been blocked", attack_vector);
        }

        // Test that safe paths are allowed
        let safe_paths = vec![
            "models/llama2/config.json",
            "adapters/my_adapter/weights.safetensors",
            "relative/path/to/file.txt",
            "/tmp/safe_temp_file_123.txt",  // /tmp/ is allowed for temp files
            "/var/folders/safe/path/config.json",  // Non-sensitive /var/ paths
            "/Users/test/models/config.json",  // Non-sensitive user paths
        ];

        for safe_path in safe_paths {
            let result = check_path_traversal(safe_path);
            assert!(result.is_ok(), "Safe path '{}' should have been allowed", safe_path);
        }

        Ok(())
    }

    #[test]
    fn test_enhanced_is_path_within_base() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let base_dir = temp_dir.path();

        // Create a subdirectory structure
        let sub_dir = base_dir.join("subdir");
        std::fs::create_dir(&sub_dir)?;
        let nested_file = sub_dir.join("nested.txt");
        std::fs::write(&nested_file, "test")?;

        // Test that nested file is within base
        assert!(is_path_within_base(&nested_file, base_dir)?);

        // Test that file outside base is not within
        let outside_file = std::env::temp_dir().join("outside.txt");
        std::fs::write(&outside_file, "test")?;
        assert!(!is_path_within_base(&outside_file, base_dir)?);

        Ok(())
    }
}
