//! Example: Parse AOS v2 Archive
//!
//! Demonstrates how to use the AosV2Parser to read and inspect AOS v2 archives.
//!
//! Run with:
//! ```bash
//! cargo run --example parse_aos_v2 --features mmap -- path/to/adapter.aos
//! ```

use adapteros_aos::aos_v2_parser::{AosV2Manifest, AosV2Parser};
use adapteros_core::Result;
use std::env;

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Get archive path from command line
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <path-to-aos-file>", args[0]);
        std::process::exit(1);
    }

    let path = &args[1];
    println!("Parsing AOS v2 archive: {}", path);

    // Open and parse the archive
    let mut parser = AosV2Parser::open(path)?;
    println!("✓ Archive opened successfully");

    // Parse and validate manifest
    let manifest: AosV2Manifest = parser.manifest()?;
    manifest.validate()?;
    println!("\n=== Manifest ===");
    println!("Version: {}", manifest.version);
    println!("Adapter ID: {}", manifest.adapter_id);
    println!("Rank: {}", manifest.rank);

    if let Some(hash) = &manifest.weights_hash {
        println!("Weights Hash: {}", hash.to_hex());
        println!("\n=== Verifying Hash ===");
        parser.verify_hash(hash)?;
        println!("✓ Hash verification passed");
    }

    // Get tensor metadata
    println!("\n=== Tensor Metadata ===");
    let tensor_info = parser.tensor_metadata()?;
    println!("Total tensors: {}", tensor_info.len());

    for (name, info) in tensor_info.iter() {
        println!("\nTensor: {}", name);
        println!("  Shape: {:?}", info.shape);
        println!("  DType: {}", info.dtype);
        println!("  Size: {} bytes", info.size);
        println!("  Elements: {}", info.num_elements());
        println!("  Element size: {} bytes", info.element_size());
    }

    // Extract a specific tensor
    if let Some(first_tensor_name) = parser.tensor_names()?.first() {
        println!("\n=== Extracting Tensor: {} ===", first_tensor_name);
        if let Some(tensor_view) = parser.tensor(first_tensor_name)? {
            println!("Shape: {:?}", tensor_view.shape());
            println!("Data size: {} bytes", tensor_view.as_bytes().len());
            println!(
                "First 16 bytes: {:02x?}",
                &tensor_view.as_bytes()[..16.min(tensor_view.as_bytes().len())]
            );
        }
    }

    // File statistics
    println!("\n=== Archive Statistics ===");
    println!("Total file size: {} bytes", parser.file_size());
    let (weights_start, weights_end) = parser.weights_section();
    println!(
        "Weights section: {}-{} ({} bytes)",
        weights_start,
        weights_end,
        weights_end - weights_start
    );

    println!("\n✓ Parsing complete");

    Ok(())
}
