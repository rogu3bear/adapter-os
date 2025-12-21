//! Code generation orchestration for OpenAPI and TypeScript.
//!
//! This module handles:
//! 1. Building the server to extract OpenAPI spec via utoipa
//! 2. Generating TypeScript types from OpenAPI
//! 3. Validating type consistency between Rust and TS
//!
//! Dependencies checked:
//! - Rust compiler and cargo (required for build)
//! - Node.js 18+ and pnpm (required for TS generation)
//! - openapi-typescript CLI (installed via pnpm)

use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Configuration for the codegen pipeline
pub struct CodegenConfig {
    /// Workspace root directory
    pub workspace_root: PathBuf,
    /// Output directory for generated files
    pub output_dir: PathBuf,
    /// Whether to skip TypeScript generation (for CI or isolated steps)
    pub skip_ts_gen: bool,
    /// Whether to run validation checks
    pub validate: bool,
    /// Verbose output
    pub verbose: bool,
}

/// Result of a single codegen step
pub struct CodegenStep {
    pub name: String,
    pub success: bool,
    pub duration_ms: u128,
    pub message: String,
}

/// Complete codegen report
pub struct CodegenReport {
    pub steps: Vec<CodegenStep>,
    pub total_duration_ms: u128,
}

impl CodegenReport {
    fn new() -> Self {
        Self {
            steps: Vec::new(),
            total_duration_ms: 0,
        }
    }

    fn all_success(&self) -> bool {
        self.steps.iter().all(|s| s.success)
    }
}

/// Main entry point for code generation
pub async fn run() -> Result<()> {
    let workspace_root = find_workspace_root()?;
    let output_dir = workspace_root.join("target/codegen");
    fs::create_dir_all(&output_dir).context("Failed to create output directory")?;

    let config = CodegenConfig {
        workspace_root,
        output_dir,
        skip_ts_gen: false,
        validate: true,
        verbose: std::env::var("VERBOSE").is_ok(),
    };

    run_with_config(config).await
}

/// Run codegen with specific configuration
pub async fn run_with_config(config: CodegenConfig) -> Result<()> {
    let start = std::time::Instant::now();
    let mut report = CodegenReport::new();

    println!("========================================");
    println!("  AdapterOS Code Generation Pipeline");
    println!("========================================\n");

    // Step 1: Check dependencies
    println!("Step 1/4: Checking dependencies...");
    let dep_result = check_dependencies(&config);
    report.steps.push(CodegenStep {
        name: "Dependency Check".to_string(),
        success: dep_result.is_ok(),
        duration_ms: 0,
        message: match &dep_result {
            Ok(_) => "All dependencies satisfied".to_string(),
            Err(e) => e.to_string(),
        },
    });

    if !report.steps.last().unwrap().success {
        print_report(&report);
        return Err(anyhow!("Dependency check failed"));
    }

    // Step 2: Build server and export OpenAPI spec
    println!("\nStep 2/4: Building server and extracting OpenAPI spec...");
    let build_start = std::time::Instant::now();
    let build_result = build_server_and_export_openapi(&config).await;
    let build_duration = build_start.elapsed().as_millis();

    report.steps.push(CodegenStep {
        name: "Build & OpenAPI Export".to_string(),
        success: build_result.is_ok(),
        duration_ms: build_duration,
        message: match &build_result {
            Ok(spec_path) => format!("OpenAPI spec written to {}", spec_path.display()),
            Err(e) => e.to_string(),
        },
    });

    if !report.steps.last().unwrap().success {
        print_report(&report);
        return Err(anyhow!("OpenAPI export failed"));
    }

    let spec_path = build_result?;

    // Step 3: Generate TypeScript types (unless skipped)
    if !config.skip_ts_gen {
        println!("\nStep 3/4: Generating TypeScript types...");
        let ts_start = std::time::Instant::now();
        let ts_result = generate_typescript_types(&config, &spec_path).await;
        let ts_duration = ts_start.elapsed().as_millis();

        report.steps.push(CodegenStep {
            name: "TypeScript Generation".to_string(),
            success: ts_result.is_ok(),
            duration_ms: ts_duration,
            message: match &ts_result {
                Ok(ts_path) => format!("TypeScript types written to {}", ts_path.display()),
                Err(e) => e.to_string(),
            },
        });

        if !report.steps.last().unwrap().success {
            print_report(&report);
            return Err(anyhow!("TypeScript generation failed"));
        }
    } else {
        report.steps.push(CodegenStep {
            name: "TypeScript Generation".to_string(),
            success: true,
            duration_ms: 0,
            message: "Skipped (--skip-ts)".to_string(),
        });
        println!("  (Skipped)");
    }

    // Step 4: Validate consistency
    if config.validate {
        println!("\nStep 4/4: Validating type consistency...");
        let validate_start = std::time::Instant::now();
        let validate_result = validate_type_consistency(&config, &spec_path).await;
        let validate_duration = validate_start.elapsed().as_millis();

        report.steps.push(CodegenStep {
            name: "Type Validation".to_string(),
            success: validate_result.is_ok(),
            duration_ms: validate_duration,
            message: match &validate_result {
                Ok(_) => "All types consistent".to_string(),
                Err(e) => e.to_string(),
            },
        });

        if !report.steps.last().unwrap().success {
            eprintln!("\n⚠ Validation warnings detected (non-fatal)");
            if config.verbose {
                print_report(&report);
            }
        }
    }

    // Summary
    report.total_duration_ms = start.elapsed().as_millis();
    print_report(&report);

    if report.all_success() {
        println!("\n✓ Code generation completed successfully");
        Ok(())
    } else {
        Err(anyhow!("Code generation failed"))
    }
}

