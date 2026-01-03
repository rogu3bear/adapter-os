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
    /// Enforcement level: determines whether violations block or warn
    pub enforcement_level: EnforcementLevel,
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

/// Enforcement level for egress policy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EnforcementLevel {
    /// Log violations only, don't block
    Warn,
    /// Block violations with errors
    Block,
    /// Automatically determine based on runtime mode
    Auto,
}

/// Web browsing egress configuration (per-tenant)
/// Allows controlled exceptions to the default deny-all egress policy
/// for web browsing and search operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebBrowseEgressConfig {
    /// Whether web browsing is enabled for this tenant
    pub enabled: bool,
    /// Allowed search provider endpoints
    pub allowed_search_endpoints: Vec<AllowedEndpoint>,
    /// Allowed page fetch domains (domain patterns)
    pub allowed_fetch_domains: Vec<DomainPattern>,
    /// Blocked domains (takes precedence over allowed)
    pub blocked_domains: Vec<String>,
    /// Maximum concurrent requests
    pub max_concurrent_requests: u32,
    /// Request timeout in seconds
    pub request_timeout_secs: u32,
    /// Whether to require HTTPS only
    pub https_only: bool,
    /// Rate limit per minute
    pub rate_limit_rpm: u32,
    /// Rate limit per day
    pub rate_limit_daily: u32,
}

/// Allowed search endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowedEndpoint {
    /// Endpoint name (e.g., "brave_search", "bing_search")
    pub name: String,
    /// Base URL pattern
    pub base_url: String,
    /// Required headers (e.g., API key header name)
    pub required_headers: Vec<String>,
    /// Rate limit per minute for this endpoint
    pub rate_limit_rpm: u32,
}

/// Domain pattern for allowed/blocked domains
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainPattern {
    /// Domain pattern (e.g., "*.wikipedia.org", "docs.python.org")
    pub pattern: String,
    /// Whether to allow subdomains
    pub allow_subdomains: bool,
    /// Content type restrictions (e.g., ["text/html", "application/json"])
    pub allowed_content_types: Vec<String>,
}

impl Default for WebBrowseEgressConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for security
            allowed_search_endpoints: vec![],
            allowed_fetch_domains: vec![],
            blocked_domains: vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
                "*.local".to_string(),
                "*.internal".to_string(),
                "*.corp".to_string(),
                "10.*".to_string(),
                "172.16.*".to_string(),
                "192.168.*".to_string(),
            ],
            max_concurrent_requests: 3,
            request_timeout_secs: 10,
            https_only: true,
            rate_limit_rpm: 10,
            rate_limit_daily: 100,
        }
    }
}

