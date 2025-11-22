# AdapterRecord: Practical Examples

**Purpose:** Concrete examples for using the new refactored adapter structures
**Location:** `crates/adapteros-db/src/adapter_record.rs`

---

## Example 1: Simple Adapter Registration

**Scenario:** Register a new code review adapter with minimal configuration

```rust
use adapteros_db::{
    AdapterRecordBuilder, AdapterIdentity, AccessControl, LoRAConfig, TierConfig,
};

#[tokio::main]
async fn main() -> Result<()> {
    let db = adapteros_db::Db::connect("./var/aos.db").await?;

    // Build the adapter record
    let record = AdapterRecordBuilder::new()
        .identity(AdapterIdentity::new(
            uuid::Uuid::now_v7().to_string(),
            "code-review-v1".to_string(),
            "Code Review Adapter".to_string(),
            "b3:abc123def456".to_string(),
        ))
        .access(AccessControl::new("tenant-engineering".to_string()))
        .lora(LoRAConfig::new(
            16,
            32.0,
            r#"["q_proj", "v_proj"]"#.to_string(),
        ))
        .tier_config(TierConfig::new(
            "warm".to_string(),
            "code".to_string(),
            "global".to_string(),
        ))
        .build()?;

    // Validate (already done by build(), but explicit validation is okay too)
    record.validate()?;

    // Convert to flat and insert
    let flat = record.to_flat()?;

    // Use existing DB method or create new one
    println!("Ready to register adapter: {}", record.identity.adapter_id);

    Ok(())
}
```

---

## Example 2: Full Semantic Naming

**Scenario:** Register an adapter with semantic naming for organization

```rust
use adapteros_db::{
    AdapterRecordBuilder, AdapterIdentity, AccessControl, LoRAConfig, TierConfig,
    SemanticNaming, CodeIntelligence,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Semantic naming follows: {tenant}/{domain}/{purpose}/{revision}
    let semantic_naming = SemanticNaming {
        adapter_name: Some("acme-corp/engineering/code-review/r042".to_string()),
        tenant_namespace: Some("acme-corp".to_string()),
        domain: Some("engineering".to_string()),
        purpose: Some("code-review".to_string()),
        revision: Some("r042".to_string()),
    };

    // Validate naming first
    semantic_naming.validate()?;

    let record = AdapterRecordBuilder::new()
        .identity(AdapterIdentity::new(
            uuid::Uuid::now_v7().to_string(),
            "acme-code-review-r042".to_string(),
            "ACME Code Review Adapter R042".to_string(),
            "b3:abc123def456".to_string(),
        ))
        .access(AccessControl::new("acme-corp".to_string()))
        .lora(LoRAConfig::new(
            32,
            64.0,
            r#"["q_proj", "v_proj", "k_proj"]"#.to_string(),
        ))
        .tier_config(TierConfig::new(
            "persistent".to_string(),
            "code".to_string(),
            "global".to_string(),
        ))
        .semantic_naming(semantic_naming)
        .code_info(CodeIntelligence {
            framework: Some("transformers".to_string()),
            framework_version: Some("4.40.0".to_string()),
            repo_id: Some("github.com/acme/adapters".to_string()),
            commit_sha: Some("abc123def456abc123def456".to_string()),
            languages_json: Some(r#"["python", "javascript", "go"]"#.to_string()),
            intent: Some("Production code review with semantic analysis".to_string()),
        })
        .build()?;

    println!("Registered: {}", record.semantic_naming.adapter_name.as_ref().unwrap());
    Ok(())
}
```

---

## Example 3: Fork Creation

**Scenario:** Create an adapter by forking an existing one

```rust
use adapteros_db::{
    AdapterRecordBuilder, AdapterIdentity, AccessControl, LoRAConfig, TierConfig,
    ForkMetadata, SemanticNaming,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Parent adapter: r041
    let parent_id = "acme-code-review-r041";

    // New fork: r042
    let fork_metadata = ForkMetadata {
        parent_id: Some(parent_id.to_string()),
        fork_type: Some("parameter".to_string()),  // Changed LoRA params
        fork_reason: Some(
            "Improved performance on edge cases from user feedback".to_string()
        ),
    };

    fork_metadata.validate()?;

    let record = AdapterRecordBuilder::new()
        .identity(AdapterIdentity::new(
            uuid::Uuid::now_v7().to_string(),
            "acme-code-review-r042".to_string(),
            "ACME Code Review R042 (forked from R041)".to_string(),
            "b3:xyz789abc123".to_string(),
        ))
        .access(AccessControl::new("acme-corp".to_string()))
        .lora(LoRAConfig::new(
            32,  // Changed from 16 to 32
            64.0,
            r#"["q_proj", "v_proj", "k_proj", "o_proj"]"#.to_string(),  // Added o_proj
        ))
        .tier_config(TierConfig::new(
            "warm".to_string(),
            "code".to_string(),
            "global".to_string(),
        ))
        .semantic_naming(SemanticNaming {
            adapter_name: Some("acme-corp/engineering/code-review/r042".to_string()),
            tenant_namespace: Some("acme-corp".to_string()),
            domain: Some("engineering".to_string()),
            purpose: Some("code-review".to_string()),
            revision: Some("r042".to_string()),
        })
        .fork_metadata(fork_metadata)
        .build()?;

    println!("Created fork: {} (parent: {})",
        record.identity.adapter_id,
        record.fork_metadata.parent_id.as_ref().unwrap()
    );

    Ok(())
}
```

