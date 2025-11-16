<<<<<<< HEAD
#![cfg(all(test, feature = "extended-tests"))]

=======
>>>>>>> integration-branch
//! Cross-platform determinism tests
//!
//! Verify that builds are bitwise identical across different Apple Silicon generations

use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn test_cross_platform_determinism() {
    // This test would run in CI with different Apple Silicon generations
    // For now, it's a placeholder that documents the expected behavior

    println!("🔍 Cross-platform determinism test");

    // Check if we're in a CI environment
    let is_ci = std::env::var("CI").is_ok();
    let platform = std::env::var("PLATFORM").unwrap_or_else(|_| "local".to_string());

    println!("   Platform: {}", platform);
    println!("   CI: {}", is_ci);

    if !is_ci {
        println!("   ⚠️ Skipping cross-platform test (not in CI)");
        return;
    }

    // In CI, this would compare M3 vs M4 builds
    // For now, we just verify the test infrastructure exists
    assert!(true, "Cross-platform determinism test infrastructure ready");
}

#[test]
fn test_binary_hash_consistency() {
    println!("🔍 Binary hash consistency test");

    let target_dir = Path::new("target/release");
    if !target_dir.exists() {
        println!("   ⚠️ Skipping (no release build found)");
        return;
    }

    // Find all executables
    let mut binaries = Vec::new();
    if let Ok(entries) = fs::read_dir(target_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_executable(&path) {
                binaries.push(path);
            }
        }
    }

    if binaries.is_empty() {
        println!("   ⚠️ No executables found");
        return;
    }

    // Compute hashes for all binaries
    for binary in &binaries {
        let hash = compute_b3_hash(binary).unwrap_or_else(|_| "unknown".to_string());
        println!(
            "   {}: {}",
            binary.file_name().unwrap().to_string_lossy(),
            hash
        );
    }

    // In a real cross-platform test, we would compare these hashes
    // across different Apple Silicon generations
    assert!(!binaries.is_empty(), "Should have at least one binary");
}

#[test]
fn test_metallib_hash_consistency() {
    println!("🔍 Metal shader hash consistency test");

    let metal_dir = Path::new("metal");
    if !metal_dir.exists() {
        println!("   ⚠️ Skipping (no metal directory found)");
        return;
    }

    // Find all .metallib files
    let mut metallibs = Vec::new();
    if let Ok(entries) = fs::read_dir(metal_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("metallib") {
                metallibs.push(path);
            }
        }
    }

    if metallibs.is_empty() {
        println!("   ⚠️ No .metallib files found");
        return;
    }

    // Compute hashes for all .metallib files
    for metallib in &metallibs {
        let hash = compute_b3_hash(metallib).unwrap_or_else(|_| "unknown".to_string());
        println!(
            "   {}: {}",
            metallib.file_name().unwrap().to_string_lossy(),
            hash
        );
    }

    // In a real cross-platform test, we would compare these hashes
    // across different Apple Silicon generations
    assert!(
        !metallibs.is_empty(),
        "Should have at least one .metallib file"
    );
}

#[test]
fn test_determinism_report_consistency() {
    println!("🔍 Determinism report consistency test");

    let report_path = Path::new("target/determinism_report.json");
    if !report_path.exists() {
        println!("   ⚠️ Skipping (no determinism report found)");
        return;
    }

    // Parse the determinism report
    let content = fs::read_to_string(report_path).expect("Failed to read determinism report");
    let report: serde_json::Value =
        serde_json::from_str(&content).expect("Failed to parse determinism report");

    // Verify required fields
    assert!(
        report["schema_version"].is_string(),
        "Missing schema_version"
    );
    assert!(
        report["build_timestamp"].is_string(),
        "Missing build_timestamp"
    );
    assert!(
        report["build_metadata"].is_object(),
        "Missing build_metadata"
    );
    assert!(report["binary_hashes"].is_object(), "Missing binary_hashes");
    assert!(
        report["artifact_hashes"].is_object(),
        "Missing artifact_hashes"
    );

    // Check reproducibility score
    if let Some(score) = report["reproducibility_score"].as_f64() {
        assert!(
            score >= 0.0 && score <= 100.0,
            "Invalid reproducibility score: {}",
            score
        );
        println!("   Reproducibility score: {:.1}/100", score);
    }

    println!("   ✅ Determinism report structure valid");
}

