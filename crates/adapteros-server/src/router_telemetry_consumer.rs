//! Router telemetry consumer background task
//!
//! Consumes RouterDecisionEvent from the bounded channel and persists to database.
//! This runs as a background task to avoid blocking the router hot path.

use adapteros_db::{routing_telemetry_bridge, Db};
use adapteros_telemetry::events::RouterDecisionEvent;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Spawn a background task to consume router decision events and persist them to the database
///
/// This function spawns a deterministic task that consumes events from the provided receiver
/// and persists them to the database using the routing_telemetry_bridge.
///
/// # Arguments
/// * `receiver` - The receiver end of the RouterDecisionWriter channel
/// * `db` - Database handle for persisting events
/// * `tenant_id` - Default tenant ID (can be overridden by event metadata if available)
///
/// # Returns
/// A handle to the spawned task that can be awaited for graceful shutdown
pub fn spawn_consumer(
    mut receiver: mpsc::Receiver<RouterDecisionEvent>,
    db: Arc<Db>,
    tenant_id: String,
) -> tokio::task::JoinHandle<()> {
    // Use tokio::spawn for the telemetry consumer since it needs to run outside
    // the deterministic executor context (it's a long-running I/O bound task)
    tokio::spawn(async move {
        info!("Router telemetry consumer started");
        let mut events_processed = 0u64;
        let mut events_failed = 0u64;

        while let Some(event) = receiver.recv().await {
            debug!(
                step = event.step,
                entropy = event.entropy,
                candidates = event.candidate_adapters.len(),
                "Processing router decision event"
            );

            // Convert event to database record
            // Generate a unique request_id for each event using UUID v4
            // Include step in prefix for traceability while ensuring uniqueness
            let request_id = format!("router-decision-{}-{}", event.step, Uuid::new_v4());

            match routing_telemetry_bridge::event_to_decision(&event, &tenant_id, Some(&request_id))
            {
                Ok(decision) => match db.insert_routing_decision(&decision).await {
                    Ok(_) => {
                        events_processed += 1;
                        if events_processed.is_multiple_of(100) {
                            info!(
                                events_processed,
                                events_failed, "Router telemetry consumer progress"
                            );
                        }
                    }
                    Err(e) => {
                        events_failed += 1;
                        warn!(
                            error = %e,
                            step = event.step,
                            "Failed to persist router decision to database"
                        );
                    }
                },
                Err(e) => {
                    events_failed += 1;
                    warn!(
                        error = %e,
                        step = event.step,
                        "Failed to convert router decision event"
                    );
                }
            }
        }

        info!(
            events_processed,
            events_failed, "Router telemetry consumer stopped"
        );
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_telemetry::events::RouterCandidate;
    use adapteros_telemetry::writer::RouterDecisionWriter;

    #[tokio::test]
    async fn test_consumer_processes_events() {
        // Create in-memory database
        let db = Arc::new(
            Db::new_in_memory()
                .await
                .expect("Failed to create database"),
        );

        // Create required tenant for FK constraint
        let tenant_id = db
            .create_tenant("Test Tenant", false)
            .await
            .expect("Failed to create tenant");

        // Create writer and receiver
        let (writer, receiver) = RouterDecisionWriter::new();

        // Spawn consumer
        let consumer_handle = spawn_consumer(receiver, db.clone(), tenant_id.clone());

        // Emit some events
        for step in 0..5 {
            let event = RouterDecisionEvent {
                step,
                input_token_id: Some(step as u32 * 10),
                candidate_adapters: vec![
                    RouterCandidate {
                        adapter_idx: 0,
                        raw_score: 0.8,
                        gate_q15: 20000,
                    },
                    RouterCandidate {
                        adapter_idx: 1,
                        raw_score: 0.2,
                        gate_q15: 5000,
                    },
                ],
                entropy: 0.75,
                tau: 1.0,
                entropy_floor: 0.02,
                stack_hash: Some("test-stack".to_string()),
                stack_id: None,
                stack_version: None,
                model_type: adapteros_types::routing::RouterModelType::Dense,
                active_experts: None,
            };

            writer.emit(event).expect("Failed to emit event");
        }

        // Give the consumer time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Drop the writer to close the channel
        drop(writer);

        // Wait for consumer to finish
        consumer_handle.await.expect("Consumer task panicked");

        // Verify events were persisted
        let decisions = db
            .query_routing_decisions(&adapteros_db::RoutingDecisionFilters {
                tenant_id: Some(tenant_id),
                limit: Some(10),
                ..Default::default()
            })
            .await
            .expect("Failed to query decisions");

        assert_eq!(decisions.len(), 5, "Should have persisted 5 decisions");
    }
}
