//! Build script for adapteros-server
//!
//! Generates a unified build ID combining git commit hash and build timestamp.
//! Format: {7-char-git-hash}-{YYYYMMDDHHmmss} e.g., "a6922d2-20260122153045"
//!
//! This ID is used for:
//! - Cache busting (UI assets, API responses)
//! - Log file naming and correlation
//! - Build traceability and debugging

#[path = "../../build_support/aos_build_id.rs"]
mod aos_build_id;

fn main() {
    // Rerun if git HEAD changes or Cargo.toml version changes
    // NOTE: We only watch .git/HEAD, not .git/index
    // Watching .git/index triggers rebuilds on every git add/commit/stash, killing incremental build performance
    // .git/HEAD changes when branches switch, which is when we actually need a new build ID
    if let Some(workspace_root) = aos_build_id::find_workspace_root() {
        println!(
            "cargo:rerun-if-changed={}",
            workspace_root.join(".git/HEAD").display()
        );
    }
    println!("cargo:rerun-if-changed=Cargo.toml");
    // Always rerun to get fresh timestamp
    println!("cargo:rerun-if-changed=build.rs");

    println!("cargo:rerun-if-env-changed=AOS_BUILD_ID");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    let build = aos_build_id::resolve_workspace_build_id().unwrap_or_else(|e| {
        println!("cargo:warning=Failed to resolve workspace build id: {e}");
        panic!("Failed to resolve workspace build id: {e}");
    });

    let (git_desc, ts) = aos_build_id::split_build_id(&build.build_id).unwrap_or_else(|| {
        println!(
            "cargo:warning=Workspace build id is not in canonical {{prefix}}-{{YYYYMMDDHHmmss}} form: {}",
            build.build_id
        );
        panic!("Workspace build id is not canonical: {}", build.build_id);
    });

    println!("cargo:rustc-env=AOS_BUILD_ID={}", build.build_id);
    println!("cargo:rustc-env=AOS_BUILD_GIT_HASH={}", git_desc);
    println!("cargo:rustc-env=AOS_BUILD_TIMESTAMP={}", ts);
}
