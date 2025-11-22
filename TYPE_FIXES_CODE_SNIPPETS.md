# Type Consistency Fixes - Code Snippets

## 1. Tier Type Change

### Before (INCORRECT)

**Rust - RegisterAdapterRequest:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RegisterAdapterRequest {
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    pub tier: i32,                    // ❌ i32 - doesn't match database
    pub languages: Vec<String>,
    pub framework: Option<String>,
}
```

**Rust - AdapterResponse:**
```rust
pub struct AdapterResponse {
    pub tier: i32,                     // ❌ i32 - causes conversion overhead
    pub languages: Vec<String>,
    pub framework: Option<String>,
    // ... missing 10 fields
}
```

**TypeScript - Adapter:**
```typescript
export interface Adapter {
    tier: number;                      // ❌ number - mismatch with database
    languages_json?: string;           // ❌ wrong field name
}
```

**Handler Logic (COMPLEX CONVERSION):**
```rust
// Convert tier string to i32: persistent=0, warm=1, ephemeral=2
let tier_int = match adapter.tier.as_str() {
    "persistent" => 0,
    "warm" => 1,
    "ephemeral" => 2,
    _ => 1, // default to warm
};

let params = AdapterRegistrationBuilder::new()
    .tier(&tier_str)  // Database wants string
    // ...
    .build()?;

// But then return i32 in response
AdapterResponse {
    tier: tier_int,    // ❌ Converting back to i32!
}
```

### After (CORRECT)

**Rust - RegisterAdapterRequest:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RegisterAdapterRequest {
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    /// Adapter tier: 'persistent', 'warm', or 'ephemeral'
    pub tier: String,                  // ✓ String - matches database
    pub languages: Vec<String>,
    pub framework: Option<String>,
    /// Adapter category: 'code', 'framework', 'codebase', or 'ephemeral'
    pub category: String,              // ✓ Added - required field
    /// Adapter scope: 'global', 'tenant', 'repo', or 'commit'
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,         // ✓ Added - optional field
    /// Expiration timestamp (ISO 8601 format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,    // ✓ Added - TTL support
}
```

**Rust - AdapterResponse:**
```rust
pub struct AdapterResponse {
    /// Storage tier: 'persistent', 'warm', or 'ephemeral'
    pub tier: String,                  // ✓ String - no conversion needed
    /// Supported programming languages
    pub languages: Vec<String>,
    pub framework: Option<String>,
    /// Adapter category: 'code', 'framework', 'codebase', or 'ephemeral'
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,      // ✓ Added
    /// Adapter scope: 'global', 'tenant', 'repo', or 'commit'
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,         // ✓ Added
    /// Framework identifier for code intelligence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework_id: Option<String>,  // ✓ Added
    /// Framework version for code intelligence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework_version: Option<String>,  // ✓ Added
    /// Repository identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,       // ✓ Added
    /// Git commit SHA
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,    // ✓ Added
    /// Adapter intent/purpose
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,        // ✓ Added
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,    // ✓ Added
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,          // ✓ Added
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<i64>,     // ✓ Added
}
```

**TypeScript - Adapter:**
```typescript
export interface Adapter {
    // Storage tier: 'persistent', 'warm', or 'ephemeral'
    tier: string;                      // ✓ string - matches Rust
    // Supported programming languages
    languages?: string[];              // ✓ array - matches Rust
    // Languages in JSON string format (for backward compatibility)
    languages_json?: string;           // ✓ kept for compatibility
    framework?: string;
    // Code intelligence fields
    category?: 'code' | 'framework' | 'codebase' | 'ephemeral';  // ✓ Added
    scope?: 'global' | 'tenant' | 'repo' | 'commit';             // ✓ Added
    framework_id?: string;             // ✓ Added
    framework_version?: string;        // ✓ Added
    repo_id?: string;                  // ✓ Added
    commit_sha?: string;               // ✓ Added
    intent?: string;                   // ✓ Added
    // Lifecycle state management
    current_state?: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
    runtime_state?: string;            // ✓ Added
    pinned?: boolean;                  // ✓ Added
    memory_bytes?: number;             // ✓ Added
}
```

