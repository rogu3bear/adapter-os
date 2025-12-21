# Abstain Detection Flow

## High-Level Flow

```
route_with_adapter_info()
    |
    |- Compute scores for adapters
    |- Select top K adapters
    |- Apply softmax + entropy floor
    |- Quantize gates to Q15
    |
    |- Compute entropy
    |
    |- check_abstain_conditions(entropy, gates)  <-- NEW
    |      |
    |      |- Check entropy > abstain_entropy_threshold?
    |      |    Yes -> emit AbstainEvent::high_entropy()
    |      |
    |      |- Check max(gates) < abstain_confidence_threshold?
    |           Yes -> emit AbstainEvent::low_confidence()
    |
    |- Build Decision
    |- Emit RouterDecisionEvent
    |
    return Decision
```

## Code Locations

### Policy Configuration
**File:** `crates/adapteros-policy/src/packs/router.rs`
**Lines:** 10-31 (RouterConfig struct)

### Router Struct
**File:** `crates/adapteros-lora-router/src/lib.rs`
**Lines:** 193-244 (Router struct definition)

### Abstain Detection
**File:** `crates/adapteros-lora-router/src/lib.rs`
**Lines:** 408-462 (`check_abstain_conditions` method)

### Integration Point
**File:** `crates/adapteros-lora-router/src/lib.rs`
**Line:** 1055 (called in `route_with_adapter_info`)

### Telemetry Events
**File:** `crates/adapteros-telemetry/src/events/telemetry_events.rs`
**Lines:** 192-278 (AbstainEvent definition)

### TelemetryWriter API
**File:** `crates/adapteros-telemetry/src/lib.rs`
**Line:** 315 (`log_abstain` method)

## Example Values

### Recommended Thresholds

```rust
// High entropy threshold (0.0 to log2(k))
// For k=4: max entropy is log2(4) = 2.0
// Threshold of 0.9 means entropy > 90% of maximum
abstain_entropy_threshold: Some(0.9)

// Low confidence threshold (0.0 to 1.0)
// Threshold of 0.3 means max gate < 30%
abstain_confidence_threshold: Some(0.3)
```

### Example Scenarios

#### Scenario 1: Balanced Distribution (High Entropy)
```
Gates: [0.27, 0.26, 0.24, 0.23]
Entropy: ~1.99 (very high, close to 2.0 maximum)
Max gate: 0.27

If entropy_threshold = 0.9:
  -> ABSTAIN (high entropy)
```

#### Scenario 2: Weak Preferences (Low Confidence)
```
Gates: [0.28, 0.25, 0.24, 0.23]
Entropy: ~1.98
Max gate: 0.28

If confidence_threshold = 0.3:
  -> ABSTAIN (max gate < 0.3)
```

#### Scenario 3: Strong Preference (No Abstain)
```
Gates: [0.65, 0.15, 0.12, 0.08]
Entropy: ~1.16
Max gate: 0.65

Thresholds: entropy=0.9, confidence=0.3
  -> NO ABSTAIN (entropy < 1.8, max gate > 0.3)
```

## Entropy Calculation

```rust
fn compute_entropy(gates: &[f32]) -> f32 {
    gates
        .iter()
        .filter(|&&g| g > 0.0)
        .map(|&g| -g * g.log2())
        .sum()
}
```

**Maximum entropy for k adapters:** `log2(k)`
- k=2: 1.0
- k=3: 1.585
- k=4: 2.0
- k=8: 3.0

**Interpretation:**
- Entropy near 0: One adapter dominates (low uncertainty)
- Entropy near max: Uniform distribution (high uncertainty)
