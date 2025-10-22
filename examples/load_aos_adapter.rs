//! Example: Loading .aos adapters in the orchestrator
//!
//! Shows how the orchestrator can now load .aos files and make them accessible

use adapteros_core::Result;
use adapteros_lora_lifecycle::{AdapterLoader, LifecycleManager};
use adapteros_manifest::Policies;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("=== .aos Adapter Loading Example ===\n");

    // Path to your adapters directory
    let adapters_path = PathBuf::from("./adapters");

    // Example 1: Direct loading with AdapterLoader
    println!("1. Direct loading with AdapterLoader:");
    let mut loader = AdapterLoader::new(adapters_path.clone());

    // This will now check for:
    // 1. ./adapters/my_adapter.aos (PREFERRED)
    // 2. ./adapters/my_adapter.safetensors (fallback)
    // 3. ./adapters/my_adapter/weights.safetensors (fallback)

    match loader.load_adapter(0, "my_adapter") {
        Ok(handle) => {
            println!("   ✓ Loaded adapter: {}", handle.path.display());
            println!("   ✓ Memory: {} bytes", handle.memory_bytes);
            println!(
                "   ✓ Format: {}",
                if handle.path.extension().and_then(|s| s.to_str()) == Some("aos") {
                    ".aos"
                } else {
                    ".safetensors"
                }
            );

            // Signature verification happens automatically for .aos files!
            println!("   ✓ Signature: Verified (if .aos file with signature)\n");
        }
        Err(e) => {
            println!("   ✗ Failed to load: {}\n", e);
            println!("   Tip: Place your adapter at ./adapters/my_adapter.aos\n");
        }
    }

    // Example 2: Using LifecycleManager (full orchestrator integration)
    println!("2. Using LifecycleManager:");
    let lifecycle_manager = LifecycleManager::new(
        vec!["base_adapter".to_string(), "code_adapter".to_string()],
        &Policies::default(),
        adapters_path,
        None,
        2,
    );

    println!("   ✓ Lifecycle manager initialized");
    println!("   ✓ Will check for:");
    println!("      - ./adapters/base_adapter.aos");
    println!("      - ./adapters/code_adapter.aos");
    println!("   ✓ Falls back to .safetensors if .aos not found\n");

    // Example 3: Preloading adapters
    println!("3. Preloading adapters:");
    match lifecycle_manager.preload_adapter(0) {
        Ok(handle) => {
            println!("   ✓ Preloaded adapter 0: {} bytes", handle.memory_bytes);
        }
        Err(e) => {
            println!("   ⚠ Failed to preload: {}", e);
            println!("   (This is expected if adapters don't exist yet)");
        }
    }

    println!("\n=== Summary ===");
    println!("✓ .aos files are now first-class adapters!");
    println!("✓ Place adapters at ./adapters/<name>.aos");
    println!("✓ Automatic signature verification");
    println!("✓ Falls back to .safetensors if needed");
    println!("\nTo create an .aos adapter:");
    println!("  aos create --input weights.safetensors --output ./adapters/my_adapter.aos --sign");

    Ok(())
}
