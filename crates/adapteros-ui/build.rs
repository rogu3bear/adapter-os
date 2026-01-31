//! Build script for adapteros-ui
//!
//! Sets the UI build version at compile time for version skew detection and
//! writes a workspace build_id file for asset versioning.

fn main() {
    // Pass Cargo package version as AOS_UI_BUILD_VERSION
    // This ensures the UI WASM binary knows its version for /healthz comparison
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "dev".to_string());
    println!("cargo:rustc-env=AOS_UI_BUILD_VERSION={}", version);

    // Build ID file for asset versioning (shared across crates)
    let build_id = resolve_build_id();
    println!("cargo:rustc-env=AOS_BUILD_ID={}", build_id);

    // Rerun if Cargo.toml changes (version bump) or git HEAD changes
    if let Some(workspace_root) = find_workspace_root() {
        println!(
            "cargo:rerun-if-changed={}",
            workspace_root.join(".git/HEAD").display()
        );
    }
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=build.rs");
}

fn resolve_build_id() -> String {
    if let Some(path) = workspace_build_id_path() {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            let trimmed = contents.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }

        let git_hash = get_git_hash();
        let timestamp = get_build_timestamp();
        let build_id = format!("{}-{}", git_hash, timestamp);

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, &build_id);
        return build_id;
    }

    let git_hash = get_git_hash();
    let timestamp = get_build_timestamp();
    format!("{}-{}", git_hash, timestamp)
}

fn workspace_build_id_path() -> Option<std::path::PathBuf> {
    find_workspace_root().map(|root| root.join("target").join("build_id.txt"))
}

/// Get short git commit hash (7 chars) or git describe tag.
fn get_git_hash() -> String {
    if let Ok(output) = std::process::Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty=-dirty"])
        .output()
    {
        if output.status.success() {
            let desc = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if desc.len() == 40 || desc.starts_with(|c: char| c.is_ascii_hexdigit()) {
                return desc.chars().take(7).collect();
            }
            return desc;
        }
    }

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

/// Get build timestamp in compact format: YYYYMMDDHHmmss
fn get_build_timestamp() -> String {
    if let Ok(epoch) = std::env::var("SOURCE_DATE_EPOCH") {
        if let Ok(secs) = epoch.parse::<i64>() {
            return format_unix_timestamp(secs);
        }
    }

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

/// Format unix timestamp to YYYYMMDDHHmmss
fn format_unix_timestamp(secs: i64) -> String {
    if let Ok(output) = std::process::Command::new("date")
        .args(["-u", "-r", &secs.to_string(), "+%Y%m%d%H%M%S"])
        .output()
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }

    if let Ok(output) = std::process::Command::new("date")
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
