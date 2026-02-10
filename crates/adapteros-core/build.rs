//! Build script for adapteros-core
//!
//! Automatically computes DATABASE_SCHEMA_VERSION from migrations directory.
//! Also emits build metadata (git hash, timestamp, rustc version, build ID)
//! as the single source of truth for the entire workspace.

use std::process::Command;

#[path = "../../build_support/aos_build_id.rs"]
mod aos_build_id;

fn main() {
    // Find workspace root (contains migrations/)
    let workspace_root =
        aos_build_id::find_workspace_root().expect("Could not find workspace root");
    let migrations_dir = workspace_root.join("migrations");

    // Watch the migrations directory for changes
    println!("cargo:rerun-if-changed={}", migrations_dir.display());

    // Watch .git/HEAD for rebuild on branch switch (not .git/index which changes on every add/commit)
    let git_head = workspace_root.join(".git/HEAD");
    if git_head.exists() {
        println!("cargo:rerun-if-changed={}", git_head.display());
    }

    // If build provenance env vars change, make sure we re-run.
    println!("cargo:rerun-if-env-changed=AOS_BUILD_ID");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

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

    // --- Crate version manifest ---
    let cargo_lock = workspace_root.join("Cargo.lock");
    println!("cargo:rerun-if-changed={}", cargo_lock.display());

    const INFERENCE_CRATES: &[&str] = &[
        "adapteros-core",
        "adapteros-crypto",
        "adapteros-config",
        "adapteros-db",
        "adapteros-lora-router",
        "adapteros-lora-worker",
        "adapteros-lora-mlx-ffi",
        "adapteros-lora-kernel-mtl",
        "adapteros-policy",
        "adapteros-server-api",
        "adapteros-server",
        "adapteros-telemetry",
    ];

    match aos_build_id::parse_crate_versions(&workspace_root, INFERENCE_CRATES) {
        Ok(entries) => {
            let manifest_json = aos_build_id::serialize_crate_manifest(&entries);
            println!("cargo:rustc-env=AOS_CRATE_MANIFEST={}", manifest_json);
        }
        Err(e) => {
            println!("cargo:warning=Failed to parse crate versions from Cargo.lock: {e}");
            // Emit empty manifest so the build doesn't break
            println!("cargo:rustc-env=AOS_CRATE_MANIFEST={{\"format\":1,\"crates\":{{}}}}");
        }
    }

    // --- Build metadata ---
    let build = aos_build_id::resolve_workspace_build_id().unwrap_or_else(|e| {
        // Build scripts should never silently produce an unknown build id.
        println!("cargo:warning=Failed to resolve workspace build id: {e}");
        panic!("Failed to resolve workspace build id: {e}");
    });
    let rustc_version = get_rustc_version();

    let (git_desc, ts) = aos_build_id::split_build_id(&build.build_id).unwrap_or_else(|| {
        println!(
            "cargo:warning=Workspace build id is not in canonical {{prefix}}-{{YYYYMMDDHHmmss}} form: {}",
            build.build_id
        );
        panic!("Workspace build id is not canonical: {}", build.build_id);
    });

    println!("cargo:rustc-env=CARGO_GIT_HASH={}", git_desc);
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", ts);
    println!("cargo:rustc-env=AOS_BUILD_ID={}", build.build_id);
    println!("cargo:rustc-env=RUSTC_VERSION={}", rustc_version);
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