**Handler Logic (SIMPLIFIED):**
```rust
// Validate tier is one of the allowed values
if !["persistent", "warm", "ephemeral"].contains(&req.tier.as_str()) {
    return Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new("tier must be one of: 'persistent', 'warm', or 'ephemeral'")
            .with_code("BAD_REQUEST")),
    ));
}

// Validate category is provided
if req.category.is_empty() {
    return Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new("category is required").with_code("BAD_REQUEST")),
    ));
}

// Direct string usage - no conversion!
let params = AdapterRegistrationBuilder::new()
    .tier(&req.tier)                   // ✓ String passed directly
    .category(Some(req.category.clone()))
    .scope(req.scope.clone())
    .expires_at(req.expires_at.clone())
    .build()?;

// Direct string in response - no conversion!
AdapterResponse {
    tier: req.tier,                    // ✓ String used directly
    category: Some(req.category.clone()),
    scope: req.scope.clone(),
}
```

---

## 2. Languages Field Standardization

### Before (INCONSISTENT)

**Rust - Correct:**
```rust
pub struct RegisterAdapterRequest {
    pub languages: Vec<String>,        // ✓ Correct
}
```

**TypeScript - Inconsistent:**
```typescript
export interface RegisterAdapterRequest {
    languages_json?: string;           // ❌ JSON string instead of array
}

export interface Adapter {
    languages_json?: string;           // ❌ JSON string instead of array
}
```

### After (CONSISTENT)

**Rust - Correct (no change needed):**
```rust
pub struct RegisterAdapterRequest {
    pub languages: Vec<String>,        // ✓ Stays as array
}
```

**TypeScript - Consistent:**
```typescript
export interface RegisterAdapterRequest {
    languages: string[];               // ✓ Array of strings (required)
}

export interface Adapter {
    languages?: string[];              // ✓ Array of strings (optional)
    languages_json?: string;           // ✓ Kept for backward compatibility
}
```

---

## 3. RegisterAdapterRequest Field Additions

### Before (INCOMPLETE)

```rust
pub struct RegisterAdapterRequest {
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    pub tier: i32,
    pub languages: Vec<String>,
    pub framework: Option<String>,
    // ❌ Missing: category, scope, expires_at
}
```

### After (COMPLETE)

```rust
pub struct RegisterAdapterRequest {
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    /// Adapter tier: 'persistent', 'warm', or 'ephemeral'
    pub tier: String,
    pub languages: Vec<String>,
    pub framework: Option<String>,
    /// Adapter category: 'code', 'framework', 'codebase', or 'ephemeral'
    pub category: String,              // ✓ Added - required
    /// Adapter scope: 'global', 'tenant', 'repo', or 'commit'
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,         // ✓ Added - optional
    /// Expiration timestamp (ISO 8601 format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,    // ✓ Added - optional
}
```

---

## 4. JSON Wire Format Examples

### Before (INCORRECT - Loses Information)

**POST /v1/adapters/register:**
```json
{
  "adapter_id": "rust-expert",
  "name": "Rust Code Expert",
  "hash_b3": "b3:abcd1234...",
  "rank": 16,
  "tier": 0,
  "languages": ["rust"],
  "framework": "llm"
  // ❌ Missing: category, scope (required but not in schema)
}
```

**GET /v1/adapters/:id:**
```json
{
  "schema_version": "v1",
  "id": "uuid-123",
  "adapter_id": "rust-expert",
  "name": "Rust Code Expert",
  "hash_b3": "b3:abcd1234...",
  "rank": 16,
  "tier": 0,
  "languages": ["rust"],
  "framework": "llm",
  "created_at": "2025-11-22T10:00:00Z",
  // ❌ Missing: category, scope, framework_id, framework_version,
  //            repo_id, commit_sha, intent, pinned, memory_bytes
}
```

### After (CORRECT - Complete Information)

**POST /v1/adapters/register:**
```json
{
  "adapter_id": "rust-expert",
  "name": "Rust Code Expert",
  "hash_b3": "b3:abcd1234...",
  "rank": 16,
  "tier": "persistent",
  "languages": ["rust"],
  "framework": "llm",
  "category": "code",
  "scope": "tenant",
  "expires_at": "2025-12-31T23:59:59Z"
}
```

**GET /v1/adapters/:id:**
```json
{
  "schema_version": "v1",
  "id": "uuid-123",
  "adapter_id": "rust-expert",
  "name": "Rust Code Expert",
  "hash_b3": "b3:abcd1234...",
  "rank": 16,
  "tier": "persistent",
  "languages": ["rust"],
  "framework": "llm",
  "category": "code",
  "scope": "tenant",
  "framework_id": "rust-1.70",
  "framework_version": "1.70.0",
  "repo_id": "repo-github-123",
  "commit_sha": "abc123def456...",
  "intent": "code-review",
  "created_at": "2025-11-22T10:00:00Z",
  "updated_at": "2025-11-22T15:30:00Z",
  "version": "1.0.0",
  "lifecycle_state": "active",
  "runtime_state": "warm",
  "pinned": false,
  "memory_bytes": 1024000,
  "stats": {
    "total_activations": 100,
    "selected_count": 85,
    "avg_gate_value": 0.92,
    "selection_rate": 85.0
  }
}
```

