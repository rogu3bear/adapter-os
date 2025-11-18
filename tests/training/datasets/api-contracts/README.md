# API Contract Test Reference Data

**Type:** CLI/API Contract Validation Dataset
**Purpose:** Fake server responses stored as reference JSON files for unit tests
**Count:** ~20 mock responses
**Format:** JSON

## Endpoints Covered

### Adapter Management
- `adapters_list.json` - `/v1/adapters/list` - List adapters with stats
- `adapter_lineage.json` - `/v1/adapters/{id}/lineage` - Family tree with ancestors/descendants

### Router Decisions
- `routing_decisions.json` - `/v1/routing/decisions` - K-sparse routing decisions with entropy metrics

### Health Checks
- `healthz_basic.json` - `/healthz` - Basic health status
- `healthz_all.json` - `/healthz/all` - All components healthy
- `healthz_degraded.json` - `/healthz/all` - System with degraded components
- `healthz_router.json` - `/healthz/router` - Router component details

## Schema Validation

Each JSON file represents a canonical API response format used for contract testing. Tests verify:

1. **Field presence** - All required fields exist
2. **Type correctness** - Fields match expected types
3. **Enum validity** - Status values match defined enums
4. **Relationship integrity** - IDs reference valid entities
5. **Format compliance** - Timestamps, hashes follow conventions

## Usage in Tests

```rust
use serde_json::from_str;

#[test]
fn test_adapters_list_contract() {
    let json = include_str!("../training/datasets/api-contracts/adapters_list.json");
    let response: AdaptersListResponse = from_str(json).expect("Valid schema");
    assert_eq!(response.adapters.len(), 3);
    assert_eq!(response.total, 3);
}
```

## Maintenance

When API schemas change:
1. Update corresponding JSON files
2. Run contract tests to verify compatibility
3. Document breaking changes in migration notes