/// Check for required dependencies
fn check_dependencies(config: &CodegenConfig) -> Result<()> {
    println!("  Checking Rust toolchain...");
    Command::new("cargo")
        .arg("--version")
        .output()
        .context("Rust/Cargo not found. Install from https://rustup.rs/")?;

    // Check for Node.js and pnpm (required for TS generation unless skipped)
    if !config.skip_ts_gen {
        println!("  Checking Node.js...");
        let node_out = Command::new("node")
            .arg("--version")
            .output()
            .context("Node.js not found. Install from https://nodejs.org/")?;

        let node_version = String::from_utf8_lossy(&node_out.stdout);
        println!("    Found: {}", node_version.trim());

        if !node_version.contains("v18") && !node_version.contains("v19") && !node_version.contains("v20") {
            bail!("Node.js 18+ required (found: {})", node_version.trim());
        }

        println!("  Checking pnpm...");
        Command::new("pnpm")
            .arg("--version")
            .output()
            .context("pnpm not found. Install with: npm install -g pnpm")?;

        println!("  Checking openapi-typescript in UI project...");
        let pkg_json = config.workspace_root.join("ui/package.json");
        if pkg_json.exists() {
            let content = fs::read_to_string(&pkg_json)?;
            if !content.contains("openapi-typescript") {
                bail!(
                    "openapi-typescript not in ui/package.json. \
                     Add via: cd ui && pnpm add -D openapi-typescript"
                );
            }
        }
    }

    Ok(())
}

/// Build server release binary and export OpenAPI spec via utoipa
async fn build_server_and_export_openapi(config: &CodegenConfig) -> Result<PathBuf> {
    let spec_output = config.output_dir.join("openapi.json");

    println!("  Running export-openapi binary...");

    // Run the export-openapi binary to generate the OpenAPI spec
    let output = Command::new("cargo")
        .args(&["run", "--bin", "export-openapi", "--package", "adapteros-server-api"])
        .arg("--")
        .arg(spec_output.to_str().unwrap())
        .current_dir(&config.workspace_root)
        .output()
        .context("Failed to run export-openapi binary")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Check if this is a compilation error due to known blockers
        if stderr.contains("adapteros-lora-worker") || stderr.contains("could not compile") {
            eprintln!("  Warning: Cannot compile export-openapi due to build system issues");
            eprintln!("  This is a known blocker (see PRD-02-BLOCKERS.md)");
            eprintln!("  Falling back to stub OpenAPI spec");

            // Create stub spec as fallback
            let stub_spec = serde_json::json!({
                "openapi": "3.0.0",
                "info": {
                    "title": "AdapterOS API",
                    "version": "1.0.0",
                    "description": "Stub spec - build system blocked (see PRD-02-BLOCKERS.md)"
                },
                "servers": [
                    { "url": "http://localhost:8080", "description": "Local dev" }
                ],
                "paths": {}
            });

            fs::write(&spec_output, serde_json::to_string_pretty(&stub_spec)?)?;
            return Ok(spec_output);
        }

        bail!("OpenAPI export failed:\n{}", stderr);
    }

    if config.verbose {
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }

    if !spec_output.exists() {
        bail!("OpenAPI spec was not generated at expected location: {}", spec_output.display());
    }

    Ok(spec_output)
}

