//! Circuit breaker telemetry events
//!
//! Events for tracking circuit breaker state transitions and operations.

use adapteros_core::identity::IdentityEnvelope;
use crate::{EventType, LogLevel, TelemetryEventBuilder};
use crate::unified_events::TelemetryEvent;
use adapteros_core::{CircuitBreakerMetrics, CircuitState};
use serde_json::json;

/// Convert CircuitState to a serializable string representation
fn state_to_string(state: &CircuitState) -> String {
    match state {
        CircuitState::Closed => "closed".to_string(),
        CircuitState::Open { .. } => "open".to_string(),
        CircuitState::HalfOpen => "half_open".to_string(),
    }
}

/// Create a default identity envelope for circuit breaker events
fn circuit_breaker_identity() -> IdentityEnvelope {
    IdentityEnvelope::new(
        "system".to_string(),
        "circuit_breaker".to_string(),
        "monitor".to_string(),
        "1.0".to_string(),
    )
}

/// Circuit breaker opened event
pub fn circuit_breaker_opened(service: &str, metrics: &CircuitBreakerMetrics) -> TelemetryEvent {
    TelemetryEventBuilder::new(
        EventType::Custom("circuit_breaker.opened".to_string()),
        LogLevel::Warn,
        format!("Circuit breaker opened for service '{}'", service),
        circuit_breaker_identity(),
    )
    .component("adapteros-core".to_string())
    .metadata(json!({
        "service": service,
        "state": state_to_string(&metrics.state),
        "requests_total": metrics.requests_total,
        "failures_total": metrics.failures_total,
        "opens_total": metrics.opens_total,
        "last_state_change": metrics.last_state_change
    }))
    .build()
}

/// Circuit breaker closed event
pub fn circuit_breaker_closed(service: &str, metrics: &CircuitBreakerMetrics) -> TelemetryEvent {
    TelemetryEventBuilder::new(
        EventType::Custom("circuit_breaker.closed".to_string()),
        LogLevel::Info,
        format!("Circuit breaker closed for service '{}'", service),
        circuit_breaker_identity(),
    )
    .component("adapteros-core".to_string())
    .metadata(json!({
        "service": service,
        "state": state_to_string(&metrics.state),
        "requests_total": metrics.requests_total,
        "successes_total": metrics.successes_total,
        "closes_total": metrics.closes_total,
        "last_state_change": metrics.last_state_change
    }))
    .build()
}

/// Circuit breaker half-open event
pub fn circuit_breaker_half_open(service: &str, metrics: &CircuitBreakerMetrics) -> TelemetryEvent {
    TelemetryEventBuilder::new(
        EventType::Custom("circuit_breaker.half_open".to_string()),
        LogLevel::Info,
        format!("Circuit breaker transitioned to half-open for service '{}'", service),
        circuit_breaker_identity(),
    )
    .component("adapteros-core".to_string())
    .metadata(json!({
        "service": service,
        "state": state_to_string(&metrics.state),
        "requests_total": metrics.requests_total,
        "half_opens_total": metrics.half_opens_total,
        "last_state_change": metrics.last_state_change
    }))
    .build()
}

/// Circuit breaker request rejected event
pub fn circuit_breaker_request_rejected(service: &str, state: CircuitState) -> TelemetryEvent {
    let reason = match state {
        CircuitState::Open { .. } => "circuit_open",
        CircuitState::HalfOpen => "half_open_limit_exceeded",
        CircuitState::Closed => "unexpected_closed_state",
    };

    TelemetryEventBuilder::new(
        EventType::Custom("circuit_breaker.request_rejected".to_string()),
        LogLevel::Warn,
        format!("Request rejected by circuit breaker for service '{}' ({})", service, reason),
        circuit_breaker_identity(),
    )
    .component("adapteros-core".to_string())
    .metadata(json!({
        "service": service,
        "state": state_to_string(&state),
        "reason": reason
    }))
    .build()
}

/// Circuit breaker recovery test event
pub fn circuit_breaker_recovery_test(service: &str, success: bool) -> TelemetryEvent {
    let level = if success { LogLevel::Info } else { LogLevel::Warn };
    let status = if success { "success" } else { "failure" };

    TelemetryEventBuilder::new(
        EventType::Custom("circuit_breaker.recovery_test".to_string()),
        level,
        format!("Circuit breaker recovery test {} for service '{}'", status, service),
        circuit_breaker_identity(),
    )
    .component("adapteros-core".to_string())
    .metadata(json!({
        "service": service,
        "success": success,
        "test_type": "half_open_recovery"
    }))
    .build()
}

/// Circuit breaker metrics snapshot event
pub fn circuit_breaker_metrics(service: &str, metrics: &CircuitBreakerMetrics) -> TelemetryEvent {
    TelemetryEventBuilder::new(
        EventType::Custom("circuit_breaker.metrics".to_string()),
        LogLevel::Debug,
        format!("Circuit breaker metrics for service '{}'", service),
        circuit_breaker_identity(),
    )
    .component("adapteros-core".to_string())
    .metadata(json!({
        "service": service,
        "state": state_to_string(&metrics.state),
        "requests_total": metrics.requests_total,
        "successes_total": metrics.successes_total,
        "failures_total": metrics.failures_total,
        "opens_total": metrics.opens_total,
        "closes_total": metrics.closes_total,
        "half_opens_total": metrics.half_opens_total,
        "last_state_change": metrics.last_state_change
    }))
    .build()
}
