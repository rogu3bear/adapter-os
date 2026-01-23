# Pagination Response Consolidation Plan

**Status**: Draft - Needs Design Review
**Created**: 2026-01-23
**Author**: Engineering

## Problem Statement

The codebase has 20+ different list/pagination response types with inconsistent field names, pagination styles, and capabilities. This creates:

1. **API inconsistency**: Clients must handle different response shapes per endpoint
2. **Type duplication**: Similar types defined in multiple locations
3. **Missing features**: Some endpoints lack pagination entirely, others lack cursor support
4. **Cognitive load**: Developers must remember which response type uses which field names

## Current State Analysis

### Canonical Generic Type

Located in `/Users/star/Dev/adapter-os/crates/adapteros-api-types/src/lib.rs:340`

```rust
pub struct PaginatedResponse<T> {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub data: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub limit: u32,
    pub pages: u32,
}
```

**Fields**: `schema_version`, `data`, `total`, `page`, `limit`, `pages`

### All List Response Types Inventory

#### Group A: Offset-Based Pagination (page/limit style)

| Type | Location | Data Field | Total Field | Page Fields | Notes |
|------|----------|------------|-------------|-------------|-------|
| `PaginatedResponse<T>` | api-types/lib.rs:340 | `data: Vec<T>` | `total: u64` | `page`, `limit`, `pages` | Generic canonical type |
| `TrainingJobListResponse` | api-types/training.rs:1601 | `jobs: Vec<TrainingJobResponse>` | `total: usize` | `page`, `page_size` | Uses `page_size` not `limit` |
| `ListUsersResponse` | api-types/admin.rs:32 | `users: Vec<UserResponse>` | `total: i64` | `page`, `page_size` (i64) | Uses i64 for pagination |
| `AuditLogsResponse` | server-api/response.rs:1252 | `logs: Vec<AuditLogResponse>` | `total: usize` | `limit`, `offset` | Offset-based, not page-based |

#### Group B: Total-Only (no pagination controls)

| Type | Location | Data Field | Total Field | Notes |
|------|----------|------------|-------------|-------|
| `ClientErrorsListResponse` | api-types/telemetry.rs:133 | `errors: Vec<ClientErrorItem>` | `total: usize` | Missing page/limit |
| `ErrorAlertRulesListResponse` | api-types/telemetry.rs:277 | `rules: Vec<ErrorAlertRuleResponse>` | `total: usize` | Missing page/limit |
| `ErrorAlertHistoryListResponse` | api-types/telemetry.rs:302 | `alerts: Vec<ErrorAlertHistoryResponse>` | `total: usize` | Missing page/limit |
| `DatasetListResponse` | api-types/training.rs:1447 | `datasets: Vec<DatasetResponse>` | `total: i64` | Has schema_version, missing page/limit |
| `ModelListResponse<T>` | api-types/models.rs:78 | `models: Vec<T>` | `total: usize` | Generic but no pagination |
| `ListPausedResponse` | api-types/review.rs:259 | `paused: Vec<PausedInferenceInfo>` | `total: usize` | Has schema_version |
| `ListPrefixTemplatesResponse` | api-types/prefix_templates.rs:133 | `templates: Vec<PrefixTemplate>` | `total: u64` | No pagination |
| `ListAdaptersResponse` | adapteros-api/types.rs:49 | `adapters: Vec<AdapterInfo>` | `total: usize` | Worker API type |

#### Group C: Cursor-Based Pagination

| Type | Location | Data Field | Cursor Fields | Notes |
|------|----------|------------|---------------|-------|
| `ListDiagRunsResponse` | api-types/diagnostics.rs:83 | `runs: Vec<DiagRunResponse>` | `next_cursor`, `has_more`, `total_count` | Full cursor support |
| `ListDiagEventsResponse` | api-types/diagnostics.rs:100 | `events: Vec<DiagEventResponse>` | `last_seq`, `has_more` | Sequence-based cursor |

#### Group D: No Pagination Metadata

| Type | Location | Data Field | Notes |
|------|----------|------------|-------|
| `AuditsResponse` | server-api/response.rs:784 | `items: Vec<AuditExtended>` | No total, no pagination |
| `SessionsResponse` | server-api/auth_enhanced/sessions.rs:17 | `sessions: Vec<SessionInfo>` | Has schema_version only |
| `TenantListResponse` | api-types/auth.rs:57 | `tenants: Vec<TenantSummary>` | Has schema_version only |
| `ApiKeyListResponse` | api-types/api_keys.rs:42 | `api_keys: Vec<ApiKeyInfo>` | No metadata at all |
| `RoutingDecisionsResponse` | server-api/response.rs:755 | `items: Vec<RoutingDecision>` | No pagination |
| `BatchItemsResponse` | server-api/response.rs:167 | `items: Vec<BatchItemResultResponse>` | No pagination |
| `CategoryPoliciesResponse` | server-api/response.rs:1155 | `policies: Vec<CategoryPolicyResponse>` | No pagination |