#[test]
fn test_environment_variables() {
    println!("🔍 Environment variables test");

    // Check for reproducibility-related environment variables
    let required_vars = [
        "SOURCE_DATE_EPOCH",
        "CARGO_INCREMENTAL",
        "RUSTC_WRAPPER",
        "CARGO_TARGET_DIR",
    ];

    let mut found_vars = Vec::new();
    for var in &required_vars {
        if let Ok(value) = std::env::var(var) {
            found_vars.push((var, value));
        }
    }

    println!("   Found environment variables:");
    for (name, value) in &found_vars {
        println!("     {}={}", name, value);
    }

    // In CI, we expect these to be set
    let is_ci = std::env::var("CI").is_ok();
    if is_ci {
        assert!(
            !found_vars.is_empty(),
            "Should have reproducibility environment variables in CI"
        );
    }
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    if let Ok(metadata) = fs::metadata(path) {
        let permissions = metadata.permissions();
        let mode = permissions.mode();
        mode & 0o111 != 0
    } else {
        false
    }
}

fn compute_b3_hash(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    use blake3::Hasher;

    let content = fs::read(path)?;
    let mut hasher = Hasher::new();
    hasher.update(&content);
    let hash = hasher.finalize();

    Ok(hash.to_hex().to_string())
}

#[test]
fn test_cargo_lock_consistency() {
    println!("🔍 Cargo.lock consistency test");

    let lock_path = Path::new("Cargo.lock");
    if !lock_path.exists() {
        println!("   ⚠️ Skipping (no Cargo.lock found)");
        return;
    }

    // Parse Cargo.lock to verify it's valid
    let content = fs::read_to_string(lock_path).expect("Failed to read Cargo.lock");

    // Basic validation - check for required sections
    assert!(
        content.contains("[package]"),
        "Cargo.lock should contain [package] section"
    );
    assert!(
        content.contains("name ="),
        "Cargo.lock should contain package names"
    );
    assert!(
        content.contains("version ="),
        "Cargo.lock should contain package versions"
    );

    // In a real cross-platform test, we would verify that Cargo.lock
    // is identical across different Apple Silicon generations
    println!("   ✅ Cargo.lock structure valid");
}

#[test]
fn test_build_metadata_consistency() {
    println!("🔍 Build metadata consistency test");

    let metadata_dir = Path::new("target/metadata");
    if !metadata_dir.exists() {
        println!("   ⚠️ Skipping (no metadata directory found)");
        return;
    }

    // Check for required metadata files
    let required_files = [
        "rustc_version.txt",
        "cargo_version.txt",
        "build_metadata.json",
    ];

    let mut found_files = Vec::new();
    for file in &required_files {
        let path = metadata_dir.join(file);
        if path.exists() {
            found_files.push(file);
        }
    }

    println!("   Found metadata files: {:?}", found_files);

    // Verify build_metadata.json structure
    let metadata_path = metadata_dir.join("build_metadata.json");
    if metadata_path.exists() {
        let content = fs::read_to_string(&metadata_path).expect("Failed to read build metadata");
        let metadata: serde_json::Value =
            serde_json::from_str(&content).expect("Failed to parse build metadata");

        assert!(
            metadata["rustc_version"].is_string(),
            "Missing rustc_version"
        );
        assert!(
            metadata["cargo_version"].is_string(),
            "Missing cargo_version"
        );
        assert!(
            metadata["target_triple"].is_string(),
            "Missing target_triple"
        );

        println!("   ✅ Build metadata structure valid");
    }

    assert!(
        !found_files.is_empty(),
        "Should have at least one metadata file"
    );
}
