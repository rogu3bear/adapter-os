# Version Guarantees & Metadata Migration Policy

**PRD-02: Adapter & Stack Metadata Normalization + Version Guarantees**

## Overview

This document defines the version guarantees and migration policies for adapter and stack metadata in AdapterOS. These rules ensure backward compatibility while allowing the system to evolve.

## Schema Versioning

### API Schema Version

The current API schema version is defined in `crates/adapteros-db/src/metadata.rs`:

```rust
pub const API_SCHEMA_VERSION: &str = "1.0.0";
```

This version is included in all API responses and telemetry bundles for compatibility tracking.

### Versioning Format

AdapterOS supports two version formats:

1. **Semantic Versioning (SemVer)**: `MAJOR.MINOR.PATCH` (e.g., `1.0.0`, `2.1.3`)
2. **Monotonic Versioning**: Sequential integers (e.g., `1`, `42`, `123`)

## Version Guarantees

### Minor Version Changes (Backward Compatible)

**Definition**: A minor version change adds new fields or capabilities without breaking existing functionality.

**Examples**:
- Adding a new optional field to `AdapterMeta` or `AdapterStackMeta`
- Adding a new lifecycle state (as long as transitions remain valid)
- Adding a new workflow type for stacks
- Extending telemetry event metadata

**Rules**:
1. New fields **MUST** be optional (use `Option<T>` in Rust structs)
2. Database migrations **MUST** provide default values for new columns
3. API responses **MUST** include all fields (with defaults for missing data)
4. Old clients **MAY** ignore new fields
5. Old data **MUST** be readable by new code
6. Schema version minor number **SHOULD** be incremented (e.g., `1.0.0` → `1.1.0`)

**Migration Path**:
1. Add database migration with `DEFAULT` values
2. Update structs with new optional fields
3. Update API handlers to populate new fields
4. Update documentation
5. **No** explicit migration required for existing data

### Major Version Changes (Breaking Changes)

**Definition**: A major version change modifies existing fields or behavior in a non-backward-compatible way.

**Examples**:
- Renaming a field in `AdapterMeta` or `AdapterStackMeta`
- Changing a field's data type (e.g., `String` → `i64`)
- Removing a field
- Changing lifecycle state transition rules
- Modifying version format (e.g., switching from SemVer to monotonic)

**Rules**:
1. Breaking changes **MUST** increment the major version (e.g., `1.2.3` → `2.0.0`)
2. An explicit migration path **MUST** be documented
3. Data migration scripts **MUST** be provided
4. Old clients **MUST** receive a compatibility error (HTTP 400) if they send incompatible data
5. New code **SHOULD** handle old data gracefully during transition period

**Migration Path**:
1. Create database migration that transforms old data to new format
2. Update structs with new fields/types
3. Update API handlers with validation for old clients
4. Document migration steps in CHANGELOG
5. Provide tools for bulk data migration (if needed)
6. Support dual-read mode during transition (optional)

## Lifecycle State Guarantees

### State Transition Rules

Lifecycle states follow a strict progression:

```
draft → active → deprecated → retired
```

**Special Rules**:
- **Retired** is a terminal state (no transitions out)
- **Ephemeral** adapters cannot transition to `deprecated` (must go directly to `retired`)
- State transitions are validated in `metadata::validate_state_transition()`

### Illegal Combinations

The following combinations are **forbidden** and will be rejected by validation:

1. `tier=ephemeral` + `lifecycle_state=deprecated`
   - Ephemeral adapters must skip the deprecated state

2. `lifecycle_state=retired` → any other state
   - Retired adapters cannot be revived

3. Backward state transitions (e.g., `active` → `draft`)
   - States can only move forward in the lifecycle

**Validation**:
```rust
use adapteros_db::metadata::{validate_state_transition, LifecycleState};

let result = validate_state_transition(
    LifecycleState::Active,
    LifecycleState::Deprecated,
    "ephemeral"
);
assert!(result.is_err()); // ephemeral cannot be deprecated
```

