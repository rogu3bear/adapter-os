//! Identity envelope for events and logs
//!
//! Provides a typed structure ensuring every event and log carries complete identity information.
//! All fields are required - no optional values.
//!
//! # Citations
//! - PRD 1: Global Identity Envelope for Events & Logs

use crate::B3Hash;
use serde::{Deserialize, Serialize};

/// Domain of operation - canonical taxonomy for all AdapterOS subsystems
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Domain {
    Router,
    Worker,
    Lifecycle,
    Telemetry,
    Policy,
    Plugin,
    CLI,
}

impl Domain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Domain::Router => "router",
            Domain::Worker => "worker",
            Domain::Lifecycle => "lifecycle",
            Domain::Telemetry => "telemetry",
            Domain::Policy => "policy",
            Domain::Plugin => "plugin",
            Domain::CLI => "cli",
        }
    }
}

impl std::fmt::Display for Domain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Purpose of operation - why the operation is happening
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Purpose {
    Inference,
    Training,
    Replay,
    Maintenance,
    PluginIO,
    Audit,
}

impl Purpose {
    pub fn as_str(&self) -> &'static str {
        match self {
            Purpose::Inference => "inference",
            Purpose::Training => "training",
            Purpose::Replay => "replay",
            Purpose::Maintenance => "maintenance",
            Purpose::PluginIO => "plugin_io",
            Purpose::Audit => "audit",
        }
    }
}

impl std::fmt::Display for Purpose {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Identity envelope containing required context for all events and logs
///
/// # Invariants (PRD 1)
/// - Every TelemetryEvent MUST have a non-empty tenant_id
/// - domain and purpose MUST be from the enums above
/// - revision MUST equal the process build hash for this binary
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentityEnvelope {
    /// Tenant identifier (e.g., "tenant-a")
    pub tenant_id: String,

    /// Domain of the operation (typed enum)
    pub domain: Domain,

    /// Purpose of the operation (typed enum)
    pub purpose: Purpose,

    /// Revision identifier (git hash or build hash)
    pub revision: B3Hash,
}

impl IdentityEnvelope {
    /// Create a new identity envelope with typed enums
    pub fn new(tenant_id: String, domain: Domain, purpose: Purpose, revision: B3Hash) -> Self {
        Self {
            tenant_id,
            domain,
            purpose,
            revision,
        }
    }

    /// Create identity envelope from strings (for backward compatibility)
    ///
    /// # Errors
    /// Returns error if domain or purpose strings don't match enum variants
    pub fn from_strings(
        tenant_id: String,
        domain: &str,
        purpose: &str,
        revision_str: &str,
    ) -> Result<Self, &'static str> {
        let domain = match domain {
            "router" => Domain::Router,
            "worker" => Domain::Worker,
            "lifecycle" => Domain::Lifecycle,
            "telemetry" => Domain::Telemetry,
            "policy" => Domain::Policy,
            "plugin" => Domain::Plugin,
            "cli" => Domain::CLI,
            _ => return Err("Invalid domain string"),
        };

        let purpose = match purpose {
            "inference" => Purpose::Inference,
            "training" => Purpose::Training,
            "replay" => Purpose::Replay,
            "maintenance" => Purpose::Maintenance,
            "plugin_io" => Purpose::PluginIO,
            "audit" => Purpose::Audit,
            _ => return Err("Invalid purpose string"),
        };

        // Parse revision string as B3Hash
        let revision = B3Hash::hash(revision_str.as_bytes());

        Ok(Self::new(tenant_id, domain, purpose, revision))
    }

