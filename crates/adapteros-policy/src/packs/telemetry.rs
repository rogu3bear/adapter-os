//! Telemetry Policy Pack
//!
//! Sampling rules, bundle rotation, and signing. Enforces enough observability
//! to audit, not enough to melt disks.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::{AosError, Result};
use adapteros_telemetry::unified_events::TelemetryEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Telemetry policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Schema hash for canonical JSON
    pub schema_hash: String,
    /// Sampling configuration
    pub sampling: SamplingConfig,
    /// Number of tokens to log full router decisions
    pub router_full_tokens: usize,
    /// Bundle configuration
    pub bundle: BundleConfig,
    /// Event types and their sampling rates
    pub event_sampling: HashMap<String, f32>,
    /// Retention policy
    pub retention: RetentionConfig,
}

/// Sampling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingConfig {
    /// Token sampling rate (after initial full logging)
    pub token: f32,
    /// Router sampling rate
    pub router: f32,
    /// Inference sampling rate
    pub inference: f32,
    /// Policy violation sampling rate
    pub policy_violation: f32,
    /// Security violation sampling rate
    pub security_violation: f32,
}

/// Bundle configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleConfig {
    /// Maximum events per bundle
    pub max_events: usize,
    /// Maximum bytes per bundle
    pub max_bytes: usize,
    /// Bundle rotation interval (hours)
    pub rotation_interval_hours: u32,
    /// Enable bundle signing
    pub enable_signing: bool,
    /// Bundle compression
    pub compression: CompressionConfig,
}

/// Compression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Enable compression
    pub enable: bool,
    /// Compression algorithm
    pub algorithm: CompressionAlgorithm,
    /// Compression level
    pub level: u32,
}

/// Compression algorithm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    /// Gzip compression
    Gzip,
    /// Brotli compression
    Brotli,
    /// LZ4 compression
    Lz4,
}

/// Retention configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    /// Retention period (days)
    pub retention_days: u32,
    /// Enable automatic cleanup
    pub enable_cleanup: bool,
    /// Cleanup interval (hours)
    pub cleanup_interval_hours: u32,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        let mut event_sampling = HashMap::new();
        event_sampling.insert("router.decision".to_string(), 1.0);
        event_sampling.insert("policy.abstain".to_string(), 1.0);
        event_sampling.insert("security.violation".to_string(), 1.0);
        event_sampling.insert("adapter.evict".to_string(), 1.0);
        event_sampling.insert("inference.token".to_string(), 0.05);
        event_sampling.insert("inference.complete".to_string(), 1.0);

        Self {
            schema_hash: "b3:default".to_string(),
            sampling: SamplingConfig {
                token: 0.05,
                router: 1.0,
                inference: 1.0,
                policy_violation: 1.0,
                security_violation: 1.0,
            },
            router_full_tokens: 128,
            bundle: BundleConfig {
                max_events: 500000,
                max_bytes: 268435456, // 256 MB
                rotation_interval_hours: 24,
                enable_signing: true,
                compression: CompressionConfig {
                    enable: true,
                    algorithm: CompressionAlgorithm::Gzip,
                    level: 6,
                },
            },
            event_sampling,
            retention: RetentionConfig {
                retention_days: 30,
                enable_cleanup: true,
                cleanup_interval_hours: 24,
            },
        }
    }
}

/// Policy telemetry view (projection for policy validation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyTelemetryView {
    /// Event type
    pub event_type: String,
    /// Event timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Event data
    pub data: serde_json::Value,
    /// Event hash
    pub hash: String,
    /// Sampling rate applied
    pub sampling_rate: f32,
}

/// Conversion from canonical TelemetryEvent to policy view
impl From<TelemetryEvent> for PolicyTelemetryView {
    fn from(ev: TelemetryEvent) -> Self {
        PolicyTelemetryView {
            event_type: ev.event_type,
            timestamp: ev.timestamp.with_timezone(&chrono::Utc),
            data: ev.metadata.unwrap_or_else(|| serde_json::Value::Null),
            hash: ev.hash.unwrap_or_else(|| "b3:default".to_string()),
            sampling_rate: ev.sampling_rate.unwrap_or(1.0),
        }
    }
}