### Field Name Inconsistencies

| Concept | Variations Found |
|---------|-----------------|
| Data array | `data`, `items`, `adapters`, `jobs`, `datasets`, `users`, `templates`, `models`, `logs`, `errors`, `rules`, `alerts`, `sessions`, `tenants`, `api_keys`, `paused`, `runs`, `events`, `policies` |
| Total count | `total: u64`, `total: usize`, `total: i64`, `total_count: i64` |
| Page size | `limit: u32`, `limit: usize`, `page_size: u32`, `page_size: i64` |
| Page number | `page: u32`, `page: i64`, `offset: usize` |
| Total pages | `pages: u32` (only in PaginatedResponse) |
| Cursor | `next_cursor: Option<String>`, `last_seq: Option<i64>` |
| Has more | `has_more: bool` |

### Type Location Duplication

Some types are duplicated across crates:
- `SessionsResponse` in both `server-api/handlers/auth_enhanced/types.rs:81` and `server-api/handlers/auth_enhanced/sessions.rs:17`
- `AuditLogsResponse` in both `server-api/types/response.rs:1252` and `adapteros-ui/api/client.rs:2026`

## Proposed Canonical Shape

### Primary: Offset-Based Pagination

```rust
/// Canonical paginated list response for offset-based pagination.
/// Use when total count is cheap to compute and page-based navigation is needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PaginatedList<T> {
    /// API schema version for forward compatibility
    #[serde(default = "schema_version")]
    pub schema_version: String,

    /// The items in this page
    pub items: Vec<T>,

    /// Total number of items across all pages
    pub total: u64,

    /// Current page number (1-indexed)
    pub page: u32,

    /// Items per page
    pub limit: u32,

    /// Total number of pages
    pub pages: u32,
}
```

### Secondary: Cursor-Based Pagination

```rust
/// Canonical paginated list response for cursor-based pagination.
/// Use for large datasets, real-time data, or when total count is expensive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CursorPaginatedList<T> {
    /// API schema version for forward compatibility
    #[serde(default = "schema_version")]
    pub schema_version: String,

    /// The items in this page
    pub items: Vec<T>,

    /// Cursor for the next page (None if no more results)
    pub next_cursor: Option<String>,

    /// Whether more results are available
    pub has_more: bool,

    /// Approximate total count (optional, may be expensive to compute)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_hint: Option<u64>,
}
```

### Tertiary: Simple List (No Pagination)

```rust
/// Simple list response for small, bounded collections.
/// Use only when the list is guaranteed to be small (< 100 items).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SimpleList<T> {
    /// API schema version for forward compatibility
    #[serde(default = "schema_version")]
    pub schema_version: String,

    /// All items (no pagination)
    pub items: Vec<T>,

    /// Total count (same as items.len())
    pub total: u64,
}
```

## Migration Plan

### Phase 1: Define Canonical Types (Non-Breaking)

1. Add `PaginatedList<T>`, `CursorPaginatedList<T>`, and `SimpleList<T>` to `adapteros-api-types/src/lib.rs`
2. Add helper constructors and `From` implementations
3. Add documentation and examples

### Phase 2: New Endpoints Use Canonical Types

1. All new list endpoints must use one of the canonical types
2. Update API design guidelines

### Phase 3: Gradual Migration (Breaking)

For each existing type, evaluate:
- **Can be deprecated**: Add `#[deprecated]` and type alias to canonical type
- **Must remain for compatibility**: Keep but implement `Into<PaginatedList<_>>`

Priority order for migration:
1. Internal-only endpoints (lowest risk)
2. Server-to-worker communication
3. Public API endpoints (highest risk, needs versioning)

### Phase 4: Client Updates

1. Update `adapteros-ui` API client to use canonical types
2. Update any external client SDKs
3. Document migration for API consumers

## Decision Points for Design Review

1. **Data field naming**: Should we use `items` (generic) or domain-specific names (`adapters`, `jobs`)?
   - Recommendation: `items` for consistency, TypeScript/JSON clients don't benefit from domain names

