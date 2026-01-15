# adapterOS Training Datasets

**Purpose:** Organized test data for validation, contract testing, and behavioral verification.

## Directory Structure

```
tests/training/datasets/
├── cli_contract/       # CLI/API contract validation (adapter endpoints)
├── routing/            # Router decision validation data
├── metrics/            # Health checks and system metrics
├── stacks/             # Adapter stack configurations
├── behaviors/          # Behavioral test scenarios
├── replay/             # Deterministic replay test data
├── determinism/        # Determinism verification datasets
├── code_ingest/        # Code ingestion test samples
└── docs_derived/       # Documentation-derived training data
```

## Dataset Categories

### cli_contract/
**Type:** API Contract Validation
**Format:** JSON mock responses
**Purpose:** Validate REST API response schemas

**Files:**
- `adapters_list.json` - /v1/adapters/list endpoint
- `adapter_lineage.json` - /v1/adapters/{id}/lineage endpoint

**Usage:** Reference data for API contract tests in `crates/adapteros-server-api/tests/api_contracts.rs`

---

### routing/
**Type:** Router Decision Validation
**Format:** JSON routing decisions
**Purpose:** Validate K-sparse routing, Q15 quantization, entropy metrics

**Files:**
- `routing_decisions.json` - /v1/routing/decisions endpoint with candidates

**Usage:** Tests for router telemetry compliance

---

### metrics/
**Type:** Health Check & Metrics Validation
**Format:** JSON health responses
**Purpose:** Validate system health reporting and component status

**Files:**
- `healthz_basic.json` - Basic health status
- `healthz_all.json` - System-wide health (all components)
- `healthz_degraded.json` - Degraded state examples
- `healthz_router.json` - Component-specific details

**Usage:** Health endpoint contract validation

---

### stacks/
**Type:** Adapter Stack Configurations
**Status:** Planned
**Purpose:** Reusable adapter combinations for workflow testing

---

### behaviors/
**Type:** Behavioral Test Scenarios
**Status:** Planned
**Purpose:** End-to-end behavior validation scenarios

---

### replay/
**Type:** Deterministic Replay Data
**Status:** Planned
**Purpose:** Tick ledger replay verification

---

### determinism/
**Type:** Determinism Verification
**Status:** Planned
**Purpose:** HKDF seeding, randomness isolation tests

---

### code_ingest/
**Type:** Code Ingestion Samples
**Status:** Planned
**Purpose:** Document processing pipeline test inputs

---

### docs_derived/
**Type:** Documentation-Derived Training Data
**Status:** Planned
**Purpose:** Training examples generated from documentation

---

## Usage in Tests

### API Contract Tests
```rust
let json = include_str!("../../../tests/training/datasets/cli_contract/adapters_list.json");
let response: AdaptersListResponse = serde_json::from_str(json)?;
assert_eq!(response.adapters.len(), 3);
```

### Routing Decision Tests
```rust
let json = include_str!("../../../tests/training/datasets/routing/routing_decisions.json");
let response: RoutingDecisionsResponse = serde_json::from_str(json)?;
// Validate K-sparse selection, Q15 quantization, etc.
```

### Health Check Tests
```rust
let json = include_str!("../../../tests/training/datasets/metrics/healthz_all.json");
let response: SystemHealthResponse = serde_json::from_str(json)?;
assert_eq!(response.components.len(), 6);
```

## Maintenance

### Adding New Datasets

1. Choose appropriate category directory
2. Create JSON file with canonical format
3. Update category README with file description
4. Add test cases in corresponding test file
5. Update this master README

### Schema Evolution

When API schemas change:
1. Update reference JSON files in affected categories
2. Update test assertions
3. Document breaking changes
4. Maintain backward compatibility where possible

## References

- [API Contract Tests](../../crates/adapteros-server-api/tests/API_CONTRACT_TESTS.md)
- [AGENTS.md](../../AGENTS.md) - Developer guide
- [Router telemetry](../../docs/PRD-04-router-telemetry.md)
- [Semantic naming](../../docs/PRD-08-semantic-naming.md)

## License

Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
