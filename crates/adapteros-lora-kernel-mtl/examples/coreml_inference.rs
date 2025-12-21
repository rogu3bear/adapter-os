//! CoreML Inference Example
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//!
//! This example demonstrates how to use the CoreML FFI for model inference.
//!
//! Usage:
//!   cargo run --example coreml_inference --features coreml-backend -- /path/to/model.mlmodelc
//!
//! Requirements:
//! - macOS 10.13+
//! - A compiled CoreML model (.mlmodelc or .mlpackage)
//! - coreml-backend feature enabled

#[cfg(all(feature = "coreml-backend", target_os = "macos"))]
use adapteros_core::Result;
#[cfg(all(feature = "coreml-backend", target_os = "macos"))]
use adapteros_lora_kernel_mtl::coreml;

#[cfg(all(feature = "coreml-backend", target_os = "macos"))]
fn main() -> Result<()> {
    // Enable verbose logging
    coreml::set_verbose(true);

    // Check CoreML availability
    if !coreml::is_available() {
        eprintln!("CoreML is not available on this system");
        return Ok(());
    }

    println!("CoreML version: {}", coreml::version());

    // Get model path from command line
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <path-to-model.mlmodelc>", args[0]);
        eprintln!("\nExample models can be created with coremltools:");
        eprintln!("  pip install coremltools");
        eprintln!("  python -c 'import coremltools as ct; ...'");
        return Ok(());
    }

    let model_path = &args[1];
    println!("\nLoading model from: {}", model_path);

    // Load model with GPU and ANE support
    let model = coreml::Model::load(model_path, true, true)?;

    // Get model metadata
    let metadata = model.metadata()?;
    println!("\nModel Metadata:");
    println!("  Version: {}", metadata.version);
    println!("  Description: {}", metadata.description);
    println!("  Inputs: {}", metadata.input_count);
    println!("  Outputs: {}", metadata.output_count);
    println!("  GPU Support: {}", metadata.supports_gpu);
    println!("  ANE Support: {}", metadata.supports_ane);

    // Create sample input (adjust shape based on your model)
    // Example: Simple 1D input of 10 elements
    let input_data = vec![0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
    let input_shape = vec![1, 10]; // batch_size=1, features=10

    println!("\nCreating input array with shape {:?}", input_shape);
    let input_array = coreml::Array::new_f32(&input_data, &input_shape)?;

    println!("Input array created:");
    println!("  Shape: {:?}", input_array.shape());
    println!("  Size: {}", input_array.size());

    // Run prediction
    println!("\nRunning prediction...");
    let prediction = model.predict(&input_array, None)?;

    // Display results
    println!("\nPrediction Results:");
    println!("  Output count: {}", prediction.output_count());

    let output_names = prediction.output_names();
    for (i, name) in output_names.iter().enumerate() {
        println!("  Output {}: {}", i, name);

        if let Ok(output_array) = prediction.get_output(name) {
            println!("    Shape: {:?}", output_array.shape());
            println!("    Size: {}", output_array.size());

            // Try to get float data
            if let Some(data) = output_array.as_f32_slice() {
                println!("    Data (first 10): {:?}", &data[..data.len().min(10)]);
            }
        }
    }

    println!("\nInference completed successfully!");

    Ok(())
}

#[cfg(not(all(feature = "coreml-backend", target_os = "macos")))]
fn main() {
    eprintln!("This example requires macOS and the coreml-backend feature.");
    eprintln!("Build with: cargo run --example coreml_inference --features coreml-backend");
    std::process::exit(1);
}
