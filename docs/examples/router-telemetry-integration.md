# Router Telemetry Integration Example

This document provides a minimal working example for integrating RouterDecisionEvent telemetry emission into the routing pipeline.

---

## Option 1: Router-Level Integration

Add telemetry writer directly to the `Router` struct.

### Step 1: Modify Router Struct

```rust
// crates/adapteros-lora-router/src/lib.rs

use adapteros_telemetry::RouterDecisionWriter;
use std::sync::Arc;

pub struct Router {
    // ... existing fields ...

    /// Optional telemetry writer for router decisions
    telemetry_writer: Option<Arc<RouterDecisionWriter>>,

    /// Current step/token counter for telemetry
    current_step: usize,
}

impl Router {
    pub fn new_with_weights(feature_weights: RouterWeights, k: usize, tau: f32, eps: f32) -> Self {
        Self {
            feature_weights,
            k,
            tau,
            eps,
            token_count: 0,
            full_log_tokens: 128,
            orthogonal_constraints: None,
            orthogonal_enabled: false,
            compression_ratio: 0.8,
            shared_downsample: false,
            active_stack_name: None,
            active_stack_adapter_ids: None,
            active_stack_hash: None,
            telemetry_writer: None,  // Initialize as None
            current_step: 0,
        }
    }

    /// Set telemetry writer for router decision tracking
    pub fn set_telemetry_writer(&mut self, writer: Arc<RouterDecisionWriter>) {
        self.telemetry_writer = Some(writer);
    }
}
```

### Step 2: Emit Telemetry in route() Method

```rust
// At the end of Router::route() method, before returning Decision

pub fn route(&mut self, features: &[f32], priors: &[f32]) -> Decision {
    // ... existing routing logic ...

    // Create Decision struct (existing code)
    let decision = Decision {
        indices,
        gates_q15,
        entropy,
        candidates: candidate_entries,
    };

    // === TELEMETRY EMISSION (NEW) ===
    if let Some(ref writer) = self.telemetry_writer {
        let event = adapteros_telemetry::events::RouterDecisionEvent {
            step: self.current_step,
            input_token_id: None,  // Add if token ID available from context
            candidate_adapters: decision.candidates.iter().map(|c| {
                adapteros_telemetry::events::RouterCandidate {
                    adapter_idx: c.adapter_idx,
                    raw_score: c.raw_score,
                    gate_q15: c.gate_q15,
                }
            }).collect(),
            entropy: decision.entropy,
            tau: self.tau,
            entropy_floor: self.eps,
            stack_hash: self.stack_hash(),
            stack_id: None,  // V1 skeleton - not required
            stack_version: None,  // V1 skeleton - not required
        };

        // Non-blocking emit, logs warning if channel full
        if let Err(e) = writer.emit(event) {
            tracing::warn!(
                step = self.current_step,
                error = %e,
                "Failed to emit router decision telemetry"
            );
        }
    }

    // Increment step counter
    self.current_step += 1;

    decision
}
```

---

## Option 2: Worker-Level Integration (Recommended for V1 Skeleton)

**Advantage:** No changes to router crate, easier to test and deploy.

### Step 1: Create Helper Function

```rust
// crates/adapteros-lora-worker/src/telemetry_helper.rs (new file)

use adapteros_lora_router::Decision;
use adapteros_telemetry::events::{RouterDecisionEvent, RouterCandidate};

pub fn create_router_decision_event(
    decision: &Decision,
    step: usize,
    tau: f32,
    entropy_floor: f32,
    stack_hash: Option<String>,
) -> RouterDecisionEvent {
    RouterDecisionEvent {
        step,
        input_token_id: None,  // Add if available from inference context
        candidate_adapters: decision.candidates.iter().map(|c| {
            RouterCandidate {
                adapter_idx: c.adapter_idx,
                raw_score: c.raw_score,
                gate_q15: c.gate_q15,
            }
        }).collect(),
        entropy: decision.entropy,
        tau,
        entropy_floor,
        stack_hash,
        stack_id: None,
        stack_version: None,
    }
}
```

### Step 2: Emit in Inference Loop

```rust
// In worker inference loop (pseudocode location)

let mut step = 0;

for token in inference_loop {
    // Call router
    let decision = router.route(&features, &priors);

    // Emit telemetry (non-blocking)
    if let Some(ref telemetry_writer) = self.telemetry_writer {
        let event = telemetry_helper::create_router_decision_event(
            &decision,
            step,
            router.tau(),
            router.eps(),
            router.stack_hash(),
        );

        let _ = telemetry_writer.emit(event);  // Fire-and-forget
    }

    // Apply gates to adapters
    apply_routing_decision(&decision);

    step += 1;
}
```

---

## Background Persistence Task

Consume telemetry events from the writer and persist to database.

```rust
// In server startup (main.rs or worker initialization)

use adapteros_db::routing_telemetry_bridge::event_to_decision;
use adapteros_telemetry::RouterDecisionWriter;

// Create writer and receiver
let (telemetry_writer, mut receiver) = RouterDecisionWriter::new();

// Clone for worker/router
let writer_clone = Arc::new(telemetry_writer);

// Background persistence task
let db = db.clone();
let tenant_id = "default".to_string();  // Get from context

tokio::spawn(async move {
    tracing::info!("Starting router telemetry persistence task");

    while let Some(event) = receiver.recv().await {
        // Convert to database record
        match event_to_decision(&event, &tenant_id, None) {
            Ok(decision) => {
                // Persist to database
                if let Err(e) = db.insert_routing_decision(&decision).await {
                    tracing::warn!(
                        error = %e,
                        step = event.step,
                        "Failed to persist router decision to database"
                    );
                }
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    step = event.step,
                    "Failed to convert router decision event"
                );
            }
        }
    }

    tracing::warn!("Router telemetry persistence task stopped");
});
```

