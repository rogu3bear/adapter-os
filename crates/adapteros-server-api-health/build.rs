//! Build script for adapteros-server-api-health
//!
//! Ensures `AOS_BUILD_ID` is set at compile time so `/healthz` always reports a
//! canonical build identifier.

#[path = "../../build_support/aos_build_id.rs"]
mod aos_build_id;

fn main() {
    println!("cargo:rerun-if-env-changed=AOS_BUILD_ID");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    let build = aos_build_id::resolve_workspace_build_id().unwrap_or_else(|e| {
        println!("cargo:warning=Failed to resolve workspace build id: {e}");
        panic!("Failed to resolve workspace build id: {e}");
    });

    println!("cargo:rustc-env=AOS_BUILD_ID={}", build.build_id);

    if let Some(root) = aos_build_id::find_workspace_root() {
        println!(
            "cargo:rerun-if-changed={}",
            root.join(".git/HEAD").display()
        );
    }
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=build.rs");
}
