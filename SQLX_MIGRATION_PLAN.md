# SQLx Query Migration Plan

## Objective
Migrate ALL remaining direct `sqlx::query` calls in handlers to Db trait methods.

## Current Status

### ✅ Completed Migrations
- Adapters handlers
- Training handlers (partial)
- Collections handlers
- Documents handlers (partial)
- Auth handlers (partial)

### 📋 Remaining Migrations

#### 1. Federation Module (`crates/adapteros-db/src/federation.rs`)
**STATUS: CREATED - NEEDS FIX IN lib.rs (duplicate import)**

Methods created:
- `get_federation_host_count()` - Count distinct federation hosts
- `get_active_quarantine_details()` - Fetch unreleased quarantine details
- `get_active_quarantine_with_cooldown()` - Quarantine with cooldown data
- `update_quarantine_last_attempt()` - Update cooldown timestamp
- `record_quarantine_release_attempt()` - Log release attempt
- `record_quarantine_release_execution()` - Mark release as executed
- `release_active_quarantines()` - Mark all quarantines as released

Handlers to update:
- `handlers/federation.rs` - Lines 257, 274, 309, 337, 359, 380, 400

#### 2. Memory Module (`crates/adapteros-db/src/memory.rs`)
**STATUS: TODO**

Queries found in `handlers/memory_detail.rs`:
- Line 264: Get adapters with memory info (JOIN with pinned_adapters)
- Line 369: Get eviction candidates

Methods needed:
```rust
async fn get_adapters_memory_info(&self) -> Result<Vec<AdapterMemoryRecord>>
async fn get_eviction_candidates(&self) -> Result<Vec<CandidateRecord>>
```

#### 3. Nodes Module (`crates/adapteros-db/src/nodes.rs`)
**STATUS: PARTIAL - Already exists, needs extension**

Queries found in `handlers/node_detail.rs`:
- Line 95: Get node record by ID
- Line 126: Get adapters loaded on node
- Line 238: Check if node is primary

Queries found in `handlers/nodes.rs`:
- Line 332: Get node list

Queries found in `handlers/worker_detail.rs`:
- Line 105: Get worker record
- Line 203: Check if worker is training
- Line 238: Get requests processed count
- Line 247: Get errors count
- Line 256: Get average latency
- Line 307: Get training tasks

Methods needed:
```rust
async fn get_node_by_id(&self, node_id: &str) -> Result<Option<NodeRecord>>
async fn get_adapters_loaded_on_node(&self, node_id: &str) -> Result<Vec<String>>
async fn is_node_primary(&self, node_id: &str) -> Result<bool>
async fn get_nodes_list(&self) -> Result<Vec<(String,)>>
async fn get_worker_by_id(&self, worker_id: &str) -> Result<Option<WorkerRecord>>
async fn is_worker_training(&self, worker_id: &str) -> Result<bool>
async fn get_worker_requests_processed(&self, worker_id: &str) -> Result<i64>
async fn get_worker_errors_count(&self, worker_id: &str) -> Result<i64>
async fn get_worker_avg_latency(&self, worker_id: &str) -> Result<Option<f64>>
async fn get_worker_training_tasks(&self, worker_id: &str) -> Result<Vec<TaskRecord>>
```

#### 4. Metrics Module (`crates/adapteros-db/src/metrics.rs`)
**STATUS: TODO**

Queries found in `handlers/metrics_time_series.rs`:
- Line 142: Get metric data points
- Line 232: Insert metric data point

Methods needed:
```rust
async fn get_metrics_time_series(&self, metric_name: &str, start: i64, end: i64) -> Result<Vec<MetricDataPoint>>
async fn insert_metric_data_point(&self, metric_name: &str, value: f64, timestamp: i64) -> Result<()>
```

#### 5. Promotion Module (`crates/adapteros-db/src/promotion.rs`)
**STATUS: TODO**

Queries found in `handlers/promotion.rs`:
- Line 185: Insert promotion request
- Line 283: Get promotion request
- Line 315: Get gate rows
- Line 353: Get approval rows
- Line 475: Get promotion request (duplicate)
- Line 554: Insert approval
- Line 578: Update gate status
- Line 664: Get stage row
- Line 724: Update stage active golden run
- Line 739: Update request status
- Line 790: Get promotion request (triplicate)
- Line 833: Get gate rows (duplicate)
- Line 937: Insert gate check
- Line 1074: Get target stage
- Line 1083: Get active golden run
- Line 1091: Update promotion request status
- Line 1106: Insert promotion history
- Line 1116: Update golden run stage

