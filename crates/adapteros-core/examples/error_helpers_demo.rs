//! Example demonstrating error helper extension traits
//!
//! This example shows how to use the error helper traits to simplify
//! error handling in AdapterOS code.
//!
//! Run with: cargo run --example error_helpers_demo -p adapteros-core

use adapteros_core::error_helpers::{ConfigErrorExt, DbErrorExt, IoErrorExt, ValidationErrorExt};
use adapteros_core::{AosError, Result};
use std::fs;
use std::path::Path;

// Example 1: Database operations with simplified error handling
fn fetch_adapter_by_id(id: &str) -> Result<String> {
    // Simulate a database query that fails
    let db_result: std::result::Result<String, String> = Err("connection timeout".to_string());

    // Before: .map_err(|e| AosError::Database(format!("Failed to fetch adapter {}: {}", id, e)))?
    // After: Just use .db_context() for concise error handling
    db_result.db_context(|| format!("fetch adapter {}", id))
}

// Example 2: I/O operations with path context
fn read_adapter_manifest(path: &Path) -> Result<String> {
    // Before: fs::read_to_string(path).map_err(|e| AosError::Io(format!("Failed to read {}: {}", path.display(), e)))?
    // After: Use .io_err_path() for automatic path inclusion
    fs::read_to_string(path).io_err_path("read adapter manifest", path)
}

// Example 3: Configuration validation
fn parse_server_port(port_str: &str) -> Result<u16> {
    // Before: port_str.parse().map_err(|e| AosError::Config(format!("Invalid server_port: {}", e)))?
    // After: Use .config_err() for setting context
    port_str.parse::<u16>().config_err("server_port")
}

// Example 4: Field validation
fn validate_adapter_name(name: &str) -> Result<()> {
    if name.is_empty() {
        // Before: return Err(AosError::Validation("Invalid adapter_name: cannot be empty".to_string()))
        // After: Use .validation_err() for field context
        return Err("cannot be empty").validation_err("adapter_name");
    }

    if name.len() > 255 {
        return Err("exceeds maximum length of 255").validation_err("adapter_name");
    }

    Ok(())
}

// Example 5: Multiple error types in one function
fn load_and_validate_adapter(id: &str, path: &Path) -> Result<String> {
    // Validate ID (validation error)
    validate_adapter_name(id)?;

    // Fetch metadata from database (database error)
    let metadata = fetch_adapter_by_id(id)?;

    // Read manifest from disk (I/O error)
    let manifest = read_adapter_manifest(path)?;

    Ok(format!(
        "Loaded adapter {} with metadata: {} and manifest length: {}",
        id,
        metadata,
        manifest.len()
    ))
}

fn main() {
    println!("AdapterOS Error Helpers Demo\n");

    // Example 1: Database error
    println!("Example 1: Database error");
    match fetch_adapter_by_id("code-review-v1") {
        Ok(result) => println!("  Success: {}", result),
        Err(e) => println!("  Error: {}", e),
    }
    println!();

    // Example 2: I/O error with path
    println!("Example 2: I/O error with path context");
    let path = Path::new("/tmp/nonexistent_adapter.toml");
    match read_adapter_manifest(path) {
        Ok(result) => println!("  Success: {}", result),
        Err(e) => println!("  Error: {}", e),
    }
    println!();

    // Example 3: Configuration parsing error
    println!("Example 3: Configuration parsing error");
    match parse_server_port("invalid_port") {
        Ok(port) => println!("  Success: {}", port),
        Err(e) => println!("  Error: {}", e),
    }
    println!();

    // Example 4: Validation errors
    println!("Example 4: Validation errors");

    match validate_adapter_name("") {
        Ok(_) => println!("  Empty name validated successfully"),
        Err(e) => println!("  Error: {}", e),
    }

    let long_name = "a".repeat(300);
    match validate_adapter_name(&long_name) {
        Ok(_) => println!("  Long name validated successfully"),
        Err(e) => println!("  Error: {}", e),
    }

    match validate_adapter_name("valid-adapter-name") {
        Ok(_) => println!("  Valid name accepted"),
        Err(e) => println!("  Error: {}", e),
    }
    println!();

    // Example 5: Chained operations
    println!("Example 5: Multiple error types in one function");
    match load_and_validate_adapter("", Path::new("/tmp/adapter.toml")) {
        Ok(result) => println!("  Success: {}", result),
        Err(e) => println!("  Error: {}", e),
    }
    println!();

    // Show the type of errors
    println!("Error type examples:");
    let errors: Vec<AosError> = vec![
        fetch_adapter_by_id("test").unwrap_err(),
        read_adapter_manifest(Path::new("/tmp/test")).unwrap_err(),
        parse_server_port("bad").unwrap_err(),
        validate_adapter_name("").unwrap_err(),
    ];

    for (i, err) in errors.iter().enumerate() {
        println!("  Error {}: {:?}", i + 1, std::mem::discriminant(err));
    }
}
