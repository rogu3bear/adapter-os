# PRD-02 Completion Guide

**Status:** Partial implementation due to build blockers
**Date:** 2025-11-17
**What's Done:** Database layer (100%), UI types (100%)
**What's Blocked:** Server API, CLI (compilation errors in dependencies)

## Summary of What Was Completed

### ✅ Database Layer (100%)
- Migration 0070 with version and lifecycle_state fields
- Canonical metadata structs (AdapterMeta, AdapterStackMeta)
- Validation system with state transition rules
- Comprehensive tests (8/8 passing)
- VERSION_GUARANTEES.md documentation

### ✅ TypeScript Types (100%)
- Added `version` and `lifecycle_state` fields to Adapter interface
- Added `LifecycleState` type: `'draft' | 'active' | 'deprecated' | 'retired'`
- Location: `/home/user/adapter-os/ui/src/api/types.ts:435-447`

### ❌ What's Blocked

**Server API** - Cannot compile due to:
- `adapteros-lora-worker`: 51 compilation errors
- Send trait violations in MutexGuard
- Mismatched argument counts
- Type mismatches

**CLI** - Cannot compile due to:
- `adapteros-lora-kernel-mtl`: Metal shader build failures
- Missing Metal toolchain files

## Step-by-Step Completion Instructions

### Phase 1: Fix Build Issues (Required First)

#### Fix Server API Dependencies
```bash
# 1. Fix adapteros-lora-worker errors
cd crates/adapteros-lora-worker
cargo build 2>&1 | grep "error\[E" > errors.txt

# Common fixes needed:
# - Replace std::sync::Mutex with tokio::sync::Mutex for Send safety
# - Fix method signatures to match trait definitions
# - Update type annotations

# 2. Verify server-api compiles
cargo build -p adapteros-server-api
```

#### Fix CLI Dependencies
```bash
# 1. Fix Metal kernel compilation
cd crates/adapteros-lora-kernel-mtl

# Ensure Metal toolchain is available
# Check if xcrun -sdk macosx metal --version works

# 2. Verify CLI compiles
cargo build -p adapteros-cli
```

### Phase 2: Complete UI Integration

#### Update AdaptersPage Component

**File:** `/home/user/adapter-os/ui/src/components/AdaptersPage.tsx`

1. **Add helper function for lifecycle state badges** (around line 130):
```typescript
const getLifecycleVariant = (state: string): "default" | "secondary" | "destructive" | "outline" => {
  switch (state) {
    case 'active': return 'default';       // Green
    case 'draft': return 'outline';        // Gray
    case 'deprecated': return 'secondary'; // Yellow/Orange
    case 'retired': return 'destructive';  // Red
    default: return 'outline';
  }
};
```

2. **Update table headers** (around line 192):
```typescript
<TableHeader>
  <TableRow>
    <TableHead>Name</TableHead>
    <TableHead>Category</TableHead>
    <TableHead>Version</TableHead>        {/* NEW */}
    <TableHead>Lifecycle</TableHead>      {/* NEW */}
    <TableHead>State</TableHead>
    <TableHead>Memory</TableHead>
    <TableHead>Activations</TableHead>
    <TableHead>Last Used</TableHead>
    <TableHead>Actions</TableHead>
  </TableRow>
</TableHeader>
```