2. **Total count type**: `u64` vs `usize` vs `i64`?
   - Recommendation: `u64` for wire format (SQLite returns i64, but negative counts are invalid)

3. **1-indexed vs 0-indexed pages**?
   - Recommendation: 1-indexed (matches user expectations, `page=1` is first page)

4. **Cursor format**: Opaque string vs structured (e.g., `id:timestamp`)?
   - Recommendation: Opaque string (allows backend flexibility)

5. **API versioning strategy**: How to handle breaking changes to existing endpoints?
   - Option A: New endpoint paths (e.g., `/v2/adapters`)
   - Option B: Accept header versioning
   - Option C: Deprecation period with dual format support

6. **Schema version field**: Keep in all types or remove?
   - Recommendation: Keep for forward compatibility detection

## Affected Endpoints

| Endpoint | Current Type | Target Type |
|----------|--------------|-------------|
| `GET /v1/adapters` | Domain-specific | `PaginatedList<AdapterResponse>` |
| `GET /v1/training/jobs` | `TrainingJobListResponse` | `PaginatedList<TrainingJobResponse>` |
| `GET /v1/datasets` | `DatasetListResponse` | `PaginatedList<DatasetResponse>` |
| `GET /v1/models` | `ModelListResponse` | `PaginatedList<ModelResponse>` |
| `GET /v1/admin/users` | `ListUsersResponse` | `PaginatedList<UserResponse>` |
| `GET /v1/audit/logs` | `AuditLogsResponse` | `PaginatedList<AuditLogResponse>` |
| `GET /v1/auth/sessions` | `SessionsResponse` | `SimpleList<SessionInfo>` |
| `GET /v1/diagnostics/runs` | `ListDiagRunsResponse` | `CursorPaginatedList<DiagRunResponse>` |
| `GET /v1/diagnostics/events` | `ListDiagEventsResponse` | `CursorPaginatedList<DiagEventResponse>` |
| `GET /v1/telemetry/errors` | `ClientErrorsListResponse` | `PaginatedList<ClientErrorItem>` |

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Breaking existing clients | Use deprecation warnings, dual-support period |
| Large migration scope | Phased approach, prioritize high-value endpoints |
| Performance regression | Benchmark before/after, especially cursor pagination |
| Type erasure issues | Use concrete type aliases where generics cause problems |

## Success Criteria

1. All new list endpoints use canonical types
2. 80% of existing list endpoints migrated within 6 months
3. No breaking changes to public API without version bump
4. UI client updated with no user-facing regressions
5. API documentation reflects canonical patterns

## Related Work

- API versioning strategy (separate RFC needed)
- OpenAPI schema generation improvements
- Client SDK type generation

## Appendix: Full Type Inventory

### adapteros-api-types/src/

| File | Type | Style |
|------|------|-------|
| lib.rs:340 | `PaginatedResponse<T>` | Offset |
| training.rs:1333 | `TrainingMetricsListResponse` | None (not paginated) |
| training.rs:1447 | `DatasetListResponse` | Total only |
| training.rs:1601 | `TrainingJobListResponse` | Offset (page_size variant) |
| telemetry.rs:133 | `ClientErrorsListResponse` | Total only |
| telemetry.rs:277 | `ErrorAlertRulesListResponse` | Total only |
| telemetry.rs:302 | `ErrorAlertHistoryListResponse` | Total only |
| diagnostics.rs:83 | `ListDiagRunsResponse` | Cursor |
| diagnostics.rs:100 | `ListDiagEventsResponse` | Cursor |
| models.rs:78 | `ModelListResponse<T>` | Total only |
| review.rs:259 | `ListPausedResponse` | Total only |
| prefix_templates.rs:133 | `ListPrefixTemplatesResponse` | Total only |
| auth.rs:57 | `TenantListResponse` | None |
| api_keys.rs:42 | `ApiKeyListResponse` | None |
| admin.rs:32 | `ListUsersResponse` | Offset |

### adapteros-server-api/src/

| File | Type | Style |
|------|------|-------|
| types/response.rs:755 | `RoutingDecisionsResponse` | None |
| types/response.rs:784 | `AuditsResponse` | None |
| types/response.rs:1155 | `CategoryPoliciesResponse` | None |
| types/response.rs:1252 | `AuditLogsResponse` | Offset |
| handlers/auth_enhanced/sessions.rs:17 | `SessionsResponse` | None |
| handlers/datasets/files.rs | `ListUploadSessionsResponse` | (to verify) |

### adapteros-api/src/

| File | Type | Style |
|------|------|-------|
| types.rs:49 | `ListAdaptersResponse` | Total only |