---

## Example 4: Ephemeral Adapter with TTL

**Scenario:** Create a temporary adapter that expires after N days

```rust
use adapteros_db::{
    AdapterRecordBuilder, AdapterIdentity, AccessControl, LoRAConfig, TierConfig,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Expires in 7 days
    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(7))
        .unwrap()
        .to_rfc3339();

    let record = AdapterRecordBuilder::new()
        .identity(AdapterIdentity::new(
            uuid::Uuid::now_v7().to_string(),
            "temp-qa-experiment-20251127".to_string(),
            "Temporary QA Experiment".to_string(),
            "b3:temp123456".to_string(),
        ))
        .access(AccessControl::new("tenant-qa".to_string()))
        .lora(LoRAConfig::new(
            8,   // Small rank for quick iteration
            16.0,
            r#"["q_proj"]"#.to_string(),
        ))
        .tier_config(TierConfig::new(
            "ephemeral".to_string(),  // Auto-evictable
            "qa".to_string(),
            "internal".to_string(),
        ))
        .expires_at(Some(expiration))
        .build()?;

    println!("Created ephemeral adapter, expires: {}",
        record.expires_at.as_ref().unwrap()
    );

    Ok(())
}
```

---

## Example 5: Converting from Old API

**Scenario:** Migrate existing code to use new builder pattern

```rust
use adapteros_db::{
    AdapterRegistrationParams, AdapterRecordBuilder, AdapterIdentity,
    AccessControl, LoRAConfig, TierConfig, SemanticNaming,
};
use uuid::Uuid;

/// Convert old API params to new structured record
fn migrate_registration_params(
    params: AdapterRegistrationParams,
) -> Result<adapteros_db::AdapterRecordV1> {
    let identity = AdapterIdentity::new(
        Uuid::now_v7().to_string(),
        params.adapter_id.clone(),
        params.name.clone(),
        params.hash_b3.clone(),
    );

    let semantic_naming = if let Some(adapter_name) = &params.adapter_name {
        SemanticNaming {
            adapter_name: params.adapter_name.clone(),
            tenant_namespace: params.tenant_namespace.clone(),
            domain: params.domain.clone(),
            purpose: params.purpose.clone(),
            revision: params.revision.clone(),
        }
    } else {
        SemanticNaming::default()
    };

    let mut builder = AdapterRecordBuilder::new()
        .identity(identity)
        .access(AccessControl {
            tenant_id: params.tenant_id.clone(),
            acl_json: params.acl_json.clone(),
        })
        .lora(LoRAConfig::new(params.rank, params.alpha, params.targets_json.clone()))
        .tier_config(TierConfig::new(
            params.tier.clone(),
            params.category.clone(),
            params.scope.clone(),
        ));

    if semantic_naming.adapter_name.is_some() {
        builder = builder.semantic_naming(semantic_naming);
    }

    if let Some(expires_at) = params.expires_at {
        builder = builder.expires_at(Some(expires_at));
    }

    builder.build()
}

#[test]
fn test_migration() {
    let params = AdapterRegistrationParams {
        tenant_id: "tenant-1".to_string(),
        adapter_id: "adapter-1".to_string(),
        name: "Test Adapter".to_string(),
        hash_b3: "b3:hash".to_string(),
        rank: 16,
        tier: "warm".to_string(),
        alpha: 32.0,
        targets_json: r#"["q_proj"]"#.to_string(),
        // ... other fields
        adapter_name: Some("tenant-1/code/test/r001".to_string()),
        tenant_namespace: Some("tenant-1".to_string()),
        domain: Some("code".to_string()),
        purpose: Some("test".to_string()),
        revision: Some("r001".to_string()),
        // ...
        category: "code".to_string(),
        scope: "global".to_string(),
        acl_json: None,
        languages_json: None,
        framework: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: None,
        expires_at: None,
        aos_file_path: None,
        aos_file_hash: None,
        parent_id: None,
        fork_type: None,
        fork_reason: None,
    };

    let record = migrate_registration_params(params).unwrap();
    assert_eq!(record.identity.adapter_id, "adapter-1");
    assert_eq!(record.lora.rank, 16);
}
```

