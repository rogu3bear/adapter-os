//! Build script for adapteros-server
//!
//! Generates a unified build ID combining git commit hash and build timestamp.
//! Format: {7-char-git-hash}-{YYYYMMDDHHmmss} e.g., "a6922d2-20260122153045"
//!
//! This ID is used for:
//! - Cache busting (UI assets, API responses)
//! - Log file naming and correlation
//! - Build traceability and debugging

use std::process::Command;

fn main() {
    // Rerun if git HEAD changes or Cargo.toml version changes
    // Paths must be relative to workspace root for git files
    if let Some(workspace_root) = find_workspace_root() {
        println!(
            "cargo:rerun-if-changed={}",
            workspace_root.join(".git/HEAD").display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            workspace_root.join(".git/index").display()
        );
    }
    println!("cargo:rerun-if-changed=Cargo.toml");
    // Always rerun to get fresh timestamp
    println!("cargo:rerun-if-changed=build.rs");

    let git_hash = get_git_hash();
    let timestamp = get_build_timestamp();

    // Combined format: short git hash + timestamp, lean and traceable
    let build_id = format!("{}-{}", git_hash, timestamp);

    println!("cargo:rustc-env=AOS_BUILD_ID={}", build_id);
    println!("cargo:rustc-env=AOS_BUILD_GIT_HASH={}", git_hash);
    println!("cargo:rustc-env=AOS_BUILD_TIMESTAMP={}", timestamp);

    // Also write to file for other tools (UI build, scripts)
    if let Ok(out_dir) = std::env::var("OUT_DIR") {
        let build_info_path = std::path::PathBuf::from(&out_dir).join("build_id.txt");
        let _ = std::fs::write(&build_info_path, &build_id);

        // Write to workspace target for cross-crate access
        if let Some(workspace_target) = find_workspace_target() {
            let workspace_build_id = workspace_target.join("build_id.txt");
            let _ = std::fs::write(&workspace_build_id, &build_id);
        }
    }
}

/// Get short git commit hash (7 chars) or "unknown" if not in a git repo
fn get_git_hash() -> String {
    // First try git describe for tagged versions
    if let Ok(output) = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty=-dirty"])
        .output()
    {
        if output.status.success() {
            let desc = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // If it's just a hash (no tag), truncate to 7 chars
            if desc.len() == 40 || desc.starts_with(|c: char| c.is_ascii_hexdigit()) {
                return desc.chars().take(7).collect();
            }
            // If tagged, return the full description (e.g., "v0.13.1-3-ga6922d2")
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
    // Use SOURCE_DATE_EPOCH for reproducible builds if set (per .cargo/config.toml)
    if let Ok(epoch) = std::env::var("SOURCE_DATE_EPOCH") {
        if let Ok(secs) = epoch.parse::<i64>() {
            return format_unix_timestamp(secs);
        }
    }

    // Otherwise use current time
    if let Ok(output) = Command::new("date").args(["-u", "+%Y%m%d%H%M%S"]).output() {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }

    // Ultimate fallback
    "00000000000000".to_string()
}

/// Format unix timestamp to YYYYMMDDHHmmss
fn format_unix_timestamp(secs: i64) -> String {
    // Simple manual formatting to avoid chrono dependency in build script
    if let Ok(output) = Command::new("date")
        .args(["-u", "-r", &secs.to_string(), "+%Y%m%d%H%M%S"])
        .output()
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }

    // GNU date fallback
    if let Ok(output) = Command::new("date")
        .args(["-u", "-d", &format!("@{}", secs), "+%Y%m%d%H%M%S"])
        .output()
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }

    "00000000000000".to_string()
}

/// Find workspace root directory (contains [workspace] in Cargo.toml)
fn find_workspace_root() -> Option<std::path::PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let mut path = std::path::PathBuf::from(manifest_dir);

    // Walk up to find workspace root (has [workspace] in Cargo.toml)
    while path.parent().is_some() {
        let cargo_toml = path.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(contents) = std::fs::read_to_string(&cargo_toml) {
                if contents.contains("[workspace]") {
                    return Some(path);
                }
            }
        }
        path = path.parent()?.to_path_buf();
    }

    None
}

/// Find workspace target directory for cross-crate build artifact sharing
fn find_workspace_target() -> Option<std::path::PathBuf> {
    find_workspace_root().map(|root| root.join("target"))
}
