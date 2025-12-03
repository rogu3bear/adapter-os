# Adapter Taxonomy & Naming Specification

**Version:** 1.0
**Status:** Canonical
**Last Updated:** 2025-11-16

---

## Overview

This document defines the canonical naming conventions and taxonomy for AdapterOS adapters and adapter stacks. These rules enforce semantic clarity, prevent namespace collisions, and establish inheritance/lineage relationships.

---

## Adapter Naming Scheme

### Format

```
{tenant}/{domain}/{purpose}/{revision}
```

**Components:**

1. **tenant**: Tenant namespace (alphanumeric + hyphens, 2-32 chars)
2. **domain**: Technical domain or knowledge area (alphanumeric + hyphens, 2-48 chars)
3. **purpose**: Specific task or capability (alphanumeric + hyphens, 2-64 chars)
4. **revision**: Revision identifier (format: `r{NNN}` where NNN is 3+ digits)

### Examples

```
shop-floor/hydraulics/troubleshooting/r001
dentist-office/scheduling/appointment-booking/r042
global/code/rust-analyzer/r015
acme-corp/legal/contract-review/r003
```

### Validation Rules

**1. Tenant Component:**
- Pattern: `^[a-z0-9][a-z0-9-]{0,30}[a-z0-9]$`
- Must start and end with alphanumeric
- No consecutive hyphens
- Reserved names: `system`, `admin`, `root`, `global`, `default`, `test`

**2. Domain Component:**
- Pattern: `^[a-z0-9][a-z0-9-]{0,46}[a-z0-9]$`
- Must start and end with alphanumeric
- Examples: `hydraulics`, `code`, `legal`, `medical-imaging`

**3. Purpose Component:**
- Pattern: `^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$`
- Must be descriptive and task-specific
- Examples: `troubleshooting`, `rust-analyzer`, `contract-review`

**4. Revision Component:**
- Pattern: `^r[0-9]{3,}$`
- Must start with 'r' followed by 3+ digits
- Zero-padded: `r001`, `r042`, `r1234`
- Monotonically increasing for same adapter path

### Combined Pattern

```regex
^[a-z0-9][a-z0-9-]{0,30}[a-z0-9]/[a-z0-9][a-z0-9-]{0,46}[a-z0-9]/[a-z0-9][a-z0-9-]{0,62}[a-z0-9]/r[0-9]{3,}$
```

**Max total length:** 200 characters

---

## Adapter Stacks Naming

### Format

```
stack.{namespace}.{identifier}
```

**Components:**

1. **namespace**: Organizational or functional grouping
2. **identifier**: Specific stack purpose

### Examples

```
stack.safe-default
stack.dentist-office
stack.shop-floor-nightshift
stack.acme-corp.production
stack.global.code-review
```

### Validation Rules

**1. Stack Prefix:**
- Must always start with `stack.`

**2. Namespace:**
- Pattern: `^[a-z0-9][a-z0-9-]{0,30}[a-z0-9]$`
- Same rules as tenant component

**3. Identifier (optional):**
- Pattern: `^[a-z0-9][a-z0-9-]{0,46}[a-z0-9]$`
- May be omitted for global stacks (e.g., `stack.safe-default`)

**4. Combined Pattern:**

```regex
^stack\.([a-z0-9][a-z0-9-]{0,30}[a-z0-9])(\.[a-z0-9][a-z0-9-]{0,46}[a-z0-9])?$
```

**Max total length:** 100 characters

---

## Tree Semantics & Lineage

### Parent Relationship

Adapters can declare a parent to inherit domain-specific behavior:

```toml
[lineage]
parent = "shop-floor/hydraulics/general/r023"
fork_reason = "Specialized for CNC machines"
```

**Parent Rules:**

1. Parent must exist in registry before child registration
2. Parent must be in same tenant namespace
3. Parent must be in same or broader domain
4. Circular dependencies forbidden

**Inheritance Semantics:**

- Child inherits ACL from parent (unless overridden)
- Child inherits tier from parent (unless explicitly downgraded)
- Child training should build on parent's knowledge

### Fork vs. Extend

**Fork:** Create independent lineage from existing adapter
```toml
[lineage]
parent = "shop-floor/hydraulics/general/r023"
fork_reason = "Divergent use case: mobile equipment"
fork_type = "independent"
```