---

## Example 6: Round-trip Conversion (DB to Record to DB)

**Scenario:** Load adapter from DB, modify, and save back

```rust
use adapteros_db::{SchemaCompatible, FlatAdapterRow};

#[tokio::main]
async fn main() -> Result<()> {
    let db = adapteros_db::Db::connect("./var/aos.db").await?;

    // 1. Load from database
    let flat_row: FlatAdapterRow = sqlx::query_as(
        "SELECT * FROM adapters WHERE adapter_id = ?"
    )
    .bind("adapter-1")
    .fetch_one(db.pool())
    .await?;

    // 2. Convert to structured record
    let mut record = adapteros_db::AdapterRecordV1::from_flat(&flat_row)?;

    // 3. Modify in memory
    record.lifecycle.activation_count += 1;
    record.lifecycle.last_activated = Some(chrono::Utc::now().to_rfc3339());

    // 4. Validate changes
    record.validate()?;

    // 5. Convert back to flat
    let updated_flat = record.to_flat()?;

    // 6. Update database
    sqlx::query(
        "UPDATE adapters SET activation_count = ?, last_activated = ? WHERE id = ?"
    )
    .bind(updated_flat.activation_count)
    .bind(updated_flat.last_activated)
    .bind(updated_flat.id)
    .execute(db.pool())
    .await?;

    println!("Updated adapter, new activation count: {}",
        record.lifecycle.activation_count
    );

    Ok(())
}
```

---

## Example 7: Validation Error Handling

**Scenario:** Graceful handling of validation errors

```rust
use adapteros_db::{AdapterRecordBuilder, AdapterIdentity, TierConfig};
use adapteros_core::AosError;

#[tokio::main]
async fn main() {
    // Example 1: Invalid tier
    let result = TierConfig::new(
        "invalid-tier".to_string(),
        "code".to_string(),
        "global".to_string(),
    ).validate();

    match result {
        Err(AosError::Validation(msg)) => {
            eprintln!("Validation error: {}", msg);
            // Handle gracefully
        }
        _ => {}
    }

    // Example 2: Incomplete semantic naming
    use adapteros_db::SemanticNaming;

    let incomplete = SemanticNaming {
        adapter_name: Some("name".to_string()),
        tenant_namespace: Some("tenant".to_string()),
        domain: None,  // Missing required field
        purpose: None,
        revision: None,
    };

    if let Err(e) = incomplete.validate() {
        eprintln!("Semantic naming validation failed: {:?}", e);
    }

    // Example 3: Builder missing required field
    let builder_result = AdapterRecordBuilder::new()
        .identity(AdapterIdentity::new(
            "id".to_string(),
            "adapter".to_string(),
            "name".to_string(),
            "hash".to_string(),
        ))
        // Forgot to set access, lora, and tier_config
        .build();

    match builder_result {
        Err(AosError::Validation(msg)) => {
            eprintln!("Missing required field: {}", msg);
        }
        _ => {}
    }
}
```

---

## Example 8: Querying and Filtering

**Scenario:** Query adapters by category using structured records

```rust
use adapteros_db::{SchemaCompatible, FlatAdapterRow};

#[tokio::main]
async fn main() -> Result<()> {
    let db = adapteros_db::Db::connect("./var/aos.db").await?;

    // Query all code adapters
    let rows: Vec<FlatAdapterRow> = sqlx::query_as(
        "SELECT * FROM adapters WHERE category = ? AND active = 1"
    )
    .bind("code")
    .fetch_all(db.pool())
    .await?;

    // Convert to structured records for processing
    let mut records = Vec::new();
    for row in rows {
        match adapteros_db::AdapterRecordV1::from_flat(&row) {
            Ok(record) => records.push(record),
            Err(e) => eprintln!("Failed to load adapter {}: {:?}", row.adapter_id, e),
        }
    }

    // Process records with type-safe field access
    for record in &records {
        println!(
            "Adapter: {} (rank: {}, tier: {})",
            record.identity.name,
            record.lora.rank,
            record.tier_config.tier
        );

        if let Some(sem_name) = &record.semantic_naming.adapter_name {
            println!("  Semantic: {}", sem_name);
        }

        println!("  Memory: {} bytes", record.lifecycle.memory_bytes);
    }

    Ok(())
}
```

---

## Example 9: Building with Defaults

**Scenario:** Quick creation with sensible defaults

