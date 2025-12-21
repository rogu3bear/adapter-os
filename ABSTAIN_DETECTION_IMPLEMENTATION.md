# Abstain Detection Implementation

## Summary

Added abstain detection and AbstainEvent emission to the Router in AdapterOS. The router now monitors two key uncertainty signals and emits telemetry events when thresholds are exceeded.

## Changes Made

### 1. RouterConfig (adapteros-policy/src/packs/router.rs)

Added two new optional threshold fields:

```rust
pub struct RouterConfig {
    // ... existing fields ...

    /// Abstain on high entropy (router uncertainty)
    pub abstain_entropy_threshold: Option<f32>,

    /// Abstain on low confidence (max gate below threshold)
    pub abstain_confidence_threshold: Option<f32>,
}
```

**Default values:** Both are `None` (disabled by default)

**Example usage:**
```rust
let mut config = RouterConfig::default();
config.abstain_entropy_threshold = Some(0.9);      // Abstain if entropy > 0.9
config.abstain_confidence_threshold = Some(0.3);   // Abstain if max gate < 0.3
```

### 2. Router Struct (adapteros-lora-router/src/lib.rs)

Added three new fields:

```rust
pub struct Router {
    // ... existing fields ...

    /// Entropy threshold above which to abstain (high uncertainty)
    abstain_entropy_threshold: Option<f32>,

    /// Confidence threshold below which to abstain (low max gate)
    abstain_confidence_threshold: Option<f32>,

    /// Optional telemetry writer for abstain events
    abstain_telemetry_writer: Option<Arc<TelemetryWriter>>,
}
```

### 3. Router Methods

#### Constructor Integration

The `new_with_policy_config()` constructor now reads abstain thresholds from the policy:

```rust
Self {
    // ... other fields ...
    abstain_entropy_threshold: policy_config.abstain_entropy_threshold,
    abstain_confidence_threshold: policy_config.abstain_confidence_threshold,
    abstain_telemetry_writer: None,
}
```

#### New Public Methods

```rust
/// Set the telemetry writer for abstain events
pub fn set_abstain_telemetry_writer(&mut self, writer: Arc<TelemetryWriter>)

/// Set abstain thresholds
pub fn set_abstain_thresholds(&mut self,
    entropy_threshold: Option<f32>,
    confidence_threshold: Option<f32>)
```

#### Abstain Detection Logic

Added `check_abstain_conditions()` method called during routing:

```rust
fn check_abstain_conditions(&self, entropy: f32, gates: &[f32]) {
    if let Some(ref writer) = self.abstain_telemetry_writer {
        // Check high entropy threshold
        if let Some(entropy_threshold) = self.abstain_entropy_threshold {
            if entropy > entropy_threshold {
                let event = AbstainEvent::high_entropy(entropy, entropy_threshold);
                writer.log_abstain(event)?;
            }
        }

        // Check low confidence threshold (max gate below threshold)
        if let Some(confidence_threshold) = self.abstain_confidence_threshold {
            let max_gate = gates.iter().fold(0.0f32, |a, &b| a.max(b));
            if max_gate < confidence_threshold {
                let event = AbstainEvent::low_confidence(max_gate, confidence_threshold);
                writer.log_abstain(event)?;
            }
        }
    }
}
```

**Integration point:** Called in `route_with_adapter_info()` after entropy calculation (line 1055):

```rust
let entropy = Self::compute_entropy(&gates);

// Check abstain conditions and emit telemetry if triggered
self.check_abstain_conditions(entropy, &gates);
```

### 4. Tests

Added two new tests:

1. `test_abstain_thresholds_from_policy_config()` - Verifies thresholds are read from policy
2. `test_set_abstain_thresholds()` - Verifies programmatic threshold setting

## Detection Conditions

### High Entropy (Uncertainty)
- **Trigger:** `entropy > abstain_entropy_threshold`
- **Meaning:** Router is highly uncertain about which adapters to select
- **Event:** `AbstainEvent::high_entropy(entropy, threshold)`

### Low Confidence (Weak Selection)
- **Trigger:** `max(gates) < abstain_confidence_threshold`
- **Meaning:** No adapter has a strong gate value; all selections are weak
- **Event:** `AbstainEvent::low_confidence(max_gate, threshold)`

## Usage Example

```rust
use adapteros_lora_router::{Router, RouterWeights};
use adapteros_policy::packs::router::RouterConfig;
use adapteros_telemetry::TelemetryWriter;
use std::sync::Arc;

// Create router with abstain thresholds from policy
let mut policy_config = RouterConfig::default();
policy_config.abstain_entropy_threshold = Some(0.9);
policy_config.abstain_confidence_threshold = Some(0.3);

let mut router = Router::new_with_policy_config(
    RouterWeights::default(),
    3,    // k
    1.0,  // tau
    &policy_config,
);

// Set up telemetry writer
let telemetry_writer = Arc::new(TelemetryWriter::new(
    "telemetry_output",
    10000,
    1024 * 1024,
)?);
router.set_abstain_telemetry_writer(telemetry_writer);

// Router will now emit AbstainEvent when conditions are met
let decision = router.route_with_adapter_info(&features, &priors, &adapter_info);
```

## Telemetry Events

Events are emitted via `TelemetryWriter::log_abstain()` and logged as:
- Event type: `"policy.abstain"`
- Event structure: See `AbstainEvent` in `adapteros-telemetry/src/events/telemetry_events.rs:192-278`

## Key Design Decisions

1. **Thresholds are optional** - Abstain detection is disabled by default to avoid breaking existing behavior
2. **Non-blocking** - Telemetry emission failures are logged but don't fail routing
3. **Policy-driven** - Thresholds can be configured via RouterConfig for consistency
4. **Dual signals** - Both entropy and confidence provide different perspectives on uncertainty
5. **Existing events** - Uses the already-defined `AbstainEvent` structure from telemetry

## Files Modified

1. `/Users/mln-dev/Dev/adapter-os/crates/adapteros-policy/src/packs/router.rs`
   - Added `abstain_entropy_threshold` and `abstain_confidence_threshold` to `RouterConfig`

2. `/Users/mln-dev/Dev/adapter-os/crates/adapteros-lora-router/src/lib.rs`
   - Added abstain fields to `Router` struct
   - Added `set_abstain_telemetry_writer()` and `set_abstain_thresholds()` methods
   - Added `check_abstain_conditions()` private method
   - Integrated abstain detection into `route_with_adapter_info()`
   - Added tests for abstain configuration

## Testing

All existing tests pass (54 tests in adapteros-lora-router).

New tests verify:
- Thresholds are correctly read from policy config
- Thresholds can be set programmatically
- Default values are None (disabled)

To run tests:
```bash
cargo test -p adapteros-lora-router --lib
```

## Notes

- The implementation follows the existing pattern for RouterDecisionWriter telemetry
- AbstainEvent factory methods (`high_entropy`, `low_confidence`) were already defined
- The feature is backward-compatible - existing code continues to work without abstain detection
- Telemetry writer must be explicitly set for events to be emitted