**Extend:** Incremental improvement on parent
```toml
[lineage]
parent = "shop-floor/hydraulics/general/r023"
fork_reason = "Add support for ISO 15380 fluids"
fork_type = "extension"
```

**Rules:**

- **Fork**: Allowed to change domain/purpose, breaks compatibility expectations
- **Extend**: Same domain/purpose, maintains compatibility, incremental changes only

### Revision Semantics

- **r001-r099**: Experimental/development versions
- **r100-r999**: Production-ready versions
- **r1000+**: Long-term stable versions with extensive validation

**Revision Constraints:**

1. Cannot skip more than 5 revisions from latest (prevents accidental conflicts)
2. Revision must be unique for given tenant/domain/purpose path
3. Lower revisions can be deprecated but not deleted (preserve lineage)

---

## Uniqueness Constraints

### Database Constraints

**Adapters Table:**

```sql
-- Unique semantic name
UNIQUE(tenant_id, domain, purpose, revision)

-- Unique display name (full path)
UNIQUE(adapter_name)

-- Hash uniqueness (content-addressed)
UNIQUE(hash_b3)
```

**Adapter Stacks Table:**

```sql
-- Unique stack name
UNIQUE(name)
```

### Application-Level Validation

1. **Tenant namespace isolation**: No cross-tenant adapter references (except `global/*`)
2. **Domain consistency**: Child adapters must stay within parent's domain tree
3. **Revision monotonicity**: New revisions must be > max existing revision for path

---

## Reserved Namespaces

### Reserved Tenants

- `system`: System-level adapters (managed by AdapterOS)
- `global`: Shared adapters across all tenants
- `test`: Testing and CI/CD only

### Reserved Domains

- `core`: Core framework functionality
- `internal`: Internal use only
- `deprecated`: Marked for removal

### Reserved Stacks

- `stack.safe-default`: Default fallback stack
- `stack.system.*`: System-managed stacks

---

## API & UI Exposure

### Display Names

**Adapters:**
- UI: `{tenant}/{domain}/{purpose} (rev {N})`
- Example: `shop-floor/hydraulics/troubleshooting (rev 42)`

**Stacks:**
- UI: Stack name as-is
- Example: `stack.shop-floor-nightshift`

### Internal IDs

- Database primary keys remain UUIDs or content hashes
- Semantic names used for lookups and display only
- Mapping table: `adapter_id` (internal) ↔ `adapter_name` (semantic)

### REST API Endpoints

```
GET  /api/adapters?tenant={tenant}&domain={domain}
GET  /api/adapters/{tenant}/{domain}/{purpose}/{revision}
POST /api/adapters/{tenant}/{domain}/{purpose}  # Auto-increment revision
GET  /api/stacks/{stack_name}
```

---

## Migration Strategy

### Phase 1: Add Columns (Non-Breaking)

```sql
ALTER TABLE adapters ADD COLUMN adapter_name TEXT UNIQUE;
ALTER TABLE adapters ADD COLUMN tenant_namespace TEXT;
ALTER TABLE adapters ADD COLUMN domain TEXT;
ALTER TABLE adapters ADD COLUMN purpose TEXT;
ALTER TABLE adapters ADD COLUMN revision TEXT;
ALTER TABLE adapters ADD COLUMN parent_id TEXT REFERENCES adapters(id);
ALTER TABLE adapters ADD COLUMN fork_type TEXT CHECK(fork_type IN ('independent', 'extension'));
ALTER TABLE adapters ADD COLUMN fork_reason TEXT;
```

### Phase 2: Backfill Existing Data

- Generate semantic names from existing `id` or `name` fields
- Default tenant: `global` or derive from existing `tenant_id`
- Default domain: `general`
- Default purpose: Use existing `name` or `id`
- Default revision: `r001`

### Phase 3: Enforce Constraints

- Add NOT NULL constraints after backfill
- Add validation triggers
- Update application code to use semantic names

---

## Validation Implementation

### Rust Validation Module

Location: `crates/adapteros-core/src/naming.rs`

**Core Types:**

