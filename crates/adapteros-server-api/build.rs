//! Build script for adapteros-server-api
//!
//! Reads the build ID from target/build_id.txt (written by adapteros-core/build.rs)
//! and re-exports it as AOS_BUILD_ID for the health endpoint.

use std::path::PathBuf;

fn main() {
    // Try to read build_id.txt from workspace target (written by adapteros-core/build.rs)
    let build_id = read_workspace_build_id().unwrap_or_else(|| {
        // Fallback: compute locally if core hasn't written the file yet
        let hash = get_git_hash();
        let ts = get_build_timestamp();
        format!("{}-{}", hash, ts)
    });

    println!("cargo:rustc-env=AOS_BUILD_ID={}", build_id);

    // Only rerun on branch changes
    if let Some(root) = find_workspace_root() {
        println!("cargo:rerun-if-changed={}", root.join(".git/HEAD").display());
    }
}

fn read_workspace_build_id() -> Option<String> {
    let target = find_workspace_root()?.join("target").join("build_id.txt");
    let content = std::fs::read_to_string(target).ok()?;
    let trimmed = content.trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

fn get_git_hash() -> String {
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }
    "unknown".to_string()
}

fn get_build_timestamp() -> String {
    if let Ok(output) = std::process::Command::new("date")
        .args(["-u", "+%Y%m%d%H%M%S"])
        .output()
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }
    "00000000000000".to_string()
}

fn find_workspace_root() -> Option<PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let mut path = PathBuf::from(manifest_dir);
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
