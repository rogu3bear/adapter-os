//! Tests for migration signing functionality

use std::fs;
use std::path::Path;

#[test]
fn test_migration_file_discovery() {
    // Test that migration files can be discovered
    let migrations_dir = Path::new("../../migrations");

    if migrations_dir.exists() {
        let entries = fs::read_dir(migrations_dir).expect("Failed to read migrations directory");

        let sql_files: Vec<_> = entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext == "sql")
                    .unwrap_or(false)
            })
            .collect();

        // Should have some SQL migration files
        assert!(!sql_files.is_empty(), "No SQL migration files found");
    } else {
        // If migrations directory doesn't exist in test environment, skip
        eprintln!("Migrations directory not found, skipping discovery test");
    }
}

#[test]
fn test_migration_signature_format() {
    // Test basic signature file format expectations
    let signatures_file = Path::new("../../migrations/signatures.json");

    if signatures_file.exists() {
        let content = fs::read_to_string(signatures_file).expect("Failed to read signatures file");

        // Should be valid JSON
        serde_json::from_str::<serde_json::Value>(&content)
            .expect("Signatures file should contain valid JSON");

        // Should not be empty
        assert!(
            !content.trim().is_empty(),
            "Signatures file should not be empty"
        );
    } else {
        // If signatures file doesn't exist, skip test
        eprintln!("Signatures file not found, skipping format test");
    }
}

#[test]
fn test_migration_file_naming() {
    // Test that migration files follow naming conventions
    let migrations_dir = Path::new("../../migrations");

    if migrations_dir.exists() {
        let entries = fs::read_dir(migrations_dir).expect("Failed to read migrations directory");

        for entry in entries.filter_map(|e| e.ok()) {
            let file_name = entry.file_name().to_string_lossy().into_owned();

            // Should start with numbers (migration order)
            if file_name.ends_with(".sql") {
                assert!(
                    file_name.chars().next().unwrap_or(' ').is_numeric(),
                    "Migration file '{}' should start with a number",
                    file_name
                );
            }
        }
    }
}