```rust
pub struct AdapterName {
    tenant: String,
    domain: String,
    purpose: String,
    revision: String,
}

pub struct StackName {
    namespace: String,
    identifier: Option<String>,
}
```

**Validation Functions:**

```rust
impl AdapterName {
    pub fn parse(name: &str) -> Result<Self>;
    pub fn validate(&self) -> Result<()>;
    pub fn to_string(&self) -> String;
    pub fn tenant(&self) -> &str;
    pub fn domain(&self) -> &str;
    pub fn purpose(&self) -> &str;
    pub fn revision_number(&self) -> u32;
}

impl StackName {
    pub fn parse(name: &str) -> Result<Self>;
    pub fn validate(&self) -> Result<()>;
    pub fn to_string(&self) -> String;
}
```

### Database Triggers (SQLite)

```sql
-- Validate adapter name format on insert/update
CREATE TRIGGER validate_adapter_name_format
BEFORE INSERT ON adapters
BEGIN
    SELECT CASE
        WHEN NEW.adapter_name NOT GLOB '[a-z0-9]*[a-z0-9]/[a-z0-9]*[a-z0-9]/[a-z0-9]*[a-z0-9]/r[0-9][0-9][0-9]*'
        THEN RAISE(ABORT, 'Invalid adapter name format')
    END;
END;

-- Validate parent exists before allowing child
CREATE TRIGGER validate_parent_exists
BEFORE INSERT ON adapters
WHEN NEW.parent_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT COUNT(*) FROM adapters WHERE id = NEW.parent_id) = 0
        THEN RAISE(ABORT, 'Parent adapter does not exist')
    END;
END;
```

---

## Testing Requirements

### Unit Tests

1. **Valid name parsing**: All example names parse correctly
2. **Invalid name rejection**: Malformed names rejected with clear errors
3. **Revision validation**: Monotonicity and format checks
4. **Parent relationship validation**: Circular dependency detection

### Integration Tests

1. **Registration workflow**: End-to-end adapter registration with semantic names
2. **Stack creation**: Valid and invalid stack names
3. **ACL inheritance**: Child inherits parent ACL
4. **Cross-tenant isolation**: Prevent unauthorized cross-tenant references

### Fuzz Testing

1. Random string inputs to naming validators
2. Boundary conditions (max lengths, special chars)
3. SQL injection attempts in names

---

## Compliance & Audit

### Audit Logging

All naming operations logged to telemetry:

```json
{
  "event_type": "adapter_registered",
  "adapter_name": "shop-floor/hydraulics/troubleshooting/r042",
  "parent_name": "shop-floor/hydraulics/general/r023",
  "fork_type": "extension",
  "registered_by": "admin",
  "timestamp": "2025-11-16T10:30:00Z"
}
```

### Naming Policy Enforcement

Policy pack: `naming_policy`

**Rules:**

1. Reject names with profanity or offensive terms
2. Reject names exceeding length limits
3. Enforce tenant isolation
4. Validate revision monotonicity

---

## API Usage Examples

### REST API Endpoints

#### Validate Adapter Name

```bash
POST /v1/adapters/validate-name
Content-Type: application/json

{
  "name": "shop-floor/hydraulics/troubleshooting/r042",
  "tenant_id": "shop-floor",
  "parent_name": "shop-floor/hydraulics/troubleshooting/r041",
  "latest_revision": 41
}
```

**Response:**
```json
{
  "valid": true,
  "violations": [],
  "parsed": {
    "tenant": "shop-floor",
    "domain": "hydraulics",
    "purpose": "troubleshooting",
    "revision": "r042",
    "revision_number": 42,
    "base_path": "shop-floor/hydraulics/troubleshooting",
    "display_name": "shop-floor/hydraulics/troubleshooting (rev 42)"
  }
}
```

**Validation Errors:**
```json
{
  "valid": false,
  "violations": [
    {
      "violation_type": "TenantIsolation",
      "component": "shop-floor",
      "reason": "Tenant mismatch: adapter tenant 'shop-floor' does not match requesting tenant 'tenant-a'",
      "suggestion": "Use tenant 'tenant-a' in the adapter name"
    }
  ],
  "parsed": null
}
```

#### Validate Stack Name