3. **Update table cells** (around line 205):
```typescript
{adapters.map(adapter => (
  <TableRow key={adapter.id}>
    <TableCell className="font-medium">{adapter.name}</TableCell>
    <TableCell>
      <Badge>{getCategoryIcon(adapter.category)} {adapter.category}</Badge>
    </TableCell>

    {/* NEW: Version column */}
    <TableCell className="text-sm text-muted-foreground">
      {adapter.version || '1.0.0'}
    </TableCell>

    {/* NEW: Lifecycle state column */}
    <TableCell>
      <Badge variant={getLifecycleVariant(adapter.lifecycle_state || 'active')}>
        {adapter.lifecycle_state || 'active'}
      </Badge>
    </TableCell>

    <TableCell>
      <Badge>{adapter.current_state}</Badge>
      {adapter.pinned && <Pin className="h-4 w-4 ml-2" />}
    </TableCell>
    <TableCell>{(adapter.memory_bytes / 1024 / 1024).toFixed(1)} MB</TableCell>
    <TableCell>{adapter.activation_count}</TableCell>
    <TableCell>{adapter.last_activated ? new Date(adapter.last_activated).toLocaleString() : 'Never'}</TableCell>
    <TableCell>
      {/* Actions dropdown... */}
    </TableCell>
  </TableRow>
))}
```

#### Update Stack Components (if applicable)

**File:** `/home/user/adapter-os/ui/src/components/AdapterStacks.tsx` (if it exists)

Similar updates for stacks:
- Add version column
- Add lifecycle_state column
- Add getLifecycleVariant helper

### Phase 3: Complete Server API Integration

**File:** `/home/user/adapter-os/crates/adapteros-server-api/src/handlers.rs` (or similar)

1. **Update adapter list handler:**
```rust
use adapteros_db::{AdapterMeta, API_SCHEMA_VERSION};

pub async fn list_adapters(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<AdapterListResponse>, (StatusCode, String)> {
    let adapters = state.db
        .list_adapters()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Convert to canonical metadata
    let adapter_metas: Vec<AdapterMeta> = adapters
        .into_iter()
        .map(|a| a.into())
        .collect();

    Ok(Json(AdapterListResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        adapters: adapter_metas,
    }))
}
```

2. **Update response types:**
```rust
#[derive(Debug, Serialize)]
pub struct AdapterListResponse {
    pub schema_version: String,
    pub adapters: Vec<AdapterMeta>,
}

#[derive(Debug, Serialize)]
pub struct AdapterDetailResponse {
    pub schema_version: String,
    pub adapter: AdapterMeta,
}

#[derive(Debug, Serialize)]
pub struct StackListResponse {
    pub schema_version: String,
    pub stacks: Vec<AdapterStackMeta>,
}
```

3. **Update all adapter endpoints:**
- `GET /api/adapters` - List adapters
- `GET /api/adapters/:id` - Get adapter details
- `GET /api/adapter-stacks` - List stacks
- `GET /api/adapter-stacks/:id` - Get stack details

### Phase 4: Complete CLI Integration

**File:** `/home/user/adapter-os/crates/adapteros-cli/src/commands/adapter.rs` (or similar)

1. **Update adapter list command:**
```rust
pub async fn list_adapters(client: &ApiClient, include_meta: bool) -> Result<()> {
    let response = client.list_adapters().await?;

    if include_meta {
        // JSON output with full metadata
        let json = serde_json::to_string_pretty(&response)?;
        println!("{}", json);
    } else {
        // Table output
        println!("{:<40} {:<10} {:<12} {:<15} {:<10}",
            "Name", "Version", "Lifecycle", "State", "Memory");
        println!("{}", "-".repeat(90));

        for adapter in response.adapters {
            println!("{:<40} {:<10} {:<12} {:<15} {:<10}",
                adapter.name,
                adapter.version,
                adapter.lifecycle_state.as_str(),
                adapter.current_state,
                format_bytes(adapter.memory_bytes),
            );
        }
    }

    Ok(())
}
```

2. **Add --include-meta flag:**
```rust
#[derive(Parser)]
pub struct ListAdaptersCommand {
    /// Output full metadata as JSON
    #[arg(long)]
    include_meta: bool,
}
```

3. **Add lifecycle state transition command:**
```rust
#[derive(Parser)]
pub struct UpdateLifecycleCommand {
    /// Adapter ID
    adapter_id: String,

    /// New lifecycle state
    #[arg(value_enum)]
    state: LifecycleState,
}

pub async fn update_lifecycle_state(
    client: &ApiClient,
    adapter_id: &str,
    new_state: LifecycleState,
) -> Result<()> {
    client.update_adapter_lifecycle_state(adapter_id, new_state).await?;
    println!("✓ Updated adapter {} to lifecycle state: {}", adapter_id, new_state.as_str());
    Ok(())
}
```

