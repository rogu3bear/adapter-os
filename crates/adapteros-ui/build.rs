//! Build script for adapteros-ui
//!
//! Sets the UI build version at compile time for version skew detection.

fn main() {
    // Pass Cargo package version as AOS_UI_BUILD_VERSION
    // This ensures the UI WASM binary knows its version for /healthz comparison
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "dev".to_string());
    println!("cargo:rustc-env=AOS_UI_BUILD_VERSION={}", version);

    // Rerun if Cargo.toml changes (version bump)
    println!("cargo:rerun-if-changed=Cargo.toml");
}
