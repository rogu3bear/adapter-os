//! CoreML Model Conversion Example
//!
//! This example demonstrates how to convert a model from safetensors to CoreML
//! with quantization and validation.
//!
//! Usage:
//!   cargo run --example coreml_conversion_example --features coreml-backend

use adapteros_lora_kernel_mtl::{
    ConversionConfig, ModelConverter, ModelValidator, QuantizationType, ValidationConfig,
};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🔧 AdapterOS CoreML Conversion Example");
    println!("======================================\n");

    // Configuration
    let input_path = Path::new("path/to/model.safetensors");
    let output_path = Path::new("path/to/model.mlpackage");

    // Step 1: Configure conversion
    println!("Step 1: Configuring conversion...");
    let conversion_config = ConversionConfig {
        quantization: Some(QuantizationType::Float16), // FP16 for ANE
        target_ane: true,
        batch_size: 1,
        sequence_length: 128,
        min_macos_version: "13.0".to_string(),
        strict_validation: true,
    };

    println!("  Quantization: {:?}", conversion_config.quantization);
    println!("  Target ANE: {}", conversion_config.target_ane);
    println!("  Batch size: {}", conversion_config.batch_size);
    println!();

    // Step 2: Create converter
    println!("Step 2: Creating model converter...");
    let converter = ModelConverter::new(conversion_config)?;
    println!("  Converter initialized\n");

    // Step 3: Convert model
    println!("Step 3: Converting model...");
    println!("  Input:  {}", input_path.display());
    println!("  Output: {}", output_path.display());

    if !input_path.exists() {
        println!("  ⚠️  Input file not found (this is a demo)");
        println!("  📝 Generated conversion script would be created at:");
        println!("      {}", output_path.with_extension("conversion.py").display());
        println!();
    } else {
        let manifest = converter.convert_safetensors_to_coreml(input_path, output_path)?;

        println!("  ✅ Conversion manifest created");
        println!("  Script: {}", manifest.script_path.display());
        println!();

        // Save manifest
        let manifest_path = output_path.with_suffix(".manifest.json");
        manifest.save(&manifest_path)?;
        println!("  Saved manifest: {}", manifest_path.display());
    }

    // Step 4: Validation (after conversion)
    println!("\nStep 4: Validating converted model (simulation)...");
    let validation_config = ValidationConfig {
        accuracy_threshold: 1e-3,
        num_samples: 10,
        check_ane_compatibility: true,
        run_benchmarks: true,
        warmup_iterations: 10,
        benchmark_iterations: 100,
    };

    let validator = ModelValidator::new(validation_config);

    if output_path.exists() {
        println!("  Running validation...");
        let report = validator.validate_model(input_path, output_path)?;

        println!("\n📊 Validation Report:");
        println!("  Status: {:?}", report.status);

        if let Some(accuracy) = &report.accuracy {
            println!("\n  Accuracy:");
            println!("    Mean relative error: {:.6}", accuracy.mean_relative_error);
            println!("    Accuracy percentage: {:.2}%", accuracy.accuracy_percentage);
        }

        if let Some(ane) = &report.ane_compatibility {
            println!("\n  ANE Compatibility:");
            println!("    Fully compatible: {}", ane.fully_compatible);
            println!("    Compatibility: {:.1}%", ane.compatibility_percentage);
        }

        if let Some(perf) = &report.performance {
            println!("\n  Performance:");
            println!("    Throughput: {:.1} tokens/sec", perf.throughput_tokens_per_sec);
            println!("    Latency: {:.2} ms", perf.avg_latency_ms);
            println!("    ANE used: {}", perf.ane_used);
        }

        // Save report
        let report_path = output_path.with_suffix(".validation.json");
        report.save(&report_path)?;
        println!("\n  Saved validation report: {}", report_path.display());

        if report.passed() {
            println!("\n✅ Validation PASSED");
        } else {
            println!("\n❌ Validation FAILED");
            for error in &report.errors {
                println!("  - {}", error);
            }
        }
    } else {
        println!("  ⚠️  Output not found (run Python script first)");
        println!("  To complete conversion, run:");
        println!("    python3 {}", output_path.with_extension("conversion.py").display());
    }

    println!("\n🎉 Example complete!");
    println!("\nNext steps:");
    println!("  1. Run the generated Python conversion script");
    println!("  2. Validate the converted model");
    println!("  3. Load the .mlpackage in your CoreML backend");

    Ok(())
}
