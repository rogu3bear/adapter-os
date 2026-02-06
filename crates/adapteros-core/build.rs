//! Build script for adapteros-core
//!
//! Automatically computes DATABASE_SCHEMA_VERSION from migrations directory.
//! Also emits build metadata (git hash, timestamp, rustc version, build ID)
//! as the single source of truth for the entire workspace.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Find workspace root (contains migrations/)
    let workspace_root = find_workspace_root().expect("Could not find workspace root");
    let migrations_dir = workspace_root.join("migrations");

    // Watch the migrations directory for changes
    println!("cargo:rerun-if-changed={}", migrations_dir.display());

    // Watch .git/HEAD for rebuild on branch switch (not .git/index which changes on every add/commit)
    let git_head = workspace_root.join(".git/HEAD");
    if git_head.exists() {
        println!("cargo:rerun-if-changed={}", git_head.display());
    }

    // Compute highest migration number
    let max_migration = compute_max_migration_number(&migrations_dir)
        .expect("Failed to compute max migration number from migrations directory");

    // Export as env var for version.rs to consume
    println!("cargo:rustc-env=DATABASE_SCHEMA_VERSION={}", max_migration);

    // Also emit warning if we found zero migrations (something's wrong)
    if max_migration == 0 {
        println!(
            "cargo:warning=No migrations found in {}. DATABASE_SCHEMA_VERSION will be 0.",
            migrations_dir.display()
        );
    }

    // --- Build metadata ---
    let git_hash = get_git_hash();
    let timestamp = get_build_timestamp();
    let build_id = format!("{}-{}", git_hash, timestamp);
    let rustc_version = get_rustc_version();

    println!("cargo:rustc-env=CARGO_GIT_HASH={}", git_hash);
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", timestamp);
    println!("cargo:rustc-env=AOS_BUILD_ID={}", build_id);
    println!("cargo:rustc-env=RUSTC_VERSION={}", rustc_version);

    // Write build_id.txt to workspace target for cross-crate/script access
    if let Some(workspace_target) = find_workspace_target() {
        let _ = std::fs::write(workspace_target.join("build_id.txt"), &build_id);
    }
}

/// Get short git commit hash via `git describe --tags --always --dirty=-dirty`
fn get_git_hash() -> String {
    if let Ok(output) = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty=-dirty"])
        .output()
    {
        if output.status.success() {
            let desc = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // If it's just a bare hash (no tag), truncate to 7 chars
            if !desc.contains('-') && desc.len() >= 7 && desc.chars().all(|c| c.is_ascii_hexdigit())
            {
                return desc.chars().take(7).collect();
            }
            return desc;
        }
    }

    // Fallback to rev-parse
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }

    "unknown".to_string()
}

/// Get build timestamp in compact format: YYYYMMDDHHmmss
fn get_build_timestamp() -> String {
    // Use SOURCE_DATE_EPOCH for reproducible builds if set
    if let Ok(epoch) = std::env::var("SOURCE_DATE_EPOCH") {
        if let Ok(secs) = epoch.parse::<i64>() {
            if let Ok(output) = Command::new("date")
                .args(["-u", "-r", &secs.to_string(), "+%Y%m%d%H%M%S"])
                .output()
            {
                if output.status.success() {
                    return String::from_utf8_lossy(&output.stdout).trim().to_string();
                }
            }
        }
    }

    if let Ok(output) = Command::new("date").args(["-u", "+%Y%m%d%H%M%S"]).output() {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }

    "00000000000000".to_string()
}

/// Get rustc version string
fn get_rustc_version() -> String {
    if let Ok(output) = Command::new("rustc").arg("--version").output() {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }
    "unknown".to_string()
}