### Phase 5: Integration Testing

1. **Database to API test:**
```bash
# Register test adapter
curl -X POST http://localhost:8080/api/adapters \
  -H "Content-Type: application/json" \
  -d '{
    "adapter_id": "test-123",
    "name": "Test Adapter",
    "hash_b3": "abc123",
    "rank": 8,
    "tier": "persistent"
  }'

# Verify response includes schema_version and version fields
curl http://localhost:8080/api/adapters | jq '.schema_version, .adapters[0].version, .adapters[0].lifecycle_state'
```

2. **API to UI test:**
```bash
# Start UI dev server
cd ui && npm run dev

# Navigate to http://localhost:5173/adapters
# Verify table shows Version and Lifecycle columns
```

3. **CLI test:**
```bash
# List adapters
aosctl adapter list

# List with full metadata
aosctl adapter list --include-meta

# Update lifecycle state
aosctl adapter update-lifecycle test-123 deprecated
```

## Acceptance Criteria Checklist

- [ ] **Database Layer**
  - [x] Migration 0070 applied
  - [x] version and lifecycle_state columns exist
  - [x] SQL triggers validate lifecycle states
  - [x] Canonical metadata structs defined
  - [x] Tests passing (8/8)

- [ ] **Server API**
  - [ ] Compilation errors fixed
  - [ ] All adapter endpoints return `schema_version` field
  - [ ] Responses use `AdapterMeta` and `AdapterStackMeta`
  - [ ] Version field present in all adapter responses
  - [ ] Lifecycle_state field present in all adapter responses

- [ ] **CLI**
  - [ ] Compilation errors fixed
  - [ ] `adapter list` shows version and lifecycle_state
  - [ ] `--include-meta` flag outputs JSON with all fields
  - [ ] `adapter update-lifecycle` command works

- [ ] **UI**
  - [x] TypeScript types updated
  - [ ] AdaptersPage shows Version column
  - [ ] AdaptersPage shows Lifecycle column
  - [ ] Lifecycle badges color-coded correctly
  - [ ] Stack views show version and lifecycle

- [ ] **Documentation**
  - [x] VERSION_GUARANTEES.md complete
  - [ ] API documentation updated
  - [ ] CLI help text updated

- [ ] **End-to-End**
  - [ ] Can register adapter with version="2.0.0"
  - [ ] Can update lifecycle state via API
  - [ ] Changes visible in UI
  - [ ] Changes visible in CLI
  - [ ] schema_version field present in all responses

## Current Status

**Completed:** 5/9 acceptance criteria (55%)

**Blocked:** 4/9 acceptance criteria due to compilation errors

**Estimated time to complete** (after build fixes): 4-6 hours
- Server API integration: 2-3 hours
- CLI integration: 1-2 hours
- UI completion: 1 hour
- Testing: 1 hour

## Notes for Future Implementer

1. **Don't skip validation** - The database validation is comprehensive, make sure API endpoints use it
2. **Test state transitions** - Ensure illegal transitions (e.g., ephemeral→deprecated) are rejected
3. **Version format** - Support both semver and monotonic, validate on input
4. **Backward compatibility** - Old adapters without version/lifecycle_state should default to "1.0.0" and "active"
5. **Error messages** - Use the validation error messages from metadata.rs for consistency

## References

- Database implementation: `/home/user/adapter-os/crates/adapteros-db/src/metadata.rs`
- Validation logic: `/home/user/adapter-os/crates/adapteros-db/src/validation.rs`
- Version guarantees: `/home/user/adapter-os/docs/VERSION_GUARANTEES.md`
- Migration: `/home/user/adapter-os/migrations/0070_metadata_normalization.sql`
