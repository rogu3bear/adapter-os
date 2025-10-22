# AdapterOS Experimental Features

This crate contains experimental features that are **NOT FOR PRODUCTION USE**.

## ⚠️ WARNING ⚠️

All features in this crate are:
- **NOT production ready**
- **Subject to breaking changes**
- **May have incomplete implementations**
- **Should not be used in production systems**

## Experimental Feature Status

| Feature | Status | Stability | Notes |
|---------|--------|-----------|-------|
| `aos-cli` | 🚧 In Development | Unstable | AOS CLI commands with TODO implementations |
| `error-recovery` | 🚧 In Development | Unstable | Placeholder retry logic |
| `migration-conflicts` | 🚧 In Development | Unstable | Schema alignment conflicts |
| `domain-adapters` | 🚧 In Development | Unstable | Domain adapter execution pipeline |

## Usage

### Basic Usage

```toml
[dependencies]
adapteros-experimental = { path = "../adapteros-experimental", features = ["experimental-aos-cli"] }
```

### Feature Flags

- `experimental-aos-cli` - AOS CLI experimental features
- `experimental-error-recovery` - Error recovery experimental features
- `experimental-migration-conflicts` - Migration conflict resolution
- `experimental-domain-adapters` - Domain adapter experimental features
- `experimental-all` - All experimental features

### Example Usage

```rust
use adapteros_experimental::*;

// Test experimental registry
let registry = ExperimentalRegistry::new();
let features = registry.list_features();
println!("Found {} experimental features", features.len());

// Use AOS CLI experimental features
#[cfg(feature = "aos-cli")]
{
    let cli = ExperimentalAosCli::new();
    // ... use experimental CLI features
}

// Use error recovery experimental features
#[cfg(feature = "error-recovery")]
{
    let recovery = ErrorRecovery::new();
    // ... use experimental error recovery features
}
```

## Deterministic Tagging System

Each experimental feature is tagged with:
- **Status**: Development stage (In Development, Experimental, Deprecated)
- **Stability**: Stability level (Unstable, Experimental, Deprecated)
- **Dependencies**: Required features and crates
- **Last Updated**: Date of last modification
- **Known Issues**: List of known problems

## Feature Details

### AOS CLI Experimental Features

**Status**: 🚧 In Development  
**Stability**: Unstable  
**Dependencies**: adapteros-cli, adapteros-single-file-adapter  
**Last Updated**: 2025-01-15  
**Known Issues**: 
- TODO: Register with control plane
- Missing control plane registration
- Incomplete implementations
- Missing error handling
- No validation

**Usage**:
```rust
use adapteros_experimental::aos_cli::*;

let cli = ExperimentalAosCli::new();
let cmd = AosCmd {
    subcommand: AosSubcommand::Create(CreateArgs {
        adapter_path: PathBuf::from("test.adapter"),
        output_path: PathBuf::from("test.aos"),
        compression: CompressionLevel::Default,
        package_options: None,
    }),
};
cli.execute(cmd).await?;
```

### Error Recovery Experimental Features

**Status**: 🚧 In Development  
**Stability**: Unstable  
**Dependencies**: tokio, anyhow  
**Last Updated**: 2025-01-15  
**Known Issues**: 
- Placeholder retry logic
- Missing error classification
- No circuit breaker
- Incomplete backoff strategies

**Usage**:
```rust
use adapteros_experimental::error_recovery::*;

let recovery = ErrorRecovery::new();
let operation = recovery.create_retry_operation(
    "test-operation".to_string(),
    recovery.default_config.clone(),
);
recovery.perform_retry_operation(Path::new("/tmp/test")).await?;
```

### Migration Conflicts Experimental Features

**Status**: 🚧 In Development  
**Stability**: Unstable  
**Dependencies**: adapteros-db, adapteros-policy  
**Last Updated**: 2025-01-15  
**Known Issues**: 
- Schema alignment conflicts
- FOREIGN KEY conflicts
- Hash watcher test failures
- Incomplete migration strategy