---

## 5. Handler Update Example - register_adapter

### Before (INCOMPLETE, CONVERSION OVERHEAD)

```rust
pub async fn register_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RegisterAdapterRequest>,
) -> Result<(StatusCode, Json<AdapterResponse>), (StatusCode, Json<ErrorResponse>)> {
    // ... validation code ...

    // Complex tier conversion
    let tier_str = match req.tier {
        0 => "persistent".to_string(),
        1 => "warm".to_string(),
        _ => "ephemeral".to_string(),
    };

    // Missing category/scope/expires_at
    let params = AdapterRegistrationBuilder::new()
        .adapter_id(&req.adapter_id)
        .name(&req.name)
        .hash_b3(&req.hash_b3)
        .rank(req.rank)
        .tier(&tier_str)
        .languages_json(Some(languages_json.clone()))
        .framework(req.framework.clone())
        .build()?;

    // ... database insertion ...

    // Incomplete response
    Ok((
        StatusCode::CREATED,
        Json(AdapterResponse {
            adapter_id: req.adapter_id.clone(),
            tier: req.tier,  // ❌ Still i32!
            languages: req.languages,
            // ❌ Missing fields
        }),
    ))
}
```

### After (COMPLETE, DIRECT USAGE)

```rust
pub async fn register_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RegisterAdapterRequest>,
) -> Result<(StatusCode, Json<AdapterResponse>), (StatusCode, Json<ErrorResponse>)> {
    // ... existing validation ...

    // ✓ Validate tier is one of the allowed values
    if !["persistent", "warm", "ephemeral"].contains(&req.tier.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("tier must be one of: 'persistent', 'warm', or 'ephemeral'")
                .with_code("BAD_REQUEST")),
        ));
    }

    // ✓ Validate category is provided
    if req.category.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("category is required")
                .with_code("BAD_REQUEST")),
        ));
    }

    // ✓ Direct string usage - no conversion!
    let params = AdapterRegistrationBuilder::new()
        .adapter_id(&req.adapter_id)
        .name(&req.name)
        .hash_b3(&req.hash_b3)
        .rank(req.rank)
        .tier(&req.tier)                    // ✓ Direct string
        .languages_json(Some(languages_json.clone()))
        .framework(req.framework.clone())
        .category(Some(req.category.clone()))   // ✓ Added
        .scope(req.scope.clone())               // ✓ Added
        .expires_at(req.expires_at.clone())     // ✓ Added
        .build()?;

    // ... database insertion ...

    // ✓ Complete response
    Ok((
        StatusCode::CREATED,
        Json(AdapterResponse {
            adapter_id: req.adapter_id.clone(),
            tier: req.tier,                      // ✓ Direct string
            languages: req.languages,
            category: Some(req.category.clone()), // ✓ Added
            scope: req.scope.clone(),             // ✓ Added
            framework_id: None,                   // ✓ Added
            framework_version: None,              // ✓ Added
            repo_id: None,                        // ✓ Added
            commit_sha: None,                     // ✓ Added
            intent: None,                         // ✓ Added
            pinned: Some(false),                  // ✓ Added
            memory_bytes: Some(0),                // ✓ Added
            // ... rest of fields ...
        }),
    ))
}
```

---

## 6. Database Alignment

### Schema (No Changes Needed)

```sql
CREATE TABLE adapters (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    tier TEXT NOT NULL CHECK(tier IN ('persistent','warm','ephemeral')),
    rank INTEGER NOT NULL,
    languages_json TEXT,
    framework TEXT,
    category TEXT,          -- Already in schema (migration 0068+)
    scope TEXT,             -- Already in schema (migration 0068+)
    framework_id TEXT,      -- Already in schema (migration 0068+)
    framework_version TEXT, -- Already in schema (migration 0068+)
    repo_id TEXT,           -- Already in schema (migration 0068+)
    commit_sha TEXT,        -- Already in schema (migration 0068+)
    intent TEXT,            -- Already in schema (migration 0068+)
    expires_at TEXT,        -- Already in schema (migration 0060+)
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE pinned_adapters (
    tenant_id TEXT NOT NULL REFERENCES tenants(id),
    adapter_id TEXT NOT NULL,
    pinned_until TEXT,
    reason TEXT,
    pinned_by TEXT REFERENCES users(id),
    PRIMARY KEY (tenant_id, adapter_id)
);
```

**API types now properly expose all these fields via AdapterResponse.**

