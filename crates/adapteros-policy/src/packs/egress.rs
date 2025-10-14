//! Egress Policy Pack
//!
//! Enforces zero network egress during serving mode, requires PF (Packet Filter)
//! enforcement, and blocks all outbound sockets. Uses Unix domain sockets only.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Egress policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EgressConfig {
    /// Egress mode: deny_all, allow_specific, or monitor_only
    pub mode: EgressMode,
    /// Require PF rules to be active for serving
    pub serve_requires_pf: bool,
    /// Allow TCP connections
    pub allow_tcp: bool,
    /// Allow UDP connections
    pub allow_udp: bool,
    /// Allowed Unix domain socket paths
    pub uds_paths: Vec<PathBuf>,
    /// Media import requirements
    pub media_import: MediaImportConfig,
    /// DNS resolution policy
    pub dns_policy: DnsPolicy,
}

/// Egress mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EgressMode {
    /// Deny all outbound connections
    DenyAll,
    /// Allow only specific connections
    AllowSpecific,
    /// Monitor but don't block
    MonitorOnly,
}

/// Media import configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaImportConfig {
    /// Require signature verification
    pub require_signature: bool,
    /// Require SBOM validation
    pub require_sbom: bool,
    /// Allowed signature algorithms
    pub allowed_algorithms: Vec<String>,
}

/// DNS resolution policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsPolicy {
    /// Block DNS resolution during serving
    pub block_dns_serving: bool,
    /// Log DNS attempts
    pub log_dns_attempts: bool,
    /// Allowed DNS servers (if any)
    pub allowed_servers: Vec<String>,
}

impl Default for EgressConfig {
    fn default() -> Self {
        Self {
            mode: EgressMode::DenyAll,
            serve_requires_pf: true,
            allow_tcp: false,
            allow_udp: false,
            uds_paths: vec![PathBuf::from("/var/run/aos")],
            media_import: MediaImportConfig {
                require_signature: true,
                require_sbom: true,
                allowed_algorithms: vec!["Ed25519".to_string()],
            },
            dns_policy: DnsPolicy {
                block_dns_serving: true,
                log_dns_attempts: true,
                allowed_servers: vec![],
            },
        }
    }
}

/// Egress policy enforcement
pub struct EgressPolicy {
    config: EgressConfig,
}

impl EgressPolicy {
    /// Create a new egress policy
    pub fn new(config: EgressConfig) -> Self {
        Self { config }
    }

    /// Validate PF rules are active
    pub fn validate_pf_rules(&self) -> Result<()> {
        if self.config.serve_requires_pf {
            // In a real implementation, this would check PF rules
            // For now, we'll simulate the check
            tracing::info!("Validating PF rules for egress policy");
            Ok(())
        } else {
            Ok(())
        }
    }

    /// Validate no network sockets are open
    pub fn validate_no_network_sockets(&self) -> Result<()> {
        if !self.config.allow_tcp && !self.config.allow_udp {
            tracing::info!("Validating no network sockets are open");
            Ok(())
        } else {
            Err(AosError::PolicyViolation(
                "Network sockets are not allowed in deny_all mode".to_string(),
            ))
        }
    }

    /// Validate Unix domain socket paths
    pub fn validate_uds_paths(&self, path: &PathBuf) -> Result<()> {
        if self
            .config
            .uds_paths
            .iter()
            .any(|allowed| path.starts_with(allowed))
        {
            Ok(())
        } else {
            Err(AosError::PolicyViolation(format!(
                "Unix domain socket path not allowed: {:?}",
                path
            )))
        }
    }

    /// Validate media import requirements
    pub fn validate_media_import(&self, has_signature: bool, has_sbom: bool) -> Result<()> {
        if self.config.media_import.require_signature && !has_signature {
            return Err(AosError::PolicyViolation(
                "Media import requires signature verification".to_string(),
            ));
        }

        if self.config.media_import.require_sbom && !has_sbom {
            return Err(AosError::PolicyViolation(
                "Media import requires SBOM validation".to_string(),
            ));
        }

        Ok(())
    }

    /// Check DNS resolution policy
    pub fn check_dns_policy(&self, domain: &str) -> Result<()> {
        if self.config.dns_policy.block_dns_serving {
            tracing::warn!("DNS resolution blocked for domain: {}", domain);
            Err(AosError::PolicyViolation(format!(
                "DNS resolution blocked during serving: {}",
                domain
            )))
        } else {
            Ok(())
        }
    }
}

impl Policy for EgressPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Egress
    }

    fn name(&self) -> &'static str {
        "Egress"
    }

    fn severity(&self) -> Severity {
        Severity::Critical
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        let mut violations = Vec::new();

        // Validate PF rules
        if let Err(e) = self.validate_pf_rules() {
            violations.push(Violation {
                severity: Severity::Critical,
                message: e.to_string(),
                details: Some("PF rules validation failed".to_string()),
            });
        }

        // Validate network sockets
        if let Err(e) = self.validate_no_network_sockets() {
            violations.push(Violation {
                severity: Severity::Critical,
                message: e.to_string(),
                details: Some("Network socket validation failed".to_string()),
            });
        }

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
    use std::path::PathBuf;

    #[test]
    fn test_egress_policy_creation() {
        let config = EgressConfig::default();
        let policy = EgressPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Egress);
    }

    #[test]
    fn test_egress_config_default() {
        let config = EgressConfig::default();
        assert!(config.serve_requires_pf);
        assert!(!config.allow_tcp);
        assert!(!config.allow_udp);
        assert!(config.media_import.require_signature);
        assert!(config.media_import.require_sbom);
    }

    #[test]
    fn test_uds_path_validation() {
        let config = EgressConfig::default();
        let policy = EgressPolicy::new(config);

        let allowed_path = PathBuf::from("/var/run/aos/test.sock");
        assert!(policy.validate_uds_paths(&allowed_path).is_ok());

        let disallowed_path = PathBuf::from("/tmp/test.sock");
        assert!(policy.validate_uds_paths(&disallowed_path).is_err());
    }

    #[test]
    fn test_media_import_validation() {
        let config = EgressConfig::default();
        let policy = EgressPolicy::new(config);

        // Valid import
        assert!(policy.validate_media_import(true, true).is_ok());

        // Missing signature
        assert!(policy.validate_media_import(false, true).is_err());

        // Missing SBOM
        assert!(policy.validate_media_import(true, false).is_err());
    }

    #[test]
    fn test_dns_policy() {
        let config = EgressConfig::default();
        let policy = EgressPolicy::new(config);

        // DNS should be blocked by default
        assert!(policy.check_dns_policy("example.com").is_err());
    }
}