**Usage**:
```rust
use adapteros_experimental::migration_conflicts::*;

let mut resolver = MigrationConflictResolver::new();
resolver.detect_conflicts(Path::new("/tmp/test")).await?;
resolver.resolve_conflicts().await?;
resolver.validate_schema(Path::new("/tmp/test")).await?;
resolver.generate_migration_plan().await?;
```

### Domain Adapters Experimental Features

**Status**: 🚧 In Development  
**Stability**: Unstable  
**Dependencies**: adapteros-server-api-types, adapteros-core  
**Last Updated**: 2025-01-15  
**Known Issues**: 
- Merge conflicts
- Missing pipeline stages
- Incomplete error handling
- No validation

**Usage**:
```rust
use adapteros_experimental::domain_adapters::*;

let config = DomainAdapterConfig {
    adapter_id: "test-adapter".to_string(),
    adapter_name: "Test Adapter".to_string(),
    adapter_version: "1.0.0".to_string(),
    adapter_description: "Test adapter".to_string(),
    parameters: HashMap::new(),
    feature_flags: HashMap::new(),
};

let mut executor = DomainAdapterExecutor::new(config);
let response = executor.execute_pipeline("test request").await?;
```

## Testing

### Run All Tests

```bash
cargo test --features experimental-all
```

### Run Specific Feature Tests

```bash
# Test AOS CLI features
cargo test --features experimental-aos-cli

# Test error recovery features
cargo test --features experimental-error-recovery

# Test migration conflicts features
cargo test --features experimental-migration-conflicts

# Test domain adapters features
cargo test --features experimental-domain-adapters
```

### Run Experimental Test Binary

```bash
cargo run --bin experimental-test --features experimental-all
```

## Contributing

When adding experimental features:

1. Create a new module in `src/`
2. Add feature flag to `Cargo.toml`
3. Update this documentation
4. Add deterministic tags
5. Include comprehensive tests
6. Add to experimental registry

### Example Module Structure

```rust
//! # Experimental Feature Name
//!
//! This module contains experimental features that are **NOT FOR PRODUCTION USE**.
//!
//! ## ⚠️ WARNING ⚠️
//!
//! All features in this module are:
//! - **NOT production ready**
//! - **Subject to breaking changes**
//! - **May have incomplete implementations**
//! - **Should not be used in production systems**
//!
//! ## Feature Status
//!
//! | Feature | Status | Stability | Notes |
//! |---------|--------|-----------|-------|
//! | `FeatureName` | 🚧 In Development | Unstable | Description |
//!
//! ## Known Issues
//!
//! - Issue 1
//! - Issue 2
//!
//! ## Dependencies
//!
//! - dependency1
//! - dependency2
//!
//! ## Last Updated
//!
//! 2025-01-15 - Initial experimental implementation

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]

// Implementation here...
```

## Migration Path

Experimental features should eventually be:

1. **Completed** and moved to production crates
2. **Deprecated** and removed
3. **Stabilized** and moved to stable APIs

### Completion Checklist

- [ ] All TODO items resolved
- [ ] Error handling implemented
- [ ] Input validation added
- [ ] Comprehensive tests written
- [ ] Documentation updated
- [ ] Performance optimized
- [ ] Security reviewed
- [ ] Production readiness verified

## Policy Compliance

This crate follows AdapterOS experimental feature policies:

- **Isolation**: Experimental features are isolated from production code
- **Documentation**: All features are thoroughly documented
- **Testing**: Comprehensive test coverage required
- **Tagging**: Deterministic tagging system implemented
- **Migration**: Clear migration path to production

## License

This crate is licensed under the same terms as AdapterOS:
- MIT License
- Apache License 2.0

## Support

For questions about experimental features:

1. Check this documentation
2. Review the experimental registry
3. Examine the test cases
4. Contact the development team

---

**Remember**: These features are **NOT FOR PRODUCTION USE**. Use at your own risk.