```bash
POST /v1/stacks/validate-name
Content-Type: application/json

{
  "name": "stack.production-env",
  "tenant_id": "tenant-a"
}
```

**Response:**
```json
{
  "valid": true,
  "violations": [],
  "parsed": {
    "namespace": "production-env",
    "identifier": null,
    "full_name": "stack.production-env"
  }
}
```

#### Get Next Revision Number

```bash
GET /v1/adapters/next-revision/shop-floor/hydraulics/troubleshooting
```

**Response:**
```json
{
  "next_revision": 43,
  "suggested_name": "shop-floor/hydraulics/troubleshooting/r043",
  "base_path": "shop-floor/hydraulics/troubleshooting"
}
```

### Rust API Examples

#### Parse and Validate Names

```rust
use adapteros_core::{AdapterName, StackName};

// Parse adapter name
let name = AdapterName::parse("shop-floor/hydraulics/troubleshooting/r042")?;
println!("Tenant: {}", name.tenant());
println!("Domain: {}", name.domain());
println!("Purpose: {}", name.purpose());
println!("Revision: {} ({})", name.revision(), name.revision_number()?);
println!("Display: {}", name.display_name());

// Parse stack name
let stack = StackName::parse("stack.production-env.primary")?;
println!("Namespace: {}", stack.namespace());
println!("Identifier: {:?}", stack.identifier());
```

#### Lineage Tracking

```rust
use adapteros_registry::Registry;
use adapteros_core::{AdapterName, ForkType};

let registry = Registry::open("./registry.db")?;

// Register parent adapter
let parent_name = AdapterName::parse("tenant-a/engineering/code-review/r001")?;
registry.register_adapter_with_name(
    "adapter-parent-id",
    Some(&parent_name),
    &hash,
    "persistent",
    16,
    &vec!["tenant-a".to_string()],
    None,
    None,
)?;

// Register child adapter (extension)
let child_name = AdapterName::parse("tenant-a/engineering/code-review/r002")?;
registry.register_adapter_with_name(
    "adapter-child-id",
    Some(&child_name),
    &hash,
    "persistent",
    16,
    &vec!["tenant-a".to_string()],
    Some("adapter-parent-id"),
    Some(ForkType::Extension),
)?;

// Query lineage
let lineage = registry.list_adapters_in_lineage("tenant-a", "engineering", "code-review")?;
println!("Found {} adapters in lineage", lineage.len());
```

#### Policy Enforcement

```rust
use adapteros_policy::packs::naming_policy::{NamingPolicy, NamingConfig, AdapterNameValidation};

let policy = NamingPolicy::new(NamingConfig::default());

let request = AdapterNameValidation {
    name: "tenant-a/engineering/code-review/r001".to_string(),
    tenant_id: "tenant-a".to_string(),
    parent_name: None,
    latest_revision: None,
};

// Validate adapter name
policy.validate_adapter_name(&request)?;

// Analyze violations without panicking
let violations = policy.analyze_adapter_name(&request);
for violation in violations {
    println!("{:?}: {}", violation.violation_type, violation.reason);
    if let Some(suggestion) = &violation.suggestion {
        println!("  Suggestion: {}", suggestion);
    }
}
```

---

## Future Extensions

### Planned Features

1. **Semantic versioning**: Optional semver-style revisions (`r1.2.3`)
2. **Tags**: Additional metadata tags (`#production`, `#experimental`)
3. **Namespaced stacks**: `stack.{tenant}.{purpose}` for tenant-specific stacks
4. **Alias support**: Friendly aliases for long semantic names

### Compatibility Guarantee

- All names matching v1.0 spec remain valid in future versions
- New features will be additive only (no breaking changes to existing names)

---

## References

- **RAG vs Adapter Positioning**: See [CLAUDE.md Section 8.4](../CLAUDE.md#84-rag-vs-adapter-positioning) for when to use adapters vs RAG
- **Database Schema**: `migrations/0001_init.sql`
- **Registry Implementation**: `crates/adapteros-registry/src/lib.rs`
- **Manifest Format**: `crates/adapteros-manifest/src/lib.rs`
- **Policy Packs**: `crates/adapteros-policy/src/packs/`

---

**Approved By:** AdapterOS Core Team
**Effective Date:** 2025-11-16
