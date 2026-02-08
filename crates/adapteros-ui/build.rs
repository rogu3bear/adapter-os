//! Build script for adapteros-ui
//!
//! Sets the UI build version at compile time for version skew detection and
//! writes a workspace build_id file for asset versioning.

#[path = "../../build_support/aos_build_id.rs"]
mod aos_build_id;

fn main() {
    // Pass Cargo package version as AOS_UI_BUILD_VERSION
    // This ensures the UI WASM binary knows its version for /healthz comparison
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "dev".to_string());
    println!("cargo:rustc-env=AOS_UI_BUILD_VERSION={}", version);

    println!("cargo:rerun-if-env-changed=AOS_BUILD_ID");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    // Build ID file for asset versioning (shared across crates)
    let build = aos_build_id::resolve_workspace_build_id().unwrap_or_else(|e| {
        println!("cargo:warning=Failed to resolve workspace build id: {e}");
        panic!("Failed to resolve workspace build id: {e}");
    });
    println!("cargo:rustc-env=AOS_BUILD_ID={}", build.build_id);

    // Rerun if Cargo.toml changes (version bump) or git HEAD changes
    if let Some(workspace_root) = aos_build_id::find_workspace_root() {
        println!(
            "cargo:rerun-if-changed={}",
            workspace_root.join(".git/HEAD").display()
        );
    }
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=build.rs");
}
