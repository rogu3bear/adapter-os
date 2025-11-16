# Formal Definitions and Patent Citations

This document maintains citations linking patent claims to their current implementations in the AdapterOS codebase.

---

## Adapter Taxonomy System

**Feature:** Hierarchical naming system with semantic validation and lineage tracking

**Patent Gate:** Adversarial test coverage for security-critical naming validation

### Core Implementation

1. **AdapterName Type**
   - **Location:** `crates/adapteros-core/src/naming.rs` (lines 1-450)
   - **Validation:** Format `{tenant}/{domain}/{purpose}/{revision}` with regex enforcement
   - **Security:** Reserved namespace blocking, tenant isolation, SQL injection prevention

2. **StackName Type**
   - **Location:** `crates/adapteros-core/src/naming.rs` (lines 451-620)
   - **Validation:** Format `stack.{namespace}[.{identifier}]` with max length enforcement
   - **Security:** Profanity filtering, path traversal prevention

3. **NamingPolicy**
   - **Location:** `crates/adapteros-policy/src/packs/naming_policy.rs` (lines 1-450)
   - **Policy ID:** 23 (23rd canonical policy pack)
   - **Enforcement:** Tenant isolation, revision monotonicity, reserved namespace protection

### Security Testing

**Adversarial Coverage:** `tests/fault_injection_harness.rs` (lines 723-1014)

1. **Malformed Input Resistance** (lines 727-752)
   - Empty strings, missing components, invalid characters
   - **Citation:** test_adapter_name_malformed_inputs

2. **Reserved Namespace Protection** (lines 754-774)
   - Blocks system, admin, root, global, default, test tenants
   - **Citation:** test_adapter_name_reserved_namespaces

3. **Tenant Isolation Enforcement** (lines 776-796)
   - Prevents cross-tenant adapter creation
   - **Citation:** test_adapter_name_tenant_isolation_violation

4. **Revision Monotonicity** (lines 798-817)
   - Prevents large revision gaps (>5)
   - **Citation:** test_adapter_name_revision_monotonicity_violation

5. **Injection Attack Prevention** (lines 860-892)
   - SQL injection resistance
   - Path traversal prevention
   - **Citations:**
     - test_adapter_name_sql_injection_attempt
     - test_adapter_name_path_traversal_attempt

6. **Input Validation Edge Cases** (lines 894-978)
   - Consecutive hyphens rejection
   - Leading/trailing hyphens rejection
   - Unicode character rejection
   - Max length enforcement (200 chars for adapters, 100 for stacks)
   - **Citations:**
     - test_adapter_name_consecutive_hyphens
     - test_adapter_name_leading_trailing_hyphens
     - test_adapter_name_unicode_rejection
     - test_adapter_name_max_length_violation

### Database Schema

**Migration:** `migrations/0061_adapter_taxonomy.sql`

1. **Semantic Name Columns**
   - adapter_name (UNIQUE), tenant_namespace, domain, purpose, revision
   - **Constraints:** Unique(tenant_id, domain, purpose, revision)

2. **Lineage Tracking**
   - parent_id (REFERENCES adapters.id), fork_type, fork_reason
   - **Validation:** Triggers prevent circular dependencies

3. **Validation Triggers**
   - validate_adapter_name_format (BEFORE INSERT)
   - validate_parent_exists (BEFORE INSERT)
   - **Security:** Format validation at database layer

### REST API

**Endpoints:** `crates/adapteros-server-api/src/handlers.rs`

1. **POST /v1/adapters/validate-name**
   - Real-time name validation before registration
   - Returns parsed components and violation details

2. **POST /v1/stacks/validate-name**
   - Stack name validation endpoint
   - Checks namespace and identifier validity

3. **GET /v1/adapters/next-revision/{tenant}/{domain}/{purpose}**
   - Auto-increments revision numbers
   - Enforces monotonicity constraints

### UI Integration

**Component:** `ui/src/components/Adapters.tsx` (lines 88-131, 515-560)

1. **Semantic Name Display**
   - Color-coded components: blue (tenant), green (domain), standard (purpose)
   - Revision badges, fork type indicators, lineage markers

2. **Backward Compatibility**
   - Falls back to legacy names when semantic fields absent
   - Gradual migration support

### Fuzz Testing

**Targets:** `fuzz/fuzz_targets/`

