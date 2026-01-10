// Routing Telemetry Bridge
// Purpose: Convert telemetry RouterDecisionEvent to database RoutingDecision records
// Author: JKCA
// Date: 2025-11-17

use crate::{Db, RouterCandidate, RoutingDecision};
use adapteros_core::Result;
use adapteros_telemetry::events::RouterDecisionEvent;
use tracing::warn;

/// Convert a RouterDecisionEvent from telemetry to a RoutingDecision database record
///
/// Args:
/// - `event` - The router decision event from telemetry
/// - `tenant_id` - The tenant ID for the inference request
/// - `request_id` - Optional request ID for correlation
///
/// Returns: A RoutingDecision ready to be inserted into the database
pub fn event_to_decision(
    event: &RouterDecisionEvent,
    tenant_id: &str,
    request_id: Option<&str>,
) -> Result<RoutingDecision> {
    // Convert candidates to JSON
    let candidates: Vec<RouterCandidate> = event
        .candidate_adapters
        .iter()
        .map(|c| RouterCandidate {
            adapter_idx: c.adapter_idx,
            raw_score: c.raw_score,
            gate_q15: c.gate_q15,
        })
        .collect();

    let candidates_json = serde_json::to_string(&candidates).map_err(|e| {
        adapteros_core::AosError::Io(format!("Failed to serialize candidates: {}", e))
    })?;

    // Generate unique ID
    let id = format!(
        "{}-{}-{}",
        tenant_id,
        request_id.unwrap_or("unknown"),
        event.step
    );

    // Get current timestamp
    let timestamp = chrono::Utc::now().to_rfc3339();

    // Extract selected adapter IDs from candidates (those with non-zero gates)
    let selected_ids: Vec<String> = event
        .candidate_adapters
        .iter()
        .filter(|c| c.gate_q15 > 0)
        .map(|c| format!("adapter_{}", c.adapter_idx))
        .collect();
    let selected_adapter_ids = if selected_ids.is_empty() {
        None
    } else {
        Some(selected_ids.join(","))
    };

    Ok(RoutingDecision {
        id,
        tenant_id: tenant_id.to_string(),
        timestamp: timestamp.clone(),
        request_id: request_id.map(|s| s.to_string()),
        step: event.step as i64,
        input_token_id: event.input_token_id.map(|t| t as i64),
        stack_id: None, // Will be filled from context if available
        stack_hash: event.stack_hash.clone(),
        entropy: event.entropy as f64,
        tau: event.tau as f64,
        entropy_floor: event.entropy_floor as f64,
        k_value: Some(selected_ids.len() as i64),
        candidate_adapters: candidates_json,
        selected_adapter_ids,
        router_latency_us: None, // Will be filled from timing context if available
        total_inference_latency_us: None,
        overhead_pct: None,
        created_at: timestamp,
    })
}

/// Persist a batch of router decision events to the database
///
/// This is designed to be called after inference completes, processing all router
/// decisions from a single inference request.
///
/// Args:
/// - `db` - Database connection
/// - `events` - Router decision events from inference
/// - `tenant_id` - Tenant ID for the request
/// - `request_id` - Optional request ID for correlation
///
/// Errors: Logs warnings but does not propagate errors to avoid breaking inference
pub async fn persist_router_decisions(
    db: &Db,
    events: &[RouterDecisionEvent],
    tenant_id: &str,
    request_id: Option<&str>,
) -> Result<usize> {
    let mut persisted = 0;

    for event in events {
        match event_to_decision(event, tenant_id, request_id) {
            Ok(decision) => {
                if let Err(e) = db.insert_routing_decision(&decision).await {
                    warn!(
                        error = %e,
                        tenant_id = %tenant_id,
                        request_id = ?request_id,
                        step = event.step,
                        "Failed to persist router decision to database"
                    );
                } else {
                    persisted += 1;
                }
            }
            Err(e) => {
                warn!(
                    error = %e,
                    tenant_id = %tenant_id,
                    request_id = ?request_id,
                    step = event.step,
                    "Failed to convert router decision event to database record"
                );
            }
        }
    }

    Ok(persisted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_telemetry::events::RouterCandidate as TelemetryCandidate;

    #[test]
    fn test_event_to_decision_conversion() {
        let event = RouterDecisionEvent {
            step: 5,
            input_token_id: Some(42),
            candidate_adapters: vec![
                TelemetryCandidate {
                    adapter_idx: 0,
                    raw_score: 0.5,
                    gate_q15: 16384,
                },
                TelemetryCandidate {
                    adapter_idx: 1,
                    raw_score: 0.3,
                    gate_q15: 8192,
                },
            ],
            entropy: 0.75,
            tau: 0.1,
            entropy_floor: 0.01,
            stack_hash: Some("abc123".to_string()),
            stack_id: None,
            stack_version: None,
            model_type: adapteros_types::routing::RouterModelType::Dense,
            active_experts: None,
            backend_type: None,
        };

        let decision = event_to_decision(&event, "default", Some("req-123")).unwrap();

        assert_eq!(decision.tenant_id, "default");
        assert_eq!(decision.request_id, Some("req-123".to_string()));
        assert_eq!(decision.step, 5);
        assert_eq!(decision.input_token_id, Some(42));
        assert_eq!(decision.entropy, 0.75);
        assert_eq!(decision.k_value, Some(2)); // Both candidates have non-zero gates
        assert!(decision.selected_adapter_ids.is_some());
    }

    #[tokio::test]
    async fn test_persist_router_decisions() {
        let db = Db::new_in_memory()
            .await
            .expect("Failed to create in-memory database");

        // Create required tenant for FK constraint
        let tenant_id = db
            .create_tenant("Test Tenant", false)
            .await
            .expect("Failed to create tenant");

        let events = vec![RouterDecisionEvent {
            step: 1,
            input_token_id: Some(10),
            candidate_adapters: vec![TelemetryCandidate {
                adapter_idx: 0,
                raw_score: 0.8,
                gate_q15: 20000,
            }],
            entropy: 0.9,
            tau: 0.2,
            entropy_floor: 0.01,
            stack_hash: None,
            stack_id: None,
            stack_version: None,
            model_type: adapteros_types::routing::RouterModelType::Dense,
            active_experts: None,
            backend_type: None,
        }];

        let persisted = persist_router_decisions(&db, &events, &tenant_id, Some("req-001"))
            .await
            .expect("Failed to persist decisions");

        assert_eq!(persisted, 1);

        // Verify it was actually inserted
        let decisions = db
            .query_routing_decisions(&crate::RoutingDecisionFilters {
                tenant_id: Some(tenant_id.clone()),
                ..Default::default()
            })
            .await
            .expect("Failed to query decisions");

        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].step, 1);
    }
}