/// Find workspace root by walking up from CARGO_MANIFEST_DIR
fn find_workspace_root() -> Option<PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let mut path = PathBuf::from(manifest_dir);

    // Walk up to find workspace root (has [workspace] in Cargo.toml)
    while let Some(parent) = path.parent() {
        let cargo_toml = parent.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(contents) = std::fs::read_to_string(&cargo_toml) {
                if contents.contains("[workspace]") {
                    return Some(parent.to_path_buf());
                }
            }
        }
        path = parent.to_path_buf();
    }

    None
}

/// Find workspace target directory for cross-crate build artifact sharing
fn find_workspace_target() -> Option<PathBuf> {
    find_workspace_root().map(|root| root.join("target"))
}

/// Compute the highest migration number from the migrations directory
///
/// Supports two naming conventions:
/// 1. Four-digit prefix: `0001_init.sql`, `0297_embedding_benchmarks.sql`
/// 2. Timestamp prefix: `20260112125636_add_dataset_validation_json.sql`
///
/// Returns the maximum numeric prefix found (as u32).
///
/// # Note on Timestamp Format
///
/// Timestamp-format migrations (14+ digits) will overflow u32::MAX (4,294,967,295).
/// In practice, the system uses the 4-digit sequential format as the canonical
/// migration sequence. Timestamp migrations are rare and not part of the primary
/// migration numbering scheme. If a timestamp migration is the highest number found,
/// it will be silently ignored due to parse failure, and the highest 4-digit
/// migration will be used instead.
fn compute_max_migration_number(migrations_dir: &std::path::Path) -> std::io::Result<u32> {
    let mut max_number = 0u32;

    if !migrations_dir.exists() {
        return Ok(0);
    }

    for entry in std::fs::read_dir(migrations_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only look at .sql files
        if path.extension().and_then(|s| s.to_str()) != Some("sql") {
            continue;
        }

        // Extract filename
        if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
            // Try to extract numeric prefix (before first underscore)
            if let Some(prefix) = filename.split('_').next() {
                // Parse as u32 (handles both 4-digit and timestamp formats)
                if let Ok(number) = prefix.parse::<u32>() {
                    max_number = max_number.max(number);
                }
            }
        }
    }

    Ok(max_number)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_compute_max_migration_four_digit() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Create sample migrations with 4-digit prefix
        fs::write(temp_dir.path().join("0001_init.sql"), "").unwrap();
        fs::write(temp_dir.path().join("0002_patch_proposals.sql"), "").unwrap();
        fs::write(temp_dir.path().join("0297_embedding_benchmarks.sql"), "").unwrap();

        let max = compute_max_migration_number(temp_dir.path()).unwrap();
        assert_eq!(max, 297);
    }

    #[test]
    fn test_compute_max_migration_timestamp() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Create sample migrations with timestamp prefix
        fs::write(
            temp_dir
                .path()
                .join("20260112125636_add_dataset_validation_json.sql"),
            "",
        )
        .unwrap();
        fs::write(
            temp_dir
                .path()
                .join("20260120085000_add_identity_datasets.sql"),
            "",
        )
        .unwrap();

        let max = compute_max_migration_number(temp_dir.path()).unwrap();
        assert_eq!(max, 20260120085000);
    }

    #[test]
    fn test_compute_max_migration_mixed() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Mix of both formats (real scenario)
        fs::write(temp_dir.path().join("0297_embedding_benchmarks.sql"), "").unwrap();
        fs::write(
            temp_dir
                .path()
                .join("20260112125636_add_dataset_validation_json.sql"),
            "",
        )
        .unwrap();

        let max = compute_max_migration_number(temp_dir.path()).unwrap();
        // Timestamp format is numerically larger
        assert_eq!(max, 20260112125636);
    }

    #[test]
    fn test_compute_max_migration_empty_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let max = compute_max_migration_number(temp_dir.path()).unwrap();
        assert_eq!(max, 0);
    }

    #[test]
    fn test_compute_max_migration_nonexistent_dir() {
        let max = compute_max_migration_number(&std::path::PathBuf::from("/nonexistent")).unwrap();
        assert_eq!(max, 0);
    }
}
