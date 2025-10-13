//! Training Signal Bridge
//!
//! Connects lifecycle manager and profiler to the signal protocol,
//! converting telemetry events into training signals for SSE streaming.
//!
//! Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.2

use crate::signal::{Signal, SignalBuilder, SignalPriority, SignalType};
use adapteros_lora_lifecycle::{AdapterTransitionEvent, AdapterActivationEvent, AdapterEvictionEvent};
use adapteros_profiler::AdapterMetrics;
use serde_json::json;
use tokio::sync::mpsc;

/// Bridge for converting lifecycle events to signals
///
/// This component listens to telemetry events from the lifecycle manager
/// and profiler, converting them into training signals that can be streamed
/// via SSE to clients.
///
/// # Example
/// ```no_run
/// use adapteros_lora_worker::training_signal_bridge::TrainingSignalBridge;
/// use tokio::sync::mpsc;
///
/// let (signal_tx, signal_rx) = mpsc::channel(100);
/// let mut bridge = TrainingSignalBridge::new(signal_tx);
///
/// // Bridge will convert lifecycle events to signals
/// ```
pub struct TrainingSignalBridge {
    signal_tx: mpsc::Sender<Signal>,
}

impl TrainingSignalBridge {
    /// Create a new training signal bridge
    pub fn new(signal_tx: mpsc::Sender<Signal>) -> Self {
        Self { signal_tx }
    }

    /// Handle adapter state transition event
    ///
    /// Converts a lifecycle transition event into an AdapterStateTransition signal.
    ///
    /// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.2
    pub async fn on_adapter_transition(&self, event: &AdapterTransitionEvent) {
        let signal = SignalBuilder::new(SignalType::AdapterStateTransition)
            .priority(SignalPriority::Normal)
            .with_field("adapter_id", json!(event.adapter_id))
            .with_field("from_state", json!(event.from_state))
            .with_field("to_state", json!(event.to_state))
            .with_field("reason", json!(event.reason))
            .build();

        let _ = self.signal_tx.send(signal).await;
    }

    /// Handle adapter promotion event
    ///
    /// Converts a transition to a higher state (e.g., warm → hot) into
    /// an AdapterPromoted signal.
    pub async fn on_adapter_promoted(&self, adapter_id: &str, to_state: &str, reason: &str) {
        let signal = SignalBuilder::new(SignalType::AdapterPromoted)
            .priority(SignalPriority::High)
            .with_field("adapter_id", json!(adapter_id))
            .with_field("to_state", json!(to_state))
            .with_field("reason", json!(reason))
            .build();

        let _ = self.signal_tx.send(signal).await;
    }

    /// Handle adapter demotion event
    pub async fn on_adapter_demoted(&self, adapter_id: &str, to_state: &str, reason: &str) {
        let signal = SignalBuilder::new(SignalType::AdapterDemoted)
            .priority(SignalPriority::Normal)
            .with_field("adapter_id", json!(adapter_id))
            .with_field("to_state", json!(to_state))
            .with_field("reason", json!(reason))
            .build();

        let _ = self.signal_tx.send(signal).await;
    }

    /// Handle profiler metrics update
    ///
    /// Simplified version that takes raw metrics values.
    ///
    /// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.3
    pub async fn on_profiler_metrics(&self, adapter_id: &str, activation_pct: f32, avg_latency_us: f32, memory_bytes: usize) {
        let signal = SignalBuilder::new(SignalType::ProfilerMetrics)
            .priority(SignalPriority::Low) // High frequency, sample down
            .with_field("adapter_id", json!(adapter_id))
            .with_field("activation_pct", json!(activation_pct))
            .with_field("avg_latency_us", json!(avg_latency_us))
            .with_field("memory_bytes", json!(memory_bytes))
            .build();

        let _ = self.signal_tx.send(signal).await;
    }

    /// Handle K reduction event (memory pressure)
    ///
    /// Emitted when the router's K sparse value is reduced due to memory pressure.
    ///
    /// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.2
    pub async fn on_k_reduced(&self, old_k: usize, new_k: usize, reason: &str) {
        let signal = SignalBuilder::new(SignalType::KReduced)
            .priority(SignalPriority::Critical) // Important for operators
            .with_field("old_k", json!(old_k))
            .with_field("new_k", json!(new_k))
            .with_field("reason", json!(reason))
            .build();

        let _ = self.signal_tx.send(signal).await;
    }