## API Response Format

All API responses **MUST** include a `schema_version` field for version tracking:

```json
{
  "schema_version": "1.0.0",
  "adapter": {
    "id": "adapter-123",
    "version": "2.1.0",
    "lifecycle_state": "active",
    ...
  }
}
```

## Telemetry Bundle Versioning

Telemetry bundles **MUST** include a `schema_version` field in the metadata:

```json
{
  "bundle_id": "bundle-456",
  "schema_version": "1.0.0",
  "events": [...]
}
```

This allows downstream systems to parse telemetry correctly even as the schema evolves.

## Migration Checklist

When making schema changes, use this checklist:

### For Minor Changes (Additive Only)
- [ ] Add database migration with `DEFAULT` values
- [ ] Update structs with new optional fields
- [ ] Update canonical metadata structs (`AdapterMeta`, `AdapterStackMeta`)
- [ ] Update all SQL queries to include new fields
- [ ] Update API response serialization
- [ ] Add conversion logic in `From` implementations
- [ ] Update tests to cover new fields
- [ ] Update API documentation
- [ ] Increment minor version in `API_SCHEMA_VERSION`

### For Major Changes (Breaking)
- [ ] All minor change steps above
- [ ] Document breaking changes in CHANGELOG
- [ ] Create data migration script
- [ ] Add backward compatibility layer (if feasible)
- [ ] Update error messages for incompatible clients
- [ ] Add migration verification tests
- [ ] Update deployment documentation
- [ ] Increment major version in `API_SCHEMA_VERSION`
- [ ] Communicate changes to users

## Examples

### Example 1: Adding a New Optional Field (Minor Change)

**Migration** (`migrations/NNNN_add_adapter_priority.sql`):
```sql
ALTER TABLE adapters ADD COLUMN priority INTEGER DEFAULT 50;
```

**Struct Update** (`crates/adapteros-db/src/metadata.rs`):
```rust
pub struct AdapterMeta {
    // ... existing fields ...
    pub priority: Option<i32>,  // NEW: Optional field
}
```

**Conversion** (`crates/adapteros-db/src/metadata.rs`):
```rust
impl From<crate::adapters::Adapter> for AdapterMeta {
    fn from(adapter: crate::adapters::Adapter) -> Self {
        AdapterMeta {
            // ... existing mappings ...
            priority: adapter.priority,  // Maps to Option<i32>
        }
    }
}
```

**Version Bump**: `1.0.0` → `1.1.0`

### Example 2: Renaming a Field (Major Change)

**Migration** (`migrations/NNNN_rename_tier_to_persistence_level.sql`):
```sql
-- Rename column
ALTER TABLE adapters RENAME COLUMN tier TO persistence_level;

-- Update values to new format
UPDATE adapters SET persistence_level = CASE
    WHEN persistence_level = 'ephemeral' THEN 'transient'
    WHEN persistence_level = 'warm' THEN 'cached'
    WHEN persistence_level = 'persistent' THEN 'durable'
END;
```

**Struct Update** (`crates/adapteros-db/src/metadata.rs`):
```rust
pub struct AdapterMeta {
    // OLD: pub tier: String,
    pub persistence_level: PersistenceLevel,  // NEW: Enum type
}
```

**Version Bump**: `1.2.3` → `2.0.0`

**Migration Notice**: "Version 2.0.0 renames `tier` to `persistence_level` and changes values. See migration guide for details."

## References

- **Canonical Metadata**: `crates/adapteros-db/src/metadata.rs`
- **Database Migrations**: `migrations/`
- **Migration Signing**: `./scripts/sign_migrations.sh`
- **API Schema Version**: `adapteros_db::metadata::API_SCHEMA_VERSION`

## Maintainer Notes

**Copyright**: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

**Last Updated**: 2025-11-17 (PRD-02 implementation)

**Signature**: This document follows the versioning policy established in PRD-02. All changes to this policy require explicit approval and documentation.
