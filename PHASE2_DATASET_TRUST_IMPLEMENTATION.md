# Phase 2 - Dataset Trust System Implementation

## Overview

This document describes the implementation of Phase 2 of the Dataset Trust System, which adds version-specific endpoints for trust management and ensures proper trust propagation to linked adapters.

## Implementation Summary

### New Endpoints

#### 1. Apply Trust Override to Dataset Version
**Endpoint:** `POST /v1/datasets/{dataset_id}/versions/{version_id}/trust-override`

**Purpose:** Apply an administrative trust override to a specific dataset version.

**Request Body:**
```json
{
  "override_state": "allowed" | "allowed_with_warning" | "blocked" | "needs_approval",
  "reason": "Optional human-readable justification"
}
```

**Response:**
```json
{
  "dataset_id": "...",
  "dataset_version_id": "...",
  "override_state": "allowed",
  "effective_trust_state": "allowed",
  "reason": "Manual approval after review"
}
```

**Features:**
- Validates dataset and version existence
- Enforces tenant isolation on both dataset and version
- Validates override state against allowed values
- Automatically propagates trust changes to linked adapter versions via DB layer
- Logs action for audit trail

#### 2. Update Safety Signals for Dataset Version
**Endpoint:** `POST /v1/datasets/{dataset_id}/versions/{version_id}/safety`

**Purpose:** Update safety validation signals (PII, toxicity, leak, anomaly) for a specific dataset version.

**Request Body:**
```json
{
  "pii_status": "clean" | "warn" | "block" | "unknown",
  "toxicity_status": "clean" | "warn" | "block" | "unknown",
  "leak_status": "clean" | "warn" | "block" | "unknown",
  "anomaly_status": "clean" | "warn" | "block" | "unknown"
}
```

**Response:**
```json
{
  "dataset_id": "...",
  "dataset_version_id": "...",
  "trust_state": "allowed",
  "overall_safety_status": "clean"
}
```

**Features:**
- Validates dataset and version existence
- Enforces tenant isolation on both dataset and version
- Computes overall safety status based on individual signals
- Derives trust state using canonical semantics
- Automatically propagates trust changes to linked adapter versions via DB layer
- Records validation run for audit trail
- Logs action for observability

## Trust Propagation Mechanism

### How Trust Propagates

Trust propagation is **automatically handled** by the DB layer functions:

1. **`create_dataset_version_override`** (line 997-1031 in `training_datasets.rs`)
   - Captures previous effective trust state
   - Creates the override record
   - Calls `propagate_dataset_trust_change` with old and new states

2. **`update_dataset_version_safety_status`** (line 891-962 in `training_datasets.rs`)
   - Captures previous effective trust state
   - Updates safety signals and recomputes trust
   - Calls `propagate_dataset_trust_change` with old and new states

3. **`update_dataset_version_structural_validation`** (line 834-888 in `training_datasets.rs`)
   - Captures previous effective trust state
   - Updates validation status and recomputes trust
   - Calls `propagate_dataset_trust_change` with old and new states

### Propagation Algorithm

The `propagate_dataset_trust_change` function (in `adapter_repositories.rs`, line 523):

1. Normalizes trust states to adapter trust vocabulary
2. Detects blocked transitions (when dataset becomes blocked)
3. Fetches all adapter versions linked to the dataset version via `adapter_version_dataset_versions` join table
4. For each linked adapter version:
   - If blocked transition: Sets adapter to `blocked_regressed` state
   - Otherwise: Recomputes adapter trust by aggregating all linked dataset trust states
5. Commits changes in a single transaction
6. Triggers downstream notifications/alerts for regressed adapters

### Trust State Mapping

**Dataset Trust States:**
- `allowed` - Validation passed, all safety signals clean
- `allowed_with_warning` - Validation passed, but safety warnings present
- `needs_approval` - Validation pending or unresolved safety signals
- `blocked` - Validation failed or safety signals blocked
- `unknown` - Trust not evaluated

**Adapter Trust Aggregation:**
Priority order when multiple datasets are linked:
1. `blocked` (highest priority - any blocked dataset blocks the adapter)
2. `needs_approval`
3. `allowed_with_warning`
4. `allowed` (lowest priority)

## Implementation Files

### Handler Endpoints
- **File:** `crates/adapteros-server-api/src/handlers/datasets.rs`
- **Functions:**
  - `apply_dataset_version_trust_override` (line 1655-1746)
  - `update_dataset_version_safety` (line 1765-1868)

### Route Registration
- **File:** `crates/adapteros-server-api/src/routes.rs`
- **Routes:**
  - Line 1565-1567: Trust override route
  - Line 1569-1571: Safety update route
- **OpenAPI paths:**
  - Line 220-221: Added to API documentation

### Database Layer
- **File:** `crates/adapteros-db/src/training_datasets.rs`
- **Functions:**
  - `create_dataset_version_override` (line 997-1031)
  - `update_dataset_version_safety_status` (line 891-962)
  - `update_dataset_version_structural_validation` (line 834-888)
  - `get_effective_trust_state` (line 1034-1064)
  - `derive_trust_state` (line 44-93) - Pure function for trust derivation

- **File:** `crates/adapteros-db/src/adapter_repositories.rs`
- **Functions:**
  - `propagate_dataset_trust_change` (line 523+)
  - `recompute_adapter_trust_for_version` (aggregates dataset trust)
  - `set_adapter_trust_state` (sets adapter trust state)