    /// Handle adapter activation event
    pub async fn on_adapter_activation(&self, event: &AdapterActivationEvent) {
        let signal = SignalBuilder::new(SignalType::AdapterStateTransition)
            .priority(SignalPriority::Low)
            .with_field("adapter_id", json!(event.adapter_id))
            .with_field("state", json!(event.state))
            .with_field("category", json!(event.category))
            .with_field("activation_count", json!(event.activation_count))
            .build();

        let _ = self.signal_tx.send(signal).await;
    }

    /// Handle adapter eviction event
    pub async fn on_adapter_evicted(&self, event: &AdapterEvictionEvent) {
        let signal = SignalBuilder::new(SignalType::AdapterStateTransition)
            .priority(SignalPriority::High)
            .with_field("adapter_id", json!(event.adapter_id))
            .with_field("from_state", json!(event.from_state))
            .with_field("to_state", json!("unloaded"))
            .with_field("category", json!(event.category))
            .with_field("memory_freed", json!(event.memory_freed))
            .with_field("reason", json!("eviction"))
            .build();

        let _ = self.signal_tx.send(signal).await;
    }

    /// Send training progress update
    ///
    /// Generic training progress signal for ongoing adapter fine-tuning.
    pub async fn on_training_progress(
        &self,
        adapter_id: &str,
        epoch: u32,
        loss: f32,
        progress_pct: f32,
    ) {
        let signal = SignalBuilder::new(SignalType::TrainingProgress)
            .priority(SignalPriority::Normal)
            .with_field("adapter_id", json!(adapter_id))
            .with_field("epoch", json!(epoch))
            .with_field("loss", json!(loss))
            .with_field("progress_pct", json!(progress_pct))
            .build();

        let _ = self.signal_tx.send(signal).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_adapter_transition_signal() {
        let (tx, mut rx) = mpsc::channel(10);
        let bridge = TrainingSignalBridge::new(tx);

        let event = AdapterTransitionEvent {
            adapter_id: "test_adapter".to_string(),
            from_state: "warm".to_string(),
            to_state: "hot".to_string(),
            reason: "high_activation".to_string(),
        };

        bridge.on_adapter_transition(&event).await;

        let signal = rx.recv().await.expect("Test signal receive should succeed");
        assert_eq!(signal.signal_type, SignalType::AdapterStateTransition);
        assert_eq!(signal.priority, SignalPriority::Normal);
    }

    #[tokio::test]
    async fn test_adapter_promoted_signal() {
        let (tx, mut rx) = mpsc::channel(10);
        let bridge = TrainingSignalBridge::new(tx);

        bridge
            .on_adapter_promoted("adapter_1", "resident", "consistent_high_quality")
            .await;

        let signal = rx.recv().await.expect("Test signal receive should succeed");
        assert_eq!(signal.signal_type, SignalType::AdapterPromoted);
        assert_eq!(signal.priority, SignalPriority::High);
    }

    #[tokio::test]
    async fn test_k_reduced_signal() {
        let (tx, mut rx) = mpsc::channel(10);
        let bridge = TrainingSignalBridge::new(tx);

        bridge.on_k_reduced(3, 2, "memory_pressure").await;

        let signal = rx.recv().await.expect("Test signal receive should succeed");
        assert_eq!(signal.signal_type, SignalType::KReduced);
        assert_eq!(signal.priority, SignalPriority::Critical);
    }

    #[tokio::test]
    async fn test_profiler_metrics_signal() {
        let (tx, mut rx) = mpsc::channel(10);
        let bridge = TrainingSignalBridge::new(tx);

        bridge.on_profiler_metrics("adapter_2", 15.5, 450.0, 1024 * 1024 * 10).await;

        let signal = rx.recv().await.expect("Test signal receive should succeed");
        assert_eq!(signal.signal_type, SignalType::ProfilerMetrics);
        assert_eq!(signal.priority, SignalPriority::Low);
    }
}

