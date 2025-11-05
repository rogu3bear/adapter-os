//! Stub implementations for when telemetry features are not available

use adapteros_core::Result;
use serde::Serialize;

/// Stub telemetry writer for when telemetry is not available
#[derive(Debug, Clone)]
pub struct TelemetryWriter;

impl TelemetryWriter {
    pub fn log(&self, _event_type: &str, _event: impl Serialize) -> Result<()> {
        // No-op when telemetry is not available
        Ok(())
    }

    pub fn log_security_event(&self, _event: SecurityEvent) -> Result<()> {
        // No-op when telemetry is not available
        Ok(())
    }
}

/// Stub security event for when telemetry is not available
#[derive(Debug)]
pub enum SecurityEvent {
    PolicyViolation {
        policy: String,
        violation_type: String,
        details: serde_json::Value,
        timestamp: String,
    },
}