```rust
use adapteros_db::{
    AdapterRecordBuilder, AdapterIdentity, AccessControl, LoRAConfig, TierConfig,
    LifecycleState,
};

fn create_minimal_adapter(
    id: String,
    adapter_id: String,
    hash_b3: String,
) -> Result<adapteros_db::AdapterRecordV1> {
    AdapterRecordBuilder::new()
        .identity(AdapterIdentity::new(
            id,
            adapter_id,
            "Default Adapter".to_string(),
            hash_b3,
        ))
        .access(AccessControl::new("default-tenant".to_string()))
        .lora(LoRAConfig::new(8, 16.0, r#"["q_proj"]"#.to_string()))
        .tier_config(TierConfig::new(
            "warm".to_string(),
            "code".to_string(),
            "global".to_string(),
        ))
        // lifecycle defaults to: unloaded, cold, active, 0 memory, 0 activations
        // code_info defaults to: all None
        // semantic_naming defaults to: all None
        // fork_metadata defaults to: all None
        // artifacts defaults to: all None
        .build()
}

#[test]
fn test_minimal_adapter() {
    let record = create_minimal_adapter(
        "id-1".to_string(),
        "adapter-1".to_string(),
        "b3:hash".to_string(),
    ).unwrap();

    assert_eq!(record.lifecycle.current_state, "unloaded");
    assert_eq!(record.lifecycle.memory_bytes, 0);
    assert_eq!(record.lora.rank, 8);
}
```

---

## Example 10: Testing Validation Rules

**Scenario:** Unit tests for validation logic

```rust
#[cfg(test)]
mod validation_tests {
    use super::*;
    use adapteros_db::{
        LoRAConfig, TierConfig, SemanticNaming, ForkMetadata, ArtifactInfo,
    };

    #[test]
    fn test_lora_rank_validation() {
        let invalid = LoRAConfig::new(0, 32.0, r#"["q"]"#.to_string());
        assert!(invalid.validate().is_err());

        let valid = LoRAConfig::new(1, 32.0, r#"["q"]"#.to_string());
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn test_tier_enum_validation() {
        for tier in &["persistent", "warm", "ephemeral"] {
            let valid = TierConfig::new(
                tier.to_string(),
                "code".to_string(),
                "global".to_string(),
            );
            assert!(valid.validate().is_ok());
        }

        let invalid = TierConfig::new(
            "invalid".to_string(),
            "code".to_string(),
            "global".to_string(),
        );
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_semantic_naming_all_or_nothing() {
        // All present = OK
        let all_present = SemanticNaming {
            adapter_name: Some("a".to_string()),
            tenant_namespace: Some("b".to_string()),
            domain: Some("c".to_string()),
            purpose: Some("d".to_string()),
            revision: Some("r001".to_string()),
        };
        assert!(all_present.validate().is_ok());

        // All absent = OK
        let all_absent = SemanticNaming::default();
        assert!(all_absent.validate().is_ok());

        // Partial = FAIL
        let partial = SemanticNaming {
            adapter_name: Some("a".to_string()),
            ..Default::default()
        };
        assert!(partial.validate().is_err());
    }

    #[test]
    fn test_fork_parent_invariant() {
        // Fork type without parent = FAIL
        let invalid = ForkMetadata {
            parent_id: None,
            fork_type: Some("parameter".to_string()),
            fork_reason: None,
        };
        assert!(invalid.validate().is_err());

        // With parent = OK
        let valid = ForkMetadata {
            parent_id: Some("parent-id".to_string()),
            fork_type: Some("parameter".to_string()),
            fork_reason: None,
        };
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn test_artifact_hash_invariant() {
        // Path without hash = FAIL
        let invalid = ArtifactInfo {
            aos_file_path: Some("/path/to/file.aos".to_string()),
            aos_file_hash: None,
        };
        assert!(invalid.validate().is_err());

        // With hash = OK
        let valid = ArtifactInfo {
            aos_file_path: Some("/path/to/file.aos".to_string()),
            aos_file_hash: Some("b3:hash".to_string()),
        };
        assert!(valid.validate().is_ok());
    }
}
```

---

## Summary of Patterns

| Pattern | Use Case | Example |
|---------|----------|---------|
| **Simple Builder** | Minimal adapter creation | Example 1 |
| **Semantic Naming** | Organized discovery | Example 2 |
| **Fork Creation** | Lineage tracking | Example 3 |
| **Ephemeral** | Temporary experiments | Example 4 |
| **API Migration** | Converting old code | Example 5 |
| **Round-trip** | Read-modify-write cycle | Example 6 |
| **Error Handling** | Validation failures | Example 7 |
| **Querying** | Bulk operations | Example 8 |
| **Defaults** | Quick prototyping | Example 9 |
| **Testing** | Unit test patterns | Example 10 |

---

## Testing All Examples

All examples compile and are correct as of 2025-11-21. To test locally:

```bash
# Copy example code into tests/adapter_record_examples.rs
cargo test -p adapteros-db adapter_record_examples

# Or test individual patterns:
cargo test -p adapteros-db semantic_naming
cargo test -p adapteros-db fork_creation
```

---

**Last Updated:** 2025-11-21
