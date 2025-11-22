//! Audit determinism command
//!
//! Validates backend determinism attestation and provides detailed report

use adapteros_lora_kernel_api::FusedKernels;
use adapteros_lora_worker::{create_backend, create_backend_with_model, BackendChoice};
use anyhow::Result;
use clap::Args;
use std::path::Path;

/// Simple output helper for audit determinism
pub struct Output;

impl Output {
    pub fn info(&self, msg: &str) {
        println!("{}", msg);
    }

    pub fn verbose(&self, msg: &str) {
        if std::env::var("RUST_LOG")
            .unwrap_or_default()
            .contains("debug")
        {
            println!("{}", msg);
        }
    }

    pub fn success(&self, msg: &str) {
        println!("✅ {}", msg);
    }

    pub fn error(&self, msg: &str) {
        eprintln!("❌ {}", msg);
    }
}

#[derive(Args, Debug)]
pub struct AuditDeterminismArgs {
    /// Backend to audit (metal, mlx, coreml)
    #[arg(long, default_value = "metal")]
    backend: String,

    /// Model path (for MLX backend)
    #[arg(long)]
    model_path: Option<String>,

    /// Output format (text, json)
    #[arg(long, default_value = "text")]
    format: String,
}

pub fn run(args: &AuditDeterminismArgs, output: &Output) -> Result<i32> {
    output.info("🔍 Auditing Backend Determinism\n");

    // Parse backend type and create backend
    let backend: Box<dyn FusedKernels> = match args.backend.to_lowercase().as_str() {
        "metal" => {
            output.verbose("Creating Metal backend...");
            create_backend(BackendChoice::Metal)?
        }
        "mlx" => {
            let model_path = args
                .model_path
                .clone()
                .or_else(|| std::env::var("AOS_MODEL_PATH").ok())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "MLX backend requires --model-path argument or AOS_MODEL_PATH env var"
                    )
                })?;
            output.verbose(&format!(
                "Creating MLX backend with model: {}...",
                model_path
            ));
            create_backend_with_model(BackendChoice::Mlx, Path::new(&model_path))?
        }
        "coreml" => {
            output.verbose("Creating CoreML backend...");
            create_backend(BackendChoice::CoreML)?
        }
        other => {
            return Err(anyhow::anyhow!("Unknown backend type: {}", other));
        }
    };

    // Get attestation report
    output.verbose("Retrieving attestation report...");
    let report = backend.attest_determinism()?;

    // Display report based on format
    match args.format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&report)?;
            println!("{}", json);
        }
        "text" | _ => {
            output.info("=== Determinism Attestation Report ===\n");

            output.info(&format!("Backend Type:       {:?}", report.backend_type));
            output.info(&format!("Deterministic:      {}", report.deterministic));
            output.info(&format!("RNG Seeding:        {:?}", report.rng_seed_method));
            output.info(&format!(
                "FP Mode:            {:?}",
                report.floating_point_mode
            ));

            if let Some(ref hash) = report.metallib_hash {
                output.info(&format!("Metallib Hash:      {}", hash.to_short_hex()));
            } else {
                output.info("Metallib Hash:      N/A");
            }

            if !report.compiler_flags.is_empty() {
                output.info("\nCompiler Flags:");
                for flag in &report.compiler_flags {
                    output.info(&format!("  - {}", flag));
                }
            }

            if let Some(ref manifest) = report.manifest {
                output.info("\nKernel Manifest:");
                output.info(&format!("  Build Time:       {}", manifest.build_timestamp));
                output.info(&format!("  Rust Version:     {}", manifest.rust_version));
                output.info(&format!("  SDK Version:      {}", manifest.sdk_version));
                output.info(&format!("  xcrun Version:    {}", manifest.xcrun_version));
            }

            output.info("\n=== Validation ===\n");
        }
    }

    // Validate the report
    match report.validate() {
        Ok(()) => {
            if args.format == "text" {
                output.success("✓ Backend passes determinism validation");
                output.info(
                    "\nThis backend is suitable for production use with deterministic execution.",
                );
            }
            Ok(0) // Exit code 0 for success
        }
        Err(e) => {
            if args.format == "text" {
                output.error(&format!("✗ Backend fails determinism validation: {}", e));
                output.info("\nThis backend should NOT be used for production serving.");
                output.info("Consider using Metal backend or fixing validation issues.");
            }
            Ok(1) // Exit code 1 for validation failure
        }
    }
}