/// Telemetry bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryBundle {
    /// Bundle ID
    pub bundle_id: String,
    /// Bundle timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Events in bundle
    pub events: Vec<TelemetryEvent>,
    /// Bundle hash
    pub bundle_hash: String,
    /// Merkle root
    pub merkle_root: String,
    /// Bundle signature
    pub signature: Option<String>,
}

/// Telemetry policy enforcement
pub struct TelemetryPolicy {
    config: TelemetryConfig,
}

impl TelemetryPolicy {
    /// Create a new telemetry policy
    pub fn new(config: TelemetryConfig) -> Self {
        Self { config }
    }

    /// Validate event sampling
    pub fn validate_event_sampling(&self, event_type: &str, sampling_rate: f32) -> Result<()> {
        if let Some(expected_rate) = self.config.event_sampling.get(event_type) {
            if (sampling_rate - expected_rate).abs() > 1e-6 {
                return Err(AosError::PolicyViolation(format!(
                    "Event sampling rate {} does not match expected {} for event type {}",
                    sampling_rate, expected_rate, event_type
                )));
            }
        }

        Ok(())
    }

    /// Check if event should be sampled
    pub fn should_sample_event(&self, event_type: &str, token_count: usize) -> bool {
        let sampling_rate = self.config.event_sampling.get(event_type).unwrap_or(&0.0);

        // Always sample first N tokens for router decisions
        if event_type == "router.decision" && token_count <= self.config.router_full_tokens {
            return true;
        }

        // Use sampling rate for other events
        if *sampling_rate >= 1.0 {
            return true;
        }

        // Simple random sampling (in real implementation, use proper RNG)
        let random_value = (token_count as f32 * 0.001) % 1.0;
        random_value < *sampling_rate
    }

    /// Validate bundle size
    pub fn validate_bundle_size(&self, bundle: &TelemetryBundle) -> Result<()> {
        if bundle.events.len() > self.config.bundle.max_events {
            return Err(AosError::PolicyViolation(format!(
                "Bundle has {} events, exceeds maximum {}",
                bundle.events.len(),
                self.config.bundle.max_events
            )));
        }

        // Estimate bundle size (rough approximation)
        let estimated_size = bundle.events.len() * 1024; // Assume 1KB per event
        if estimated_size > self.config.bundle.max_bytes {
            return Err(AosError::PolicyViolation(format!(
                "Bundle estimated size {} bytes exceeds maximum {}",
                estimated_size, self.config.bundle.max_bytes
            )));
        }

        Ok(())
    }