    /// Validate the envelope fields (basic non-empty check per PRD 1)
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.tenant_id.is_empty() {
            return Err("tenant_id cannot be empty");
        }
        // Domain and Purpose are guaranteed valid by type system
        // Revision (B3Hash) cannot be empty by construction
        Ok(())
    }

    /// Create default revision from environment AOS_REVISION or git commit hash
    pub fn default_revision() -> B3Hash {
        let rev_str = std::env::var("AOS_REVISION").unwrap_or_else(|_| {
            // Fallback to git rev-parse HEAD if in git repo
            if let Ok(output) = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
            {
                if output.status.success() {
                    String::from_utf8_lossy(&output.stdout).trim().to_string()
                } else {
                    "unknown".to_string()
                }
            } else {
                "unknown".to_string()
            }
        });

        // Hash the revision string to create a B3Hash
        B3Hash::hash(rev_str.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_enum() {
        assert_eq!(Domain::Router.as_str(), "router");
        assert_eq!(Domain::Worker.as_str(), "worker");
        assert_eq!(Domain::Lifecycle.as_str(), "lifecycle");
        assert_eq!(Domain::Telemetry.as_str(), "telemetry");
        assert_eq!(Domain::Policy.as_str(), "policy");
        assert_eq!(Domain::Plugin.as_str(), "plugin");
        assert_eq!(Domain::CLI.as_str(), "cli");
    }

    #[test]
    fn test_purpose_enum() {
        assert_eq!(Purpose::Inference.as_str(), "inference");
        assert_eq!(Purpose::Training.as_str(), "training");
        assert_eq!(Purpose::Replay.as_str(), "replay");
        assert_eq!(Purpose::Maintenance.as_str(), "maintenance");
        assert_eq!(Purpose::PluginIO.as_str(), "plugin_io");
        assert_eq!(Purpose::Audit.as_str(), "audit");
    }

    #[test]
    fn test_identity_envelope_creation() {
        let revision = B3Hash::hash(b"test-revision");
        let envelope = IdentityEnvelope::new(
            "tenant-a".to_string(),
            Domain::Router,
            Purpose::Inference,
            revision,
        );

        assert_eq!(envelope.tenant_id, "tenant-a");
        assert_eq!(envelope.domain, Domain::Router);
        assert_eq!(envelope.purpose, Purpose::Inference);
        assert_eq!(envelope.revision, revision);
    }

    #[test]
    fn test_identity_envelope_from_strings() {
        let envelope =
            IdentityEnvelope::from_strings("tenant-a".to_string(), "router", "inference", "v1.0.0")
                .unwrap();

        assert_eq!(envelope.tenant_id, "tenant-a");
        assert_eq!(envelope.domain, Domain::Router);
        assert_eq!(envelope.purpose, Purpose::Inference);
    }

    #[test]
    fn test_identity_envelope_validation() {
        let revision = B3Hash::hash(b"test");
        let valid = IdentityEnvelope::new(
            "tenant-a".to_string(),
            Domain::Router,
            Purpose::Inference,
            revision,
        );
        assert!(valid.validate().is_ok());

        let invalid = IdentityEnvelope::new(
            "".to_string(), // Empty tenant_id
            Domain::Router,
            Purpose::Inference,
            revision,
        );
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_invalid_domain_string() {
        let result =
            IdentityEnvelope::from_strings("tenant-a".to_string(), "invalid", "inference", "v1.0.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_purpose_string() {
        let result =
            IdentityEnvelope::from_strings("tenant-a".to_string(), "router", "invalid", "v1.0.0");
        assert!(result.is_err());
    }

    /// Golden test for serialized envelope format (PRD 1 requirement)
    #[test]
    fn test_golden_serialized_format() {
        let revision = B3Hash::hash(b"test-rev-123");
        let envelope = IdentityEnvelope::new(
            "tenant-a".to_string(),
            Domain::Router,
            Purpose::Inference,
            revision,
        );

        let serialized = serde_json::to_string(&envelope).unwrap();
        let deserialized: IdentityEnvelope = serde_json::from_str(&serialized).unwrap();

        // Verify round-trip
        assert_eq!(envelope, deserialized);

        // Verify JSON structure contains required fields
        let json: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert!(json.get("tenant_id").is_some());
        assert!(json.get("domain").is_some());
        assert!(json.get("purpose").is_some());
        assert!(json.get("revision").is_some());

        // Verify values
        assert_eq!(json["tenant_id"], "tenant-a");
        assert_eq!(json["domain"], "Router");
        assert_eq!(json["purpose"], "Inference");
    }

    /// Test that empty tenant_id is rejected (PRD 1 invariant)
    #[test]
    fn test_empty_tenant_id_rejected() {
        let envelope = IdentityEnvelope::new(
            "".to_string(),
            Domain::Worker,
            Purpose::Training,
            B3Hash::hash(b"test"),
        );

        let result = envelope.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "tenant_id cannot be empty");
    }

    /// Test that all Domain enum variants serialize correctly
    #[test]
    fn test_domain_serialization() {
        let domains = vec![
            Domain::Router,
            Domain::Worker,
            Domain::Lifecycle,
            Domain::Telemetry,
            Domain::Policy,
            Domain::Plugin,
            Domain::CLI,
        ];

        for domain in domains {
            let envelope = IdentityEnvelope::new(
                "test".to_string(),
                domain,
                Purpose::Maintenance,
                B3Hash::hash(b"test"),
            );
            let serialized = serde_json::to_string(&envelope).unwrap();
            let deserialized: IdentityEnvelope = serde_json::from_str(&serialized).unwrap();
            assert_eq!(envelope, deserialized);
        }
    }

    /// Test that all Purpose enum variants serialize correctly
    #[test]
    fn test_purpose_serialization() {
        let purposes = vec![
            Purpose::Inference,
            Purpose::Training,
            Purpose::Replay,
            Purpose::Maintenance,
            Purpose::PluginIO,
            Purpose::Audit,
        ];

        for purpose in purposes {
            let envelope = IdentityEnvelope::new(
                "test".to_string(),
                Domain::Worker,
                purpose,
                B3Hash::hash(b"test"),
            );
            let serialized = serde_json::to_string(&envelope).unwrap();
            let deserialized: IdentityEnvelope = serde_json::from_str(&serialized).unwrap();
            assert_eq!(envelope, deserialized);
        }
    }

    /// Test default_revision produces valid B3Hash
    #[test]
    fn test_default_revision_is_valid() {
        let revision = IdentityEnvelope::default_revision();
        // B3Hash should have 32 bytes
        assert_eq!(revision.as_bytes().len(), 32);
    }
}