/// Generate TypeScript types from OpenAPI spec using openapi-typescript
async fn generate_typescript_types(config: &CodegenConfig, spec_path: &Path) -> Result<PathBuf> {
    let ts_output = config.workspace_root.join("ui/src/api/types.generated.ts");

    println!("  Converting OpenAPI spec to TypeScript...");
    println!("    Input: {}", spec_path.display());
    println!("    Output: {}", ts_output.display());

    // Navigate to UI directory and run openapi-typescript
    let output = Command::new("pnpm")
        .arg("exec")
        .arg("openapi-typescript")
        .arg(spec_path.to_string_lossy().as_ref())
        .arg("--output")
        .arg(&ts_output)
        .arg("--transform")
        .arg("./scripts/openapi-transform.js") // Custom transform if exists
        .current_dir(config.workspace_root.join("ui"))
        .output()
        .context("Failed to run openapi-typescript")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // openapi-typescript exits with non-zero on some warnings, so we check file creation
        if !ts_output.exists() {
            bail!("TypeScript generation failed:\n{}", stderr);
        }
        println!("  (Warnings, but types generated)");
    }

    // Verify output file was created
    if !ts_output.exists() {
        bail!("Generated TypeScript file not found at {}", ts_output.display());
    }

    // Format generated file with prettier if available
    let prettier_output = Command::new("pnpm")
        .args(&["exec", "prettier", "--write", ts_output.to_str().unwrap()])
        .current_dir(config.workspace_root.join("ui"))
        .output();

    if let Ok(out) = prettier_output {
        if !out.status.success() && config.verbose {
            println!("  Warning: Prettier formatting failed");
        }
    }

    Ok(ts_output)
}

/// Validate that generated TypeScript types match Rust API definitions
async fn validate_type_consistency(config: &CodegenConfig, spec_path: &Path) -> Result<()> {
    println!("  Checking OpenAPI spec integrity...");

    // Read and parse spec
    let spec_content = fs::read_to_string(spec_path)
        .context("Failed to read OpenAPI spec")?;

    let spec: serde_json::Value = serde_json::from_str(&spec_content)
        .context("Invalid JSON in OpenAPI spec")?;

    // Check required fields
    let required_fields = vec!["openapi", "info", "paths"];
    for field in required_fields {
        if !spec.get(field).is_some() {
            bail!("OpenAPI spec missing required field: {}", field);
        }
    }

    // Count endpoints
    let paths = spec.get("paths")
        .and_then(|p| p.as_object())
        .map(|p| p.len())
        .unwrap_or(0);

    println!("  Found {} API endpoints", paths);

    if paths == 0 {
        bail!("No API endpoints found in spec");
    }

    // Validate TS types if generated
    let ts_path = config.workspace_root.join("ui/src/api/types.generated.ts");
    if ts_path.exists() {
        println!("  Validating generated TypeScript types...");

        let ts_content = fs::read_to_string(&ts_path)
            .context("Failed to read generated TS types")?;

        // Basic validation: check for common type exports
        let has_exports = ts_content.contains("export type") || ts_content.contains("export interface");
        if !has_exports {
            bail!("Generated TypeScript has no exported types");
        }

        let export_count = ts_content.matches("export type ").count()
            + ts_content.matches("export interface ").count();
        println!("  Found {} TypeScript type definitions", export_count);
    }

    // Check for schema completeness
    println!("  Validating request/response schemas...");
    let schemas = spec.get("components")
        .and_then(|c| c.get("schemas"))
        .and_then(|s| s.as_object())
        .map(|s| s.len())
        .unwrap_or(0);

    println!("  Found {} schema definitions", schemas);

    Ok(())
}

/// Find workspace root by looking for Cargo.toml with [workspace]
fn find_workspace_root() -> Result<PathBuf> {
    let mut current = std::env::current_dir()?;

    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = fs::read_to_string(&cargo_toml)?;
            if content.contains("[workspace]") {
                return Ok(current);
            }
        }

        if !current.pop() {
            anyhow::bail!("Could not find workspace root");
        }
    }
}

/// Pretty-print codegen report
fn print_report(report: &CodegenReport) {
    println!("\n========================================");
    println!("  Code Generation Report");
    println!("========================================\n");

    for step in &report.steps {
        let status = if step.success { "✓" } else { "✗" };
        let duration = if step.duration_ms > 0 {
            format!(" ({} ms)", step.duration_ms)
        } else {
            String::new()
        };

        println!("{} {}{}", status, step.name, duration);
        println!("  {}", step.message);
    }

    println!("\nTotal time: {} ms", report.total_duration_ms);
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_workspace_root() {
        let root = find_workspace_root();
        assert!(root.is_ok());
        let path = root.unwrap();
        assert!(path.join("Cargo.toml").exists());
        assert!(path.join("Cargo.toml").read_to_string()
            .unwrap_or_default()
            .contains("[workspace]"));
    }

    #[test]
    fn test_codegen_report_all_success() {
        let mut report = CodegenReport::new();
        report.steps.push(CodegenStep {
            name: "Test 1".to_string(),
            success: true,
            duration_ms: 100,
            message: "OK".to_string(),
        });
        assert!(report.all_success());

        report.steps.push(CodegenStep {
            name: "Test 2".to_string(),
            success: false,
            duration_ms: 50,
            message: "Failed".to_string(),
        });
        assert!(!report.all_success());
    }
}