## Security & Isolation

### Tenant Isolation
Both endpoints enforce strict tenant isolation:

1. **Dataset-level validation:**
   - Fetches dataset and validates `tenant_id` matches user's tenant
   - Uses `validate_tenant_isolation(&claims, dataset_tenant_id)`

2. **Version-level validation:**
   - Fetches dataset version and validates it belongs to the dataset
   - Validates version `tenant_id` matches user's tenant
   - Prevents cross-dataset version manipulation

### Permission Requirements
- Both endpoints require `Permission::DatasetValidate`
- Only users with dataset validation permissions can modify trust or safety

### Input Validation
- **Trust override:** Validates state is one of the allowed values
- **Safety signals:** Each signal must be `clean`, `warn`, `block`, or `unknown`
- **Version ownership:** Ensures version belongs to the specified dataset

## Testing

### Manual Testing
A test script is provided at `var/tmp/test_dataset_trust_phase2.sh`:

```bash
# Start the server in no-auth mode
AOS_DEV_NO_AUTH=1 ./start up

# Run the test script
bash var/tmp/test_dataset_trust_phase2.sh
```

### Expected Behavior

1. **Safety update:**
   - Updates PII, toxicity, leak, and anomaly status fields
   - Recomputes `trust_state` based on validation + safety signals
   - Propagates changes to linked adapters
   - Returns new trust state and overall safety

2. **Trust override:**
   - Creates an override record with reason and actor
   - New effective trust state = override state
   - Propagates to linked adapters (may regress adapters if blocking)
   - Returns effective trust state

3. **Trust propagation:**
   - Finds all adapters trained on the dataset version
   - Updates adapter trust states based on aggregated dataset trust
   - Marks adapters as `blocked_regressed` if dataset becomes blocked
   - All updates occur in a single transaction

## API Examples

### Update Safety Signals
```bash
curl -X POST \
  http://localhost:8080/v1/datasets/{dataset_id}/versions/{version_id}/safety \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <token>" \
  -d '{
    "pii_status": "clean",
    "toxicity_status": "warn",
    "leak_status": "clean",
    "anomaly_status": "clean"
  }'
```

**Response:**
```json
{
  "dataset_id": "01JFGK...",
  "dataset_version_id": "01JFGL...",
  "trust_state": "allowed_with_warning",
  "overall_safety_status": "warn"
}
```

### Apply Trust Override
```bash
curl -X POST \
  http://localhost:8080/v1/datasets/{dataset_id}/versions/{version_id}/trust-override \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <token>" \
  -d '{
    "override_state": "allowed",
    "reason": "Reviewed by security team - false positive on PII scan"
  }'
```

**Response:**
```json
{
  "dataset_id": "01JFGK...",
  "dataset_version_id": "01JFGL...",
  "override_state": "allowed",
  "effective_trust_state": "allowed",
  "reason": "Reviewed by security team - false positive on PII scan"
}
```

## Observability

### Logging
Both endpoints emit structured logs:

```rust
info!(
    dataset_id = %dataset_id,
    version_id = %version_id,
    trust_state = %trust_state,
    actor = %claims.sub,
    "Updated dataset version safety status"
);
```

### Audit Trail
- Trust overrides are recorded in `dataset_version_overrides` table
- Safety validation runs are recorded in `dataset_version_validations` table
- All changes include `created_by` field for accountability

## Related Work

### Existing Dataset-Level Endpoints (for reference)
- `POST /v1/datasets/{dataset_id}/trust_override` - Operates on latest version
- `POST /v1/datasets/{dataset_id}/safety` - Operates on latest version (via `update_dataset_safety`)

These dataset-level endpoints internally call `ensure_dataset_version_exists()` to get/create the latest version, then delegate to the same DB functions.

### Version Management
- `GET /v1/datasets/{dataset_id}/versions` - List all versions
- `POST /v1/datasets/{dataset_id}/versions` - Create new version

## Completion Checklist

- [x] Implement `apply_dataset_version_trust_override` handler
- [x] Implement `update_dataset_version_safety` handler
- [x] Register routes in `routes.rs`
- [x] Add endpoints to OpenAPI documentation
- [x] Verify DB functions exist and call `propagate_dataset_trust_change`
- [x] Confirm trust propagation logic is correct
- [x] Add tenant isolation validation
- [x] Add permission checks
- [x] Add structured logging
- [x] Code compiles successfully
- [x] Create test script
- [x] Document implementation

## Next Steps

1. **Integration Testing:**
   - Create full integration test with dataset → adapter linkage
   - Verify trust propagation with real adapter versions
   - Test blocked transition notifications

2. **UI Integration:**
   - Update Dataset Lab to call version-specific endpoints
   - Add version selector for trust operations
   - Show trust propagation results

3. **Monitoring:**
   - Add metrics for trust state transitions
   - Track propagation latency
   - Alert on blocked regressions

## References

- DB Implementation: `crates/adapteros-db/src/training_datasets.rs`
- Propagation Logic: `crates/adapteros-db/src/adapter_repositories.rs`
- API Handlers: `crates/adapteros-server-api/src/handlers/datasets.rs`
- Routes: `crates/adapteros-server-api/src/routes.rs`