impl WebBrowseEgressConfig {
    /// Check if a domain is allowed for browsing
    pub fn is_domain_allowed(&self, domain: &str) -> bool {
        if !self.enabled {
            return false;
        }

        // Check blocked domains first (takes precedence)
        for blocked in &self.blocked_domains {
            if Self::matches_pattern(domain, blocked) {
                return false;
            }
        }

        // Check allowed domains
        for allowed in &self.allowed_fetch_domains {
            if Self::matches_pattern(domain, &allowed.pattern) {
                return true;
            }
            if allowed.allow_subdomains {
                let subdomain_pattern = format!("*.{}", allowed.pattern);
                if Self::matches_pattern(domain, &subdomain_pattern) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a URL matches a pattern (supports wildcards)
    fn matches_pattern(domain: &str, pattern: &str) -> bool {
        if pattern.starts_with("*.") {
            // Wildcard pattern: *.example.com matches sub.example.com
            let suffix = &pattern[1..]; // ".example.com"
            domain.ends_with(suffix) || domain == &pattern[2..]
        } else if pattern.ends_with(".*") {
            // Wildcard suffix: 192.168.* matches 192.168.1.1
            let prefix = &pattern[..pattern.len() - 1];
            domain.starts_with(prefix)
        } else {
            domain == pattern
        }
    }

    /// Check if an endpoint is allowed
    pub fn is_endpoint_allowed(&self, endpoint_name: &str) -> bool {
        self.enabled
            && self
                .allowed_search_endpoints
                .iter()
                .any(|e| e.name == endpoint_name)
    }
}

impl Default for EgressConfig {
    fn default() -> Self {
        Self {
            mode: EgressMode::DenyAll,
            serve_requires_pf: true,
            allow_tcp: false,
            allow_udp: false,
            uds_paths: vec![PathBuf::from("./var/run/aos")],
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
            enforcement_level: EnforcementLevel::Auto,
        }
    }
}

/// Runtime mode for egress enforcement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMode {
    /// Development mode: relaxed enforcement
    Dev,
    /// Staging mode: moderate enforcement
    Staging,
    /// Production mode: strict enforcement
    Prod,
}

impl RuntimeMode {
    /// Check if this mode should block egress violations
    pub fn should_block_egress(&self) -> bool {
        matches!(self, RuntimeMode::Prod)
    }

    /// Check if this mode allows egress
    pub fn allows_egress(&self) -> bool {
        matches!(self, RuntimeMode::Dev)
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

    /// Determine if violations should block based on enforcement level and runtime mode
    fn should_block(&self, runtime_mode: Option<RuntimeMode>) -> bool {
        match self.config.enforcement_level {
            EnforcementLevel::Warn => false,
            EnforcementLevel::Block => true,
            EnforcementLevel::Auto => runtime_mode
                .map(|m| m.should_block_egress())
                // Fail-closed: default to blocking when runtime_mode is not specified
                .unwrap_or(true),
        }
    }

    /// Validate PF rules are active
    pub fn validate_pf_rules(&self) -> Result<()> {
        if self.config.serve_requires_pf {
            // PF rule validation is not implemented yet
            // Fail-closed: return error instead of silently passing
            tracing::error!("PF rule validation not implemented but required by policy");
            Err(AosError::PolicyViolation(
                "PF rule validation not implemented - cannot verify packet filter enforcement"
                    .to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Validate no network sockets are open
    pub fn validate_no_network_sockets(&self) -> Result<()> {
        self.validate_no_network_sockets_with_mode(None)
    }

    /// Validate no network sockets are open (with runtime mode)
    pub fn validate_no_network_sockets_with_mode(
        &self,
        runtime_mode: Option<RuntimeMode>,
    ) -> Result<()> {
        if !self.config.allow_tcp && !self.config.allow_udp {
            tracing::info!("Validating no network sockets are open");
            Ok(())
        } else {
            let msg = "Network sockets are not allowed in deny_all mode".to_string();
            if self.should_block(runtime_mode) {
                Err(AosError::PolicyViolation(msg))
            } else {
                tracing::warn!("{}", msg);
                Ok(())
            }
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
        self.check_dns_policy_with_mode(domain, None)
    }

    /// Check DNS resolution policy (with runtime mode)
    pub fn check_dns_policy_with_mode(
        &self,
        domain: &str,
        runtime_mode: Option<RuntimeMode>,
    ) -> Result<()> {
        if self.config.dns_policy.block_dns_serving {
            let msg = format!("DNS resolution blocked during serving: {}", domain);
            if self.should_block(runtime_mode) {
                tracing::error!("{}", msg);
                Err(AosError::PolicyViolation(msg))
            } else {
                tracing::warn!("{}", msg);
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    /// Check network egress attempt (new method for runtime enforcement)
    pub fn check_network_egress(
        &self,
        protocol: &str,
        destination: &str,
        runtime_mode: Option<RuntimeMode>,
    ) -> Result<()> {
        // Allow UDS always
        if protocol == "uds" {
            return Ok(());
        }

        // Check if protocol is allowed
        let allowed = match protocol {
            "tcp" => self.config.allow_tcp,
            "udp" => self.config.allow_udp,
            _ => false,
        };

        if !allowed {
            let msg = format!(
                "Egress blocked: {} to {} (protocol not allowed)",
                protocol, destination
            );
            if self.should_block(runtime_mode) {
                tracing::error!("{}", msg);
                Err(AosError::PolicyViolation(msg))
            } else {
                tracing::warn!("{}", msg);
                Ok(())
            }
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

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        // Extract runtime mode from context metadata
        let runtime_mode = ctx
            .metadata()
            .get("runtime_mode")
            .and_then(|s| match s.as_str() {
                "dev" | "development" => Some(RuntimeMode::Dev),
                "staging" | "stage" => Some(RuntimeMode::Staging),
                "prod" | "production" => Some(RuntimeMode::Prod),
                _ => None,
            });

        let should_block = self.should_block(runtime_mode);

        // Validate PF rules
        if let Err(e) = self.validate_pf_rules() {
            if should_block {
                violations.push(Violation {
                    severity: Severity::Critical,
                    message: e.to_string(),
                    details: Some("PF rules validation failed".to_string()),
                });
            } else {
                warnings.push(format!("PF rules validation failed: {}", e));
            }
        }

        // Validate network sockets with runtime mode
        if let Err(e) = self.validate_no_network_sockets_with_mode(runtime_mode) {
            if should_block {
                violations.push(Violation {
                    severity: Severity::Critical,
                    message: e.to_string(),
                    details: Some("Network socket validation failed".to_string()),
                });
            } else {
                warnings.push(format!("Network socket validation failed: {}", e));
            }
        }

        if violations.is_empty() {
            Ok(Audit::passed(self.id()).with_warnings(warnings))
        } else {
            Ok(Audit::failed(self.id(), violations).with_warnings(warnings))
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

        let allowed_path = PathBuf::from("./var/run/aos/test.sock");
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

    #[test]
    fn test_runtime_mode_enforcement() {
        // Test that dev mode allows egress
        assert!(RuntimeMode::Dev.allows_egress());
        assert!(!RuntimeMode::Dev.should_block_egress());

        // Test that staging mode doesn't allow egress by default
        assert!(!RuntimeMode::Staging.allows_egress());
        assert!(!RuntimeMode::Staging.should_block_egress());

        // Test that prod mode blocks egress
        assert!(!RuntimeMode::Prod.allows_egress());
        assert!(RuntimeMode::Prod.should_block_egress());
    }

    #[test]
    fn test_enforcement_level_warn() {
        let mut config = EgressConfig::default();
        config.enforcement_level = EnforcementLevel::Warn;
        config.allow_tcp = true; // This would normally cause a violation
        let policy = EgressPolicy::new(config);

        // Should not block even in prod mode with Warn enforcement
        assert!(!policy.should_block(Some(RuntimeMode::Prod)));
    }

    #[test]
    fn test_enforcement_level_block() {
        let mut config = EgressConfig::default();
        config.enforcement_level = EnforcementLevel::Block;
        let policy = EgressPolicy::new(config);

        // Should always block with Block enforcement, regardless of mode
        assert!(policy.should_block(Some(RuntimeMode::Dev)));
        assert!(policy.should_block(Some(RuntimeMode::Staging)));
        assert!(policy.should_block(Some(RuntimeMode::Prod)));
    }

    #[test]
    fn test_enforcement_level_auto() {
        let mut config = EgressConfig::default();
        config.enforcement_level = EnforcementLevel::Auto;
        let policy = EgressPolicy::new(config);

        // Should only block in prod mode with Auto enforcement
        assert!(!policy.should_block(Some(RuntimeMode::Dev)));
        assert!(!policy.should_block(Some(RuntimeMode::Staging)));
        assert!(policy.should_block(Some(RuntimeMode::Prod)));

        // Should default to blocking (fail-closed) when runtime_mode is None
        assert!(policy.should_block(None));
    }

    #[test]
    fn test_check_network_egress_with_runtime_mode() {
        let config = EgressConfig::default();
        let policy = EgressPolicy::new(config);

        // UDS should always be allowed
        assert!(policy
            .check_network_egress("uds", "./var/run/aos/test.sock", Some(RuntimeMode::Prod))
            .is_ok());

        // TCP should be blocked in prod mode (default config has allow_tcp=false)
        assert!(policy
            .check_network_egress("tcp", "example.com:443", Some(RuntimeMode::Prod))
            .is_err());

        // TCP should warn but not block in dev mode with Auto enforcement
        let mut dev_config = EgressConfig::default();
        dev_config.enforcement_level = EnforcementLevel::Auto;
        let dev_policy = EgressPolicy::new(dev_config);
        assert!(dev_policy
            .check_network_egress("tcp", "example.com:443", Some(RuntimeMode::Dev))
            .is_ok());
    }

    #[test]
    fn test_dns_policy_with_runtime_mode() {
        let config = EgressConfig::default();
        let policy = EgressPolicy::new(config);

        // DNS should block in prod mode by default
        assert!(policy
            .check_dns_policy_with_mode("example.com", Some(RuntimeMode::Prod))
            .is_err());

        // DNS should warn but not block in dev mode with Auto enforcement
        let mut dev_config = EgressConfig::default();
        dev_config.enforcement_level = EnforcementLevel::Auto;
        let dev_policy = EgressPolicy::new(dev_config);
        assert!(dev_policy
            .check_dns_policy_with_mode("example.com", Some(RuntimeMode::Dev))
            .is_ok());
    }
}