---

## Testing Example

### Unit Test

```rust
#[tokio::test]
async fn test_router_telemetry_emission() {
    use adapteros_lora_router::{Router, RouterWeights};
    use adapteros_telemetry::RouterDecisionWriter;
    use std::sync::Arc;

    // Create router with telemetry
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 0.1, 0.01);
    let (writer, mut receiver) = RouterDecisionWriter::new();
    router.set_telemetry_writer(Arc::new(writer));

    // Mock features and priors
    let features = vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, /* ... */];
    let priors = vec![0.5, 0.3, 0.8, 0.1, 0.2];

    // Route
    let decision = router.route(&features, &priors);

    // Check telemetry event was emitted
    let event = receiver.try_recv().expect("Should have emitted telemetry event");
    assert_eq!(event.step, 0);
    assert_eq!(event.tau, 0.1);
    assert_eq!(event.entropy_floor, 0.01);
    assert_eq!(event.candidate_adapters.len(), 3);  // K = 3
}
```

### Integration Test (Database Persistence)

```rust
#[tokio::test]
async fn test_router_telemetry_persistence() {
    use adapteros_db::Db;
    use adapteros_db::routing_telemetry_bridge::persist_router_decisions;
    use adapteros_telemetry::events::{RouterDecisionEvent, RouterCandidate};

    let db = Db::new_in_memory().await.expect("Failed to create DB");

    let events = vec![RouterDecisionEvent {
        step: 0,
        input_token_id: Some(42),
        candidate_adapters: vec![
            RouterCandidate {
                adapter_idx: 0,
                raw_score: 0.8,
                gate_q15: 20000,
            },
            RouterCandidate {
                adapter_idx: 1,
                raw_score: 0.5,
                gate_q15: 12000,
            },
        ],
        entropy: 0.9,
        tau: 0.1,
        entropy_floor: 0.01,
        stack_hash: Some("abc123".to_string()),
        stack_id: None,
        stack_version: None,
    }];

    // Persist
    let count = persist_router_decisions(&db, &events, "default", Some("req-001"))
        .await
        .expect("Failed to persist");

    assert_eq!(count, 1);

    // Verify in database
    let decisions = db
        .query_routing_decisions(&adapteros_db::RoutingDecisionFilters {
            tenant_id: Some("default".to_string()),
            ..Default::default()
        })
        .await
        .expect("Failed to query");

    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].step, 0);
    assert_eq!(decisions[0].entropy, 0.9);
}
```

---

## Monitoring & Observability

### Check Drop Rate

```rust
// In monitoring/health check endpoint
let drop_rate = telemetry_writer.drop_rate();
let total_events = telemetry_writer.total_count();
let dropped_events = telemetry_writer.dropped_count();

if drop_rate > 0.05 {  // Alert if >5% drop rate
    tracing::warn!(
        drop_rate = %drop_rate,
        total = total_events,
        dropped = dropped_events,
        "High router telemetry drop rate detected"
    );
}
```

### Query Telemetry Data

```rust
// Get recent routing decisions for a tenant
let recent = db.query_routing_decisions(&RoutingDecisionFilters {
    tenant_id: Some("default".to_string()),
    limit: Some(100),
    ..Default::default()
}).await?;

// Get high overhead decisions (>8% budget)
let high_overhead = db.get_high_overhead_decisions(Some("default".to_string()), 50).await?;

// Get low entropy decisions (<0.5, potential routing issues)
let low_entropy = db.get_low_entropy_decisions(Some("default".to_string()), 50).await?;
```

---

## Configuration

### Environment Variables

```bash
# Optional: Configure telemetry buffer size
export ROUTER_TELEMETRY_BUFFER_SIZE=1000

# Optional: Enable/disable router telemetry
export ROUTER_TELEMETRY_ENABLED=true
```

### Runtime Configuration

```rust
// Create writer with custom capacity
let (writer, receiver) = RouterDecisionWriter::with_capacity(5000);

// Check current metrics
tracing::info!(
    total = writer.total_count(),
    dropped = writer.dropped_count(),
    drop_rate = %writer.drop_rate(),
    "Router telemetry metrics"
);
```

---

## Production Deployment

### Checklist

1. ✅ Migration 0070 applied to database
2. ✅ Telemetry writer passed to router/worker
3. ✅ Background persistence task running
4. ✅ Monitoring alerts configured for high drop rate
5. ✅ Retention policy configured (auto-delete old decisions)

### Performance Considerations

- **Channel Capacity:** Default 1000 events. Increase if drop rate >5%.
- **Persistence Latency:** Non-blocking emission, async persistence in background task.
- **Database Writes:** ~1000 writes/second sustainable for SQLite WAL mode.
- **Retention:** Consider auto-cleanup of routing decisions >30 days old.

### Rollback Plan

If telemetry causes issues:
1. Set `telemetry_writer` to `None` in router/worker
2. Stop background persistence task
3. Telemetry emission becomes no-op, zero performance impact

---

## Conclusion

**Recommended Approach:** Use **Option 2 (Worker-Level Integration)** for V1 skeleton.

**Effort:** ~1 hour to implement and test end-to-end.

**Impact:** Zero performance impact when disabled, <1% overhead when enabled (non-blocking channel writes).