    /// Validate bundle rotation
    pub fn validate_bundle_rotation(
        &self,
        last_rotation: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let rotation_interval =
            chrono::Duration::hours(self.config.bundle.rotation_interval_hours as i64);
        let next_rotation = last_rotation + rotation_interval;
        let now = chrono::Utc::now();

        if now > next_rotation {
            Err(AosError::PolicyViolation(
                "Bundle rotation is overdue".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Validate bundle signing
    pub fn validate_bundle_signing(&self, bundle: &TelemetryBundle) -> Result<()> {
        if self.config.bundle.enable_signing && bundle.signature.is_none() {
            return Err(AosError::PolicyViolation(
                "Bundle signing is required but signature is missing".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate schema hash
    pub fn validate_schema_hash(&self, hash: &str) -> Result<()> {
        if hash != self.config.schema_hash {
            Err(AosError::PolicyViolation(format!(
                "Schema hash {} does not match expected {}",
                hash, self.config.schema_hash
            )))
        } else {
            Ok(())
        }
    }

    /// Calculate event hash
    pub fn calculate_event_hash(&self, event: &PolicyTelemetryView) -> String {
        // In a real implementation, use BLAKE3 or similar
        format!("b3:{}", event.event_type.len())
    }

    /// Calculate bundle hash
    pub fn calculate_bundle_hash(&self, bundle: &TelemetryBundle) -> String {
        // In a real implementation, use BLAKE3 or similar
        format!("b3:{}", bundle.events.len())
    }

    /// Calculate Merkle root
    pub fn calculate_merkle_root(&self, bundle: &TelemetryBundle) -> String {
        // In a real implementation, build a proper Merkle tree
        format!("merkle:{}", bundle.events.len())
    }

    /// Validate retention policy
    pub fn validate_retention_policy(
        &self,
        event_timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let retention_duration =
            chrono::Duration::days(self.config.retention.retention_days as i64);
        let cutoff_time = chrono::Utc::now() - retention_duration;

        if event_timestamp < cutoff_time {
            Err(AosError::PolicyViolation(
                "Event is older than retention policy allows".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Check if bundle should be rotated
    pub fn should_rotate_bundle(&self, bundle: &TelemetryBundle) -> bool {
        // Check event count
        if bundle.events.len() >= self.config.bundle.max_events {
            return true;
        }

        // Check time interval
        let rotation_interval =
            chrono::Duration::hours(self.config.bundle.rotation_interval_hours as i64);
        let next_rotation = bundle.timestamp + rotation_interval;
        let now = chrono::Utc::now();

        now > next_rotation
    }

    /// Generate canonical JSON
    pub fn generate_canonical_json(&self, event: &PolicyTelemetryView) -> Result<String> {
        // In a real implementation, use JCS (JSON Canonicalization Scheme)
        serde_json::to_string(event).map_err(|e| AosError::PolicyViolation(e.to_string()))
    }
}

impl Policy for TelemetryPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Telemetry
    }

    fn name(&self) -> &'static str {
        "Telemetry"
    }

    fn severity(&self) -> Severity {
        Severity::Medium
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        let violations = Vec::new();

        // Basic validation - in a real implementation, this would check
        // specific policy requirements

        if violations.is_empty() {
            Ok(Audit::passed(self.id()))
        } else {
            Ok(Audit::failed(self.id(), violations))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_telemetry_policy_creation() {
        let config = TelemetryConfig::default();
        let policy = TelemetryPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Telemetry);
        assert_eq!(policy.name(), "Telemetry");
        assert_eq!(policy.severity(), Severity::Medium);
    }

    #[test]
    fn test_telemetry_config_default() {
        let config = TelemetryConfig::default();
        assert_eq!(config.router_full_tokens, 128);
        assert_eq!(config.bundle.max_events, 500000);
        assert_eq!(config.bundle.max_bytes, 268435456);
        assert!(config.bundle.enable_signing);
        assert!(config.retention.enable_cleanup);
    }

    #[test]
    fn test_validate_event_sampling() {
        let config = TelemetryConfig::default();
        let policy = TelemetryPolicy::new(config);

        // Valid sampling rate
        assert!(policy
            .validate_event_sampling("router.decision", 1.0)
            .is_ok());

        // Invalid sampling rate
        assert!(policy
            .validate_event_sampling("router.decision", 0.5)
            .is_err());
    }

    #[test]
    fn test_should_sample_event() {
        let config = TelemetryConfig::default();
        let policy = TelemetryPolicy::new(config);

        // Should always sample first N tokens for router decisions
        assert!(policy.should_sample_event("router.decision", 50));
        assert!(policy.should_sample_event("router.decision", 128));

        // Should sample based on rate for other events
        assert!(policy.should_sample_event("policy.abstain", 100));
    }

    #[test]
    fn test_validate_bundle_size() {
        let config = TelemetryConfig::default();
        let policy = TelemetryPolicy::new(config);

        let valid_bundle = TelemetryBundle {
            bundle_id: "bundle1".to_string(),
            timestamp: Utc::now(),
            events: vec![], // Empty bundle
            bundle_hash: "hash1".to_string(),
            merkle_root: "root1".to_string(),
            signature: Some("sig1".to_string()),
        };

        assert!(policy.validate_bundle_size(&valid_bundle).is_ok());

        // Test with too many events (simplified)
        let mut large_bundle = valid_bundle.clone();
        let test_event = TelemetryEvent {
            id: "test".to_string(),
            timestamp: chrono::Utc::now(),
            event_type: "test".to_string(),
            level: adapteros_telemetry::LogLevel::Info,
            message: "test".to_string(),
            component: None,
            identity: IdentityEnvelope::new(
                "test".to_string(),
                "test".to_string(),
                "test".to_string(),
                "test".to_string(),
            ),
            user_id: None,
            metadata: Some(serde_json::Value::Null),
            trace_id: None,
            span_id: None,
            hash: Some("hash".to_string()),
            sampling_rate: Some(1.0),
        };
        large_bundle.events = vec![test_event; 600000]; // Exceeds max_events

        assert!(policy.validate_bundle_size(&large_bundle).is_err());
    }

    #[test]
    fn test_validate_bundle_signing() {
        let config = TelemetryConfig::default();
        let policy = TelemetryPolicy::new(config);

        let signed_bundle = TelemetryBundle {
            bundle_id: "bundle1".to_string(),
            timestamp: Utc::now(),
            events: vec![],
            bundle_hash: "hash1".to_string(),
            merkle_root: "root1".to_string(),
            signature: Some("sig1".to_string()),
        };

        assert!(policy.validate_bundle_signing(&signed_bundle).is_ok());

        let unsigned_bundle = TelemetryBundle {
            bundle_id: "bundle1".to_string(),
            timestamp: Utc::now(),
            events: vec![],
            bundle_hash: "hash1".to_string(),
            merkle_root: "root1".to_string(),
            signature: None,
        };

        assert!(policy.validate_bundle_signing(&unsigned_bundle).is_err());
    }

    #[test]
    fn test_validate_schema_hash() {
        let config = TelemetryConfig::default();
        let policy = TelemetryPolicy::new(config);

        // Valid hash
        assert!(policy.validate_schema_hash("b3:default").is_ok());

        // Invalid hash
        assert!(policy.validate_schema_hash("b3:different").is_err());
    }

    #[test]
    fn test_validate_retention_policy() {
        let config = TelemetryConfig::default();
        let policy = TelemetryPolicy::new(config);

        // Recent event
        let recent_event = Utc::now();
        assert!(policy.validate_retention_policy(recent_event).is_ok());

        // Old event
        let old_event = Utc::now() - chrono::Duration::days(31);
        assert!(policy.validate_retention_policy(old_event).is_err());
    }

    #[test]
    fn test_should_rotate_bundle() {
        let config = TelemetryConfig::default();
        let policy = TelemetryPolicy::new(config);

        let recent_bundle = TelemetryBundle {
            bundle_id: "bundle1".to_string(),
            timestamp: Utc::now(),
            events: vec![],
            bundle_hash: "hash1".to_string(),
            merkle_root: "root1".to_string(),
            signature: Some("sig1".to_string()),
        };

        assert!(!policy.should_rotate_bundle(&recent_bundle));

        let old_bundle = TelemetryBundle {
            bundle_id: "bundle1".to_string(),
            timestamp: Utc::now() - chrono::Duration::hours(25),
            events: vec![],
            bundle_hash: "hash1".to_string(),
            merkle_root: "root1".to_string(),
            signature: Some("sig1".to_string()),
        };

        assert!(policy.should_rotate_bundle(&old_bundle));
    }

    #[test]
    fn test_calculate_hashes() {
        let config = TelemetryConfig::default();
        let policy = TelemetryPolicy::new(config);

        let event = PolicyTelemetryView {
            event_type: "test".to_string(),
            timestamp: Utc::now(),
            data: serde_json::Value::Null,
            hash: "hash".to_string(),
            sampling_rate: 1.0,
        };

        let event_hash = policy.calculate_event_hash(&event);
        assert!(event_hash.starts_with("b3:"));

        let bundle = TelemetryBundle {
            bundle_id: "bundle1".to_string(),
            timestamp: Utc::now(),
            events: vec![TelemetryEvent {
                id: "test".to_string(),
                timestamp: Utc::now(),
                event_type: "test".to_string(),
                level: adapteros_telemetry::LogLevel::Info,
                message: "test".to_string(),
                component: None,
                identity: IdentityEnvelope::new(
                    "test".to_string(),
                    "test".to_string(),
                    "test".to_string(),
                    "test".to_string(),
                ),
                user_id: None,
                metadata: Some(serde_json::Value::Null),
                trace_id: None,
                span_id: None,
                hash: Some("hash".to_string()),
                sampling_rate: Some(1.0),
            }],
            bundle_hash: "hash1".to_string(),
            merkle_root: "root1".to_string(),
            signature: Some("sig1".to_string()),
        };

        let bundle_hash = policy.calculate_bundle_hash(&bundle);
        assert!(bundle_hash.starts_with("b3:"));

        let merkle_root = policy.calculate_merkle_root(&bundle);
        assert!(merkle_root.starts_with("merkle:"));
    }
}