1. **adapter_name_parse.rs**
   - Fuzzes AdapterName::parse with arbitrary byte sequences
   - Tests component extraction, revision parsing, lineage checking

2. **stack_name_parse.rs**
   - Fuzzes StackName::parse with arbitrary inputs
   - Tests namespace/identifier extraction, validation

### Test Coverage Summary

- **Unit Tests:** 24 passing (15 core + 9 policy)
- **Adversarial Tests:** 16 attack resistance tests
- **Integration Tests:** End-to-end validation in `tests/adapter_taxonomy_integration.rs`
- **Fuzz Tests:** 2 targets for robustness verification

---

## Lineage and Fork Semantics

**Feature:** Parent-child relationships with fork type tracking

### Implementation

1. **Fork Types**
   - **Extension:** Incremental improvement, same lineage
   - **Independent:** Divergent use case, breaks compatibility

2. **Lineage Rules**
   - Parent must exist before child registration
   - Parent must be in same tenant namespace
   - Circular dependencies forbidden

3. **Citations**
   - **Core Type:** `crates/adapteros-core/src/naming.rs` (ForkType enum)
   - **Policy Enforcement:** `crates/adapteros-policy/src/packs/naming_policy.rs`
   - **Database Schema:** `migrations/0061_adapter_taxonomy.sql` (parent_id, fork_type columns)

---

## Policy Engine Threshold Configuration

**Feature:** Config-driven policy thresholds ensuring documentation and enforcement cannot diverge

**Patent Gate:** Adversarial test coverage for floating-point edge cases and boundary conditions

### Core Implementation

1. **PolicyEngine Threshold Methods**
   - **Location:** `crates/adapteros-policy/src/lib.rs` (lines 107-163)
   - **Methods:** `check_resource_limits`, `check_system_thresholds`, `check_memory_headroom`, `should_open_circuit_breaker`
   - **Configuration Source:** `adapteros_manifest::Policies` (PerformancePolicy, MemoryPolicy)

2. **PerformancePolicy Struct**
   - **Location:** `crates/adapteros-manifest/src/lib.rs` (lines 432-440)
   - **Fields:**
     - `max_tokens: usize` - Maximum tokens per request (default: 1000)
     - `cpu_threshold_pct: f32` - CPU usage threshold (default: 90.0)
     - `memory_threshold_pct: f32` - Memory usage threshold (default: 95.0)
     - `circuit_breaker_threshold: usize` - Failure count threshold (default: 5)

3. **MemoryPolicy Struct**
   - **Location:** `crates/adapteros-manifest/src/lib.rs` (lines 443-447)
   - **Fields:**
     - `min_headroom_pct: u8` - Minimum memory headroom (default: 15)

### Security Testing

**Adversarial Coverage:** `tests/fault_injection_harness.rs` (lines 1016-1200)

1. **Floating-Point Edge Cases** (lines 1020-1055)
   - NaN input handling
   - Infinity (positive/negative) input handling
   - Subnormal values
   - **Citation:** test_policy_thresholds_nan_handling, test_policy_thresholds_infinity_handling

2. **Integer Overflow/Underflow** (lines 1057-1087)
   - usize::MAX for max_tokens
   - Zero threshold edge cases
   - **Citation:** test_policy_thresholds_integer_overflow, test_policy_thresholds_zero_thresholds

3. **Boundary Value Testing** (lines 1089-1120)
   - Exact threshold boundaries (at, above, below)
   - Negative percentage values
   - Percentage values exceeding 100.0
   - **Citation:** test_policy_thresholds_boundary_values, test_policy_thresholds_negative_percentages

4. **Error Message Validation** (lines 1122-1145)
   - Ensures error messages include actual threshold values
   - Prevents silent failures
   - **Citation:** test_policy_thresholds_error_message_accuracy

### Integration Tests

**Location:** `crates/adapteros-policy/tests/policy_engine_thresholds.rs` (lines 1-317)

1. **Config-Driven Enforcement** (lines 17-121)
   - Verifies thresholds come from config, not hard-coded
   - Tests runtime config changes

2. **Documentation Contract** (lines 268-316)
   - Enforces contract between manifest defaults and PolicyEngine
   - Prevents documentation drift

---

**Last Updated:** 2025-11-16
**Maintainer:** AdapterOS Core Team