Methods needed:
```rust
async fn create_promotion_request(&self, ...) -> Result<String>
async fn get_promotion_request(&self, request_id: &str) -> Result<Option<PromotionRequest>>
async fn get_promotion_gates(&self, request_id: &str) -> Result<Vec<GateRow>>
async fn get_promotion_approvals(&self, request_id: &str) -> Result<Vec<ApprovalRow>>
async fn insert_promotion_approval(&self, ...) -> Result<()>
async fn update_gate_status(&self, gate_id: &str, status: &str) -> Result<()>
async fn get_stage_by_name(&self, stage_name: &str) -> Result<Option<StageRow>>
async fn update_stage_active_golden_run(&self, stage_name: &str, golden_run_id: &str) -> Result<()>
async fn update_promotion_request_status(&self, request_id: &str, status: &str) -> Result<()>
async fn insert_gate_check(&self, ...) -> Result<()>
async fn get_promotion_target_stage(&self, request_id: &str) -> Result<Option<String>>
async fn get_stage_active_golden_run(&self, stage_name: &str) -> Result<Option<String>>
async fn insert_promotion_history(&self, ...) -> Result<()>
async fn update_golden_run_stage(&self, golden_run_id: &str, stage: &str) -> Result<()>
```

#### 6. System State Module (`crates/adapteros-db/src/system_state.rs`)
**STATUS: TODO**

Queries found in `handlers/system_state.rs`:
- Line 185: Get tenant by ID
- Line 191: Get all tenants
- Line 216: Get stack by tenant
- Line 225: Get all stacks
- Line 236: Get adapters by tenant
- Line 254: Get all adapters

Methods needed:
```rust
async fn get_tenant_by_id_simple(&self, tenant_id: &str) -> Result<Option<TenantRecord>>
async fn get_all_tenants_simple(&self) -> Result<Vec<TenantRecord>>
async fn get_stack_by_tenant(&self, tenant_id: &str) -> Result<Vec<StackRecord>>
async fn get_all_stacks_simple(&self) -> Result<Vec<StackRecord>>
async fn get_adapters_by_tenant_simple(&self, tenant_id: &str) -> Result<Vec<AdapterRecord>>
async fn get_all_adapters_simple(&self) -> Result<Vec<AdapterRecord>>
```

#### 7. Diagnostics Module (`crates/adapteros-db/src/diagnostics.rs`)
**STATUS: TODO**

Queries found in `handlers/diagnostics.rs`:
- Line 53: Check database health
- Line 138: Get quarantine records
- Line 213: Get active stacks

Methods needed:
```rust
async fn check_database_health(&self) -> Result<()>
async fn get_quarantine_records(&self) -> Result<Vec<QuarantineRow>>
async fn get_active_stacks(&self) -> Result<Vec<StackRow>>
```

#### 8. Capacity Module (`crates/adapteros-db/src/capacity.rs`)
**STATUS: TODO - Consider adding to existing modules**

Queries found in `handlers/capacity.rs`:
- Line 105: Count loaded models
- Line 112: Count loaded adapters
- Line 122: Count active requests

Methods needed (likely in adapters.rs):
```rust
async fn count_loaded_models(&self) -> Result<i64>
async fn count_loaded_adapters(&self) -> Result<i64>
async fn count_active_requests(&self) -> Result<i64>
```

#### 9. Journeys Module (`crates/adapteros-db/src/journeys.rs` or inline)
**STATUS: TODO - sqlx! macro usage**

Queries found in `handlers/journeys.rs`:
- Line 55: Get tenant ITAR flag (sqlx! macro)
- Line 91: Get journey steps (sqlx! macro)
- Line 149: Get promotions (sqlx! macro)
- Line 197: Get metrics (sqlx! macro)

Note: Uses `sqlx!` macro which provides compile-time checking. May need different approach.

#### 10. Documents Handler Remaining
**STATUS: TODO**

Queries found in `handlers/documents.rs`:
- Line 653: Update document collection
- Line 712: Update page count

