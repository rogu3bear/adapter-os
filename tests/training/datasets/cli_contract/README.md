# CLI/API Contract Validation Dataset

**Category:** Contract Testing
**Format:** JSON mock responses
**Count:** 2 files
**Endpoints:** Adapter management

## Files

### adapters_list.json
**Endpoint:** `GET /v1/adapters/list`
**Purpose:** List adapters with metadata and stats

**Schema:**
```json
{
  "adapters": [
    {
      "id": "string",
      "adapter_id": "string",
      "name": "tenant/domain/purpose/revision",
      "hash_b3": "blake3:...",
      "rank": 16,
      "tier": "tier_1",
      "current_state": "warm",
      "languages": ["rust", "python"],
      "framework": "llama",
      "created_at": "2025-01-15T10:30:00Z",
      "stats": {
        "total_activations": 1250,
        "selected_count": 980,
        "avg_gate_value": 0.87,
        "selection_rate": 0.784
      }
    }
  ],
  "total": 3,
  "page": 1,
  "page_size": 10
}
```

**Test Coverage:**
- Schema validation
- Semantic naming format
- Tier values (tier_1, tier_2, tier_3)
- State machine (unloaded, cold, warm, hot, resident)
- BLAKE3 hash format
- Pagination fields

---

### adapter_lineage.json
**Endpoint:** `GET /v1/adapters/{id}/lineage`
**Purpose:** Get adapter family tree (ancestors + descendants)

**Schema:**
```json
{
  "adapter_id": "def456",
  "ancestors": [
    {
      "adapter_id": "def123",
      "adapter_name": "tenant-b/ml/sentiment-analysis/r001",
      "parent_id": null,
      "fork_type": null,
      "created_at": "2025-01-10T12:00:00Z"
    }
  ],
  "self_node": { /* ... */ },
  "descendants": [ /* ... */ ],
  "total_nodes": 5
}
```

**Test Coverage:**
- Tree structure validation
- Ancestry chain integrity (parent_id references)
- Fork type validation (incremental_improvement, experimental, domain_adaptation, etc.)
- Node count accuracy

---

## Validation Rules

| Field | Rule |
|-------|------|
| **name** | `tenant/domain/purpose/revision` (3 slashes) |
| **hash_b3** | `blake3:` + 64 hex chars (71 total) |
| **tier** | tier_1, tier_2, tier_3 |
| **current_state** | unloaded, cold, warm, hot, resident |
| **created_at** | ISO-8601 with 'T' and 'Z' |
| **fork_type** | Enum of 5 types (or null for root) |

## Usage

```rust
use serde_json::from_str;

#[test]
fn test_adapters_list_contract() {
    let json = include_str!("../../../tests/training/datasets/cli_contract/adapters_list.json");
    let response: AdaptersListResponse = from_str(json).expect("Valid schema");
    assert_eq!(response.adapters.len(), 3);
    assert_eq!(response.total, 3);
}
```

## References

- [API Contract Tests](../../../crates/adapteros-server-api/tests/api_contracts.rs)
- [Type Definitions](../../../crates/adapteros-server-api/src/types.rs)
- [Semantic Naming](../../../docs/PRD-08-semantic-naming.md)
