# Router Decision Validation Dataset

**Category:** Router Telemetry
**Format:** JSON routing decisions
**Count:** 1 file
**Endpoints:** Router decision queries

## Files

### routing_decisions.json
**Endpoint:** `GET /v1/routing/decisions`
**Purpose:** K-sparse routing decision validation with entropy metrics

**Schema:**
```json
{
  "decisions": [
    {
      "id": "route-001",
      "tenant_id": "tenant-a",
      "timestamp": "2025-01-18T10:30:45.123Z",
      "request_id": "req-abc-123",
      "step": 42,
      "input_token_id": 1024,
      "stack_id": "stack-prod-001",
      "stack_hash": "blake3:...",
      "entropy": 2.341,
      "tau": 1.0,
      "entropy_floor": 0.1,
      "k_value": 3,
      "candidates": [
        {
          "adapter_idx": 0,
          "raw_score": 0.876,
          "gate_q15": 28672,
          "gate_float": 0.875,
          "selected": true
        }
      ],
      "router_latency_us": 1250,
      "total_inference_latency_us": 45000,
      "overhead_pct": 2.78
    }
  ],
  "total": 3,
  "page": 1,
  "page_size": 50
}
```

## Test Coverage

### K-Sparse Selection
- Exactly k candidates have `selected: true`
- Top-k candidates (by raw_score) are selected
- Non-top-k candidates have `selected: false`

### Q15 Quantization
- `gate_q15` in range [-32768, 32767]
- Conversion accuracy: `gate_float ≈ gate_q15 / 32767.0` (tolerance < 0.01)

### Overhead Metrics
- Calculation: `overhead_pct ≈ (router_latency_us / total_inference_latency_us) * 100`
- Tolerance: < 0.5%

### Entropy Bounds
- `entropy >= 0`
- `entropy_floor <= entropy`
- `tau > 0`

## Validation Rules

| Field | Rule | Tolerance |
|-------|------|-----------|
| **k_value** | Count of `selected: true` must equal k_value | Exact |
| **gate_q15** | Range: [-32768, 32767] | Exact |
| **gate_float** | `gate_q15 / 32767.0` | < 0.01 |
| **overhead_pct** | `(router_us / total_us) * 100` | < 0.5% |
| **entropy** | >= 0 and >= entropy_floor | Exact |
| **tau** | > 0 | Exact |

## Example Test

```rust
#[test]
fn test_routing_decisions_k_sparse() {
    let json = include_str!("../../../tests/training/datasets/routing/routing_decisions.json");
    let response: RoutingDecisionsResponse = serde_json::from_str(json).unwrap();

    for decision in &response.decisions {
        let k = decision.k_value.unwrap_or(0);
        let selected_count = decision.candidates.iter()
            .filter(|c| c.selected)
            .count() as i64;

        assert_eq!(selected_count, k, "Exactly k candidates should be selected");

        // Verify top-k are selected
        let mut sorted = decision.candidates.clone();
        sorted.sort_by(|a, b| b.raw_score.partial_cmp(&a.raw_score).unwrap());

        for (i, candidate) in sorted.iter().enumerate() {
            if i < k as usize {
                assert!(candidate.selected, "Top-{} should be selected", i + 1);
            } else {
                assert!(!candidate.selected, "Non-top-k should not be selected");
            }
        }
    }
}
```

## References

- [API Contract Tests](../../../crates/adapteros-server-api/tests/api_contracts.rs)
- [Router Implementation](../../../crates/adapteros-lora-router/)
- [Router Telemetry](../../../docs/PRD-04-router-telemetry.md)
- [Q15 Quantization](../../../AGENTS.md#k-sparse-routing)
