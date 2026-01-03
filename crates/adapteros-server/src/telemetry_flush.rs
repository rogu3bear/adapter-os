use adapteros_core::identity::IdentityEnvelope;
use adapteros_telemetry::unified_events::{EventType, LogLevel, TelemetryEvent as UnifiedEvent};
use adapteros_telemetry::TelemetryWriter;
use chrono::Utc;
use serde_json::json;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

static TELEMETRY_WRITER: OnceLock<Arc<TelemetryWriter>> = OnceLock::new();

pub fn register(writer: Arc<TelemetryWriter>) {
    let _ = TELEMETRY_WRITER.set(writer);
}

pub fn capture_panic(location: &str, message: &str, timeout: Duration) {
    let Some(writer) = TELEMETRY_WRITER.get() else {
        return;
    };
    let _ = std::panic::catch_unwind(|| {
        let identity = IdentityEnvelope::new(
            "system".to_string(),
            "server".to_string(),
            "panic".to_string(),
            "1.0".to_string(),
        );
        let event = UnifiedEvent {
            id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
            timestamp: Utc::now(),
            event_type: EventType::SystemError.as_str().to_string(),
            level: LogLevel::Critical,
            message: "Panic captured".to_string(),
            component: Some("adapteros-server".to_string()),
            identity,
            user_id: None,
            metadata: Some(json!({
                "location": location,
                "message": message,
            })),
            trace_id: None,
            span_id: None,
            hash: None,
            sampling_rate: None,
        };
        let _ = writer.log_event(event);
        let _ = writer.flush_with_timeout(timeout);
    });
}

pub fn flush_on_shutdown(timeout: Duration) {
    let Some(writer) = TELEMETRY_WRITER.get() else {
        return;
    };
    let _ = std::panic::catch_unwind(|| {
        let _ = writer.flush_with_timeout(timeout);
    });
}