Methods needed (in documents.rs):
```rust
async fn update_document_collection(&self, doc_id: &str, collection_id: Option<&str>) -> Result<()>
async fn update_document_page_count(&self, doc_id: &str, page_count: i64) -> Result<()>
```

#### 11. Adapter Stacks Handler
**STATUS: TODO**

Queries found in `handlers/adapter_stacks.rs`:
- Line 118: Count current adapters loaded

Methods needed (in adapters.rs or stacks.rs):
```rust
async fn count_current_adapters_loaded(&self) -> Result<i64>
```

#### 12. Plans Handler
**STATUS: TODO**

Queries found in `handlers/plans.rs`:
- Lines 155, 171: Get plan metadata
- Line 231: Update plan
- Lines 246, 252: More plan metadata

Methods needed:
```rust
async fn get_plan_metadata(&self, plan_id: &str, key: &str) -> Result<Option<String>>
async fn update_plan(&self, plan_id: &str, ...) -> Result<()>
```

#### 13. Auth Enhanced Handler
**STATUS: TODO**

Queries found in `handlers/auth_enhanced.rs`:
- Line 659: Record login attempt
- Line 688: Record MFA verification

Methods needed (in auth_sessions.rs or users.rs):
```rust
async fn record_login_attempt(&self, user_id: &str, success: bool, ip: &str) -> Result<()>
async fn record_mfa_verification(&self, user_id: &str, success: bool) -> Result<()>
```

#### 14. Training Handler
**STATUS: PARTIAL**

Queries found in `handlers/training.rs`:
- Line 242: Count running training jobs
- Line 409: Check dataset validation status

Methods needed (in training_jobs.rs or training_datasets.rs):
```rust
async fn count_running_training_jobs(&self) -> Result<i64>
async fn get_dataset_validation_status(&self, dataset_id: &str) -> Result<Option<String>>
```

#### 15. Policies Handler
**STATUS: TODO**

Queries found in `handlers/policies.rs`:
- Line 212: Get signing key
- Line 235: Insert policy

Methods needed (in policies.rs):
```rust
async fn get_policy_signing_key(&self) -> Result<Option<String>>
async fn insert_policy(&self, ...) -> Result<()>
```

#### 16. handlers.rs Main File
**STATUS: TODO - Large number of queries**

This file has ~50+ sqlx::query calls. Major categories:
- CP pointers (lines 2112, 2128, 2147, etc.)
- Metrics aggregation (lines 6396, 6403, 6413, 7376, 7383, 7393)
- Contacts (lines 7765, 7827, 7882, 7927, 7979, 7999)
- Generic queries (lines 668, 1075, 1114, 1145, 1173, 1205, 1235, etc.)
- Audit queries (lines 7052, 7094)
- Workers/signatures (lines 9583, 9646, 9691, 9755, 9778, 9800)

## Migration Strategy

### Phase 1: Low-Hanging Fruit (Federation ✅ + Simple Modules)
1. Fix federation.rs duplicate in lib.rs ✅
2. Update federation handler ✅
3. Create memory.rs module
4. Create diagnostics.rs module
5. Create capacity methods in adapters.rs

### Phase 2: Complex Modules
1. Create promotion.rs module (many methods)
2. Extend nodes.rs module (worker queries)
3. Create system_state.rs module
4. Create metrics.rs module

### Phase 3: Miscellaneous
1. Add document methods to documents.rs
2. Add auth methods to auth_sessions.rs
3. Add training methods to training_jobs.rs
4. Add policy methods to policies.rs

### Phase 4: Main handlers.rs File
1. Create cp_pointers.rs methods
2. Extend contacts.rs methods
3. Create audit aggregate methods
4. Migrate remaining queries

### Phase 5: Verification
1. Search for all `sqlx::query` in handlers directory
2. Verify count is ZERO
3. Run tests
4. Document migration

## Verification Command

```bash
# After migration, this should return NO RESULTS:
grep -r "sqlx::query" crates/adapteros-server-api/src/handlers/
```

## Notes

- Some queries use `sqlx!` macro for compile-time checking - these may need special handling
- Focus on MOVING queries to Db methods, not rewriting them
- Maintain transaction support where applicable
- Keep error messages descriptive
