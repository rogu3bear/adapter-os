//! # Experimental Features Test Binary
//!
//! This binary tests all experimental features in the adapteros-experimental crate.
//!
//! ## ⚠️ WARNING ⚠️
//!
//! This binary is for testing experimental features only and should not be used in production.
//!
//! ## Usage
//!
//! ```bash
//! cargo run --bin experimental-test --features experimental-all
//! ```

use adapteros_experimental::*;
use std::path::Path;
use tokio;

#[cfg(feature = "aos-cli")]
use adapteros_experimental::aos_cli::*;

#[cfg(feature = "error-recovery")]
use adapteros_experimental::error_recovery::*;

#[cfg(feature = "migration-conflicts")]
use adapteros_experimental::migration_conflicts::*;

#[cfg(feature = "domain-adapters")]
use adapteros_experimental::domain_adapters::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚧 EXPERIMENTAL: Testing AdapterOS Experimental Features");
    println!("🚧 EXPERIMENTAL: This is NOT FOR PRODUCTION USE");
    println!();

    // Test experimental registry
    println!("Testing Experimental Registry...");
    let registry = ExperimentalRegistry::new();
    let features = registry.list_features();
    println!("Found {} experimental features:", features.len());
    for feature in features {
        println!(
            "  - {}: {:?} ({:?})",
            feature.name, feature.status, feature.stability
        );
    }
    println!();

    // Test AOS CLI experimental features
    #[cfg(feature = "aos-cli")]
    {
        println!("Testing AOS CLI Experimental Features...");
        let cli = ExperimentalAosCli::new();

        // Test create command
        let create_args = CreateArgs {
            adapter_path: std::path::PathBuf::from("test.adapter"),
            output_path: std::path::PathBuf::from("test.aos"),
            compression: CompressionLevel::Default,
            package_options: None,
        };

        let cmd = AosCmd {
            subcommand: AosSubcommand::Create(create_args),
        };

        match cli.execute(cmd).await {
            Ok(_) => println!("  ✅ AOS CLI create command executed successfully"),
            Err(e) => println!("  ❌ AOS CLI create command failed: {}", e),
        }
        println!();
    }

    // Test error recovery experimental features
    #[cfg(feature = "error-recovery")]
    {
        println!("Testing Error Recovery Experimental Features...");
        let mut recovery = ErrorRecovery::new();

        // Test retry operation
        let path = Path::new("/tmp/test");
        match recovery.perform_retry_operation(path).await {
            Ok(_) => println!("  ✅ Error recovery retry operation completed successfully"),
            Err(e) => println!("  ❌ Error recovery retry operation failed: {}", e),
        }

        // Test retry operation creation
        let operation = recovery.create_retry_operation(
            "test-operation".to_string(),
            recovery.default_config.clone(),
        );
        println!("  ✅ Created retry operation: {}", operation.name);

        // Test statistics
        let stats = recovery.get_retry_statistics();
        println!(
            "  ✅ Retry statistics: {} total operations",
            stats.total_operations
        );
        println!();
    }

    // Test migration conflicts experimental features
    #[cfg(feature = "migration-conflicts")]
    {
        println!("Testing Migration Conflicts Experimental Features...");
        let mut resolver = MigrationConflictResolver::new();

        // Test conflict detection
        let path = Path::new("/tmp/test");
        match resolver.detect_conflicts(path).await {
            Ok(_) => println!("  ✅ Migration conflict detection completed successfully"),
            Err(e) => println!("  ❌ Migration conflict detection failed: {}", e),
        }

        // Test conflict resolution
        match resolver.resolve_conflicts().await {
            Ok(_) => println!("  ✅ Migration conflict resolution completed successfully"),
            Err(e) => println!("  ❌ Migration conflict resolution failed: {}", e),
        }

        // Test schema validation
        match resolver.validate_schema(path).await {
            Ok(_) => println!("  ✅ Schema validation completed successfully"),
            Err(e) => println!("  ❌ Schema validation failed: {}", e),
        }

        // Test migration plan generation
        match resolver.generate_migration_plan().await {
            Ok(_) => println!("  ✅ Migration plan generation completed successfully"),
            Err(e) => println!("  ❌ Migration plan generation failed: {}", e),
        }

        // Test conflict summary
        let summary = resolver.get_conflict_summary();
        println!(
            "  ✅ Conflict summary: {} total conflicts",
            summary.total_conflicts
        );
        println!();
    }

    // Test domain adapters experimental features
    #[cfg(feature = "domain-adapters")]
    {
        println!("Testing Domain Adapters Experimental Features...");
        let config = DomainAdapterConfig {
            adapter_id: "test-adapter".to_string(),
            adapter_name: "Test Adapter".to_string(),
            adapter_version: "1.0.0".to_string(),
            adapter_description: "Test adapter for experimental features".to_string(),
            parameters: std::collections::HashMap::new(),
            feature_flags: std::collections::HashMap::new(),
        };

        let mut executor = DomainAdapterExecutor::new(config.clone());

        // Test pipeline execution
        let request = "test request";
        match executor.execute_pipeline(request).await {
            Ok(response) => println!("  ✅ Pipeline execution completed: {}", response),
            Err(e) => println!("  ❌ Pipeline execution failed: {}", e),
        }

        // Test handler
        let mut handler = DomainAdapterHandler::new(config);
        match handler.handle_request(request).await {
            Ok(response) => println!("  ✅ Request handling completed: {}", response),
            Err(e) => println!("  ❌ Request handling failed: {}", e),
        }

        // Test cache statistics
        let cache_stats = handler.get_cache_statistics();
        println!(
            "  ✅ Cache statistics: {} total requests",
            cache_stats.total_requests
        );
        println!();
    }

    println!("🚧 EXPERIMENTAL: All tests completed");
    println!("🚧 EXPERIMENTAL: Remember - these features are NOT FOR PRODUCTION USE");

    Ok(())
}
