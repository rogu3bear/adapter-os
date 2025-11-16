//! Naming Policy Pack
//!
//! Enforces adapter and stack naming conventions to prevent naming chaos,
//! validate reserved namespaces, and ensure semantic consistency.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::{AdapterName, AosError, Result, StackName};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Naming policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamingConfig {
    /// Enforce profanity filtering
    pub enforce_profanity_filter: bool,
    /// Enforce reserved namespace validation
    pub enforce_reserved_namespaces: bool,
    /// Enforce tenant isolation in naming
    pub enforce_tenant_isolation: bool,
    /// Maximum revision gap allowed
    pub max_revision_gap: u32,
    /// Profanity word list (basic filtering)
    pub profanity_list: HashSet<String>,
    /// Additional reserved tenants beyond core list
    pub additional_reserved_tenants: HashSet<String>,
    /// Additional reserved domains beyond core list
    pub additional_reserved_domains: HashSet<String>,
    /// Require semantic names for all adapters
    pub require_semantic_names: bool,
    /// Enforce strict hierarchical naming
    pub enforce_hierarchy: bool,
}

impl Default for NamingConfig {
    fn default() -> Self {
        Self {
            enforce_profanity_filter: true,
            enforce_reserved_namespaces: true,
            enforce_tenant_isolation: true,
            max_revision_gap: 5,
            profanity_list: Self::default_profanity_list(),
            additional_reserved_tenants: HashSet::new(),
            additional_reserved_domains: HashSet::new(),
            require_semantic_names: false, // Optional for backward compatibility
            enforce_hierarchy: true,
        }
    }
}

impl NamingConfig {
    /// Default profanity word list (basic filtering)
    fn default_profanity_list() -> HashSet<String> {
        // Basic profanity list - in production, this should be more comprehensive
        // and possibly sourced from a maintained blocklist
        vec![
            "offensive1", "offensive2", "badword1", "badword2",
            // Add more as needed - using placeholders for demonstration
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect()
    }

    /// Check if a string contains profanity
    pub fn contains_profanity(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        self.profanity_list.iter().any(|word| lower.contains(word))
    }

    /// Get all reserved tenants (core + additional)
    pub fn all_reserved_tenants(&self) -> HashSet<String> {
        let mut reserved = adapteros_core::naming::RESERVED_TENANTS
            .iter()
            .map(|s| s.to_string())
            .collect::<HashSet<String>>();
        reserved.extend(self.additional_reserved_tenants.clone());
        reserved
    }

    /// Get all reserved domains (core + additional)
    pub fn all_reserved_domains(&self) -> HashSet<String> {
        let mut reserved = adapteros_core::naming::RESERVED_DOMAINS
            .iter()
            .map(|s| s.to_string())
            .collect::<HashSet<String>>();
        reserved.extend(self.additional_reserved_domains.clone());
        reserved
    }
}

/// Adapter name validation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterNameValidation {
    /// Adapter name to validate
    pub name: String,
    /// Tenant requesting validation
    pub tenant_id: String,
    /// Parent adapter name (if forking)
    pub parent_name: Option<String>,
    /// Latest revision in lineage (if extending)
    pub latest_revision: Option<u32>,
}

/// Stack name validation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackNameValidation {
    /// Stack name to validate
    pub name: String,
    /// Tenant requesting validation
    pub tenant_id: String,
}

/// Naming policy violation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamingViolation {
    /// Violation type
    pub violation_type: NamingViolationType,
    /// Component that violated policy
    pub component: String,
    /// Detailed reason
    pub reason: String,
    /// Suggested fix
    pub suggestion: Option<String>,
}

/// Types of naming violations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NamingViolationType {
    /// Profanity detected
    Profanity,
    /// Reserved namespace used
    ReservedNamespace,
    /// Tenant isolation violated
    TenantIsolation,
    /// Revision gap too large
    RevisionGap,
    /// Invalid format
    InvalidFormat,
    /// Hierarchical structure violated
    HierarchyViolation,
}

/// Naming policy enforcement
pub struct NamingPolicy {
    config: NamingConfig,
}

impl NamingPolicy {
    /// Create a new naming policy
    pub fn new(config: NamingConfig) -> Self {
        Self { config }
    }

    /// Validate adapter name
    pub fn validate_adapter_name(&self, request: &AdapterNameValidation) -> Result<()> {
        // Parse the name to validate format
        let adapter_name = AdapterName::parse(&request.name)
            .map_err(|e| AosError::PolicyViolation(format!("Invalid adapter name format: {}", e)))?;

        // Check profanity if enabled
        if self.config.enforce_profanity_filter {
            for component in [
                adapter_name.tenant(),
                adapter_name.domain(),
                adapter_name.purpose(),
            ] {
                if self.config.contains_profanity(component) {
                    return Err(AosError::PolicyViolation(format!(
                        "Profanity detected in name component: {}",
                        component
                    )));
                }
            }
        }

        // Check reserved namespaces if enabled
        if self.config.enforce_reserved_namespaces {
            let reserved_tenants = self.config.all_reserved_tenants();
            if reserved_tenants.contains(adapter_name.tenant()) {
                return Err(AosError::PolicyViolation(format!(
                    "Reserved tenant name: {}",
                    adapter_name.tenant()
                )));
            }

            let reserved_domains = self.config.all_reserved_domains();
            if reserved_domains.contains(adapter_name.domain()) {
                return Err(AosError::PolicyViolation(format!(
                    "Reserved domain name: {}",
                    adapter_name.domain()
                )));
            }
        }

        // Check tenant isolation if enabled
        if self.config.enforce_tenant_isolation {
            if adapter_name.tenant() != request.tenant_id {
                return Err(AosError::PolicyViolation(format!(
                    "Tenant mismatch: adapter tenant '{}' does not match requesting tenant '{}'",
                    adapter_name.tenant(),
                    request.tenant_id
                )));
            }
        }

        // Check revision gap if latest revision provided
        if let Some(latest_rev) = request.latest_revision {
            let new_rev = adapter_name.revision_number().map_err(|e| {
                AosError::PolicyViolation(format!("Failed to parse revision number: {}", e))
            })?;

            if new_rev <= latest_rev {
                return Err(AosError::PolicyViolation(format!(
                    "Revision monotonicity violation: new revision {} must be greater than latest {}",
                    new_rev, latest_rev
                )));
            }

            let gap = new_rev - latest_rev;
            if gap > self.config.max_revision_gap {
                return Err(AosError::PolicyViolation(format!(
                    "Revision gap {} exceeds maximum allowed gap of {}",
                    gap, self.config.max_revision_gap
                )));
            }
        }

        // Check hierarchy if parent name provided and hierarchy enforcement enabled
        if self.config.enforce_hierarchy {
            if let Some(parent_name_str) = &request.parent_name {
                let parent_name = AdapterName::parse(parent_name_str).map_err(|e| {
                    AosError::PolicyViolation(format!("Invalid parent name format: {}", e))
                })?;

                // Ensure parent and child are in same lineage (tenant/domain/purpose must match)
                if !adapter_name.is_same_lineage(&parent_name) {
                    return Err(AosError::PolicyViolation(format!(
                        "Hierarchy violation: child '{}' is not in same lineage as parent '{}'",
                        adapter_name, parent_name
                    )));
                }
            }
        }

        Ok(())
    }

    /// Validate stack name
    pub fn validate_stack_name(&self, request: &StackNameValidation) -> Result<()> {
        // Parse the stack name to validate format
        let stack_name = StackName::parse(&request.name)
            .map_err(|e| AosError::PolicyViolation(format!("Invalid stack name format: {}", e)))?;

        // Check profanity if enabled
        if self.config.enforce_profanity_filter {
            if self.config.contains_profanity(stack_name.namespace()) {
                return Err(AosError::PolicyViolation(format!(
                    "Profanity detected in stack namespace: {}",
                    stack_name.namespace()
                )));
            }

            if let Some(identifier) = stack_name.identifier() {
                if self.config.contains_profanity(identifier) {
                    return Err(AosError::PolicyViolation(format!(
                        "Profanity detected in stack identifier: {}",
                        identifier
                    )));
                }
            }
        }

        Ok(())
    }

    /// Get policy configuration
    pub fn config(&self) -> &NamingConfig {
        &self.config
    }

    /// Analyze naming violations (returns all violations found)
    pub fn analyze_adapter_name(&self, request: &AdapterNameValidation) -> Vec<NamingViolation> {
        let mut violations = Vec::new();

        // Try to parse the name
        let adapter_name = match AdapterName::parse(&request.name) {
            Ok(name) => name,
            Err(e) => {
                violations.push(NamingViolation {
                    violation_type: NamingViolationType::InvalidFormat,
                    component: request.name.clone(),
                    reason: format!("Invalid adapter name format: {}", e),
                    suggestion: Some(
                        "Use format: {tenant}/{domain}/{purpose}/{revision} (e.g., shop-floor/hydraulics/troubleshooting/r001)".to_string(),
                    ),
                });
                return violations;
            }
        };

        // Check profanity
        if self.config.enforce_profanity_filter {
            for component in [
                adapter_name.tenant(),
                adapter_name.domain(),
                adapter_name.purpose(),
            ] {
                if self.config.contains_profanity(component) {
                    violations.push(NamingViolation {
                        violation_type: NamingViolationType::Profanity,
                        component: component.to_string(),
                        reason: format!("Profanity detected in component: {}", component),
                        suggestion: Some("Choose a professional name without offensive terms".to_string()),
                    });
                }
            }
        }

        // Check reserved namespaces
        if self.config.enforce_reserved_namespaces {
            let reserved_tenants = self.config.all_reserved_tenants();
            if reserved_tenants.contains(adapter_name.tenant()) {
                violations.push(NamingViolation {
                    violation_type: NamingViolationType::ReservedNamespace,
                    component: adapter_name.tenant().to_string(),
                    reason: format!("Reserved tenant name: {}", adapter_name.tenant()),
                    suggestion: Some("Choose a different tenant name".to_string()),
                });
            }

            let reserved_domains = self.config.all_reserved_domains();
            if reserved_domains.contains(adapter_name.domain()) {
                violations.push(NamingViolation {
                    violation_type: NamingViolationType::ReservedNamespace,
                    component: adapter_name.domain().to_string(),
                    reason: format!("Reserved domain name: {}", adapter_name.domain()),
                    suggestion: Some("Choose a different domain name".to_string()),
                });
            }
        }

        // Check tenant isolation
        if self.config.enforce_tenant_isolation
            && adapter_name.tenant() != request.tenant_id
        {
            violations.push(NamingViolation {
                violation_type: NamingViolationType::TenantIsolation,
                component: adapter_name.tenant().to_string(),
                reason: format!(
                    "Tenant mismatch: adapter tenant '{}' does not match requesting tenant '{}'",
                    adapter_name.tenant(),
                    request.tenant_id
                ),
                suggestion: Some(format!(
                    "Use tenant '{}' in the adapter name",
                    request.tenant_id
                )),
            });
        }

        // Check revision gap
        if let Some(latest_rev) = request.latest_revision {
            if let Ok(new_rev) = adapter_name.revision_number() {
                if new_rev <= latest_rev {
                    violations.push(NamingViolation {
                        violation_type: NamingViolationType::RevisionGap,
                        component: adapter_name.revision().to_string(),
                        reason: format!(
                            "Revision monotonicity violation: new revision {} must be greater than latest {}",
                            new_rev, latest_rev
                        ),
                        suggestion: Some(format!("Use revision r{:03}", latest_rev + 1)),
                    });
                } else {
                    let gap = new_rev - latest_rev;
                    if gap > self.config.max_revision_gap {
                        violations.push(NamingViolation {
                            violation_type: NamingViolationType::RevisionGap,
                            component: adapter_name.revision().to_string(),
                            reason: format!(
                                "Revision gap {} exceeds maximum allowed gap of {}",
                                gap, self.config.max_revision_gap
                            ),
                            suggestion: Some(format!(
                                "Use revision r{:03} (latest + 1)",
                                latest_rev + 1
                            )),
                        });
                    }
                }
            }
        }

        // Check hierarchy
        if self.config.enforce_hierarchy {
            if let Some(parent_name_str) = &request.parent_name {
                if let Ok(parent_name) = AdapterName::parse(parent_name_str) {
                    if !adapter_name.is_same_lineage(&parent_name) {
                        violations.push(NamingViolation {
                            violation_type: NamingViolationType::HierarchyViolation,
                            component: request.name.clone(),
                            reason: format!(
                                "Hierarchy violation: child '{}' is not in same lineage as parent '{}'",
                                adapter_name, parent_name
                            ),
                            suggestion: Some(format!(
                                "Use the same tenant/domain/purpose as parent: {}/{}/{}",
                                parent_name.tenant(),
                                parent_name.domain(),
                                parent_name.purpose()
                            )),
                        });
                    }
                }
            }
        }

        violations
    }
}

impl Policy for NamingPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Naming
    }

    fn name(&self) -> &'static str {
        "naming"
    }

    fn severity(&self) -> Severity {
        Severity::Medium
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        // Naming policy is typically enforced at registration time
        // via validate_adapter_name() and validate_stack_name()
        // This method returns a passed audit for runtime checks
        Ok(Audit::passed(self.id()).with_warnings(vec![
            format!(
                "Naming policy active: profanity_filter={}, reserved_ns={}, tenant_isolation={}, max_rev_gap={}",
                self.config.enforce_profanity_filter,
                self.config.enforce_reserved_namespaces,
                self.config.enforce_tenant_isolation,
                self.config.max_revision_gap
            )
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profanity_detection() {
        let config = NamingConfig::default();
        assert!(config.contains_profanity("offensive1"));
        assert!(config.contains_profanity("OFFENSIVE1")); // Case insensitive
        assert!(!config.contains_profanity("clean-name"));
    }

    #[test]
    fn test_valid_adapter_name() {
        let policy = NamingPolicy::new(NamingConfig::default());
        let request = AdapterNameValidation {
            name: "tenant-a/domain/purpose/r001".to_string(),
            tenant_id: "tenant-a".to_string(),
            parent_name: None,
            latest_revision: None,
        };

        assert!(policy.validate_adapter_name(&request).is_ok());
    }

    #[test]
    fn test_reserved_tenant_rejection() {
        let policy = NamingPolicy::new(NamingConfig::default());
        let request = AdapterNameValidation {
            name: "system/domain/purpose/r001".to_string(),
            tenant_id: "system".to_string(),
            parent_name: None,
            latest_revision: None,
        };

        assert!(policy.validate_adapter_name(&request).is_err());
    }

    #[test]
    fn test_tenant_isolation() {
        let policy = NamingPolicy::new(NamingConfig::default());
        let request = AdapterNameValidation {
            name: "tenant-a/domain/purpose/r001".to_string(),
            tenant_id: "tenant-b".to_string(), // Mismatch!
            parent_name: None,
            latest_revision: None,
        };

        let result = policy.validate_adapter_name(&request);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Tenant mismatch"));
    }

    #[test]
    fn test_revision_gap_enforcement() {
        let policy = NamingPolicy::new(NamingConfig::default());
        let request = AdapterNameValidation {
            name: "tenant-a/domain/purpose/r010".to_string(),
            tenant_id: "tenant-a".to_string(),
            parent_name: None,
            latest_revision: Some(1), // Gap of 9 (> max_revision_gap of 5)
        };

        let result = policy.validate_adapter_name(&request);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Revision gap"));
    }

    #[test]
    fn test_revision_monotonicity() {
        let policy = NamingPolicy::new(NamingConfig::default());
        let request = AdapterNameValidation {
            name: "tenant-a/domain/purpose/r001".to_string(),
            tenant_id: "tenant-a".to_string(),
            parent_name: None,
            latest_revision: Some(5), // New revision 1 <= latest 5
        };

        let result = policy.validate_adapter_name(&request);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("monotonicity"));
    }

    #[test]
    fn test_valid_stack_name() {
        let policy = NamingPolicy::new(NamingConfig::default());
        let request = StackNameValidation {
            name: "stack.my-namespace".to_string(),
            tenant_id: "tenant-a".to_string(),
        };

        assert!(policy.validate_stack_name(&request).is_ok());
    }

    #[test]
    fn test_naming_violation_analysis() {
        let policy = NamingPolicy::new(NamingConfig::default());

        // Test with a valid-format name that violates policies
        let request = AdapterNameValidation {
            name: "tenant-b/engineering/code-review/r010".to_string(),
            tenant_id: "tenant-a".to_string(), // Tenant mismatch
            parent_name: None,
            latest_revision: Some(1), // Revision gap
        };

        let violations = policy.analyze_adapter_name(&request);
        assert!(!violations.is_empty());

        // Should detect tenant isolation and revision gap
        let violation_types: Vec<_> = violations.iter().map(|v| &v.violation_type).collect();
        assert!(violation_types.contains(&&NamingViolationType::TenantIsolation),
            "Expected TenantIsolation violation. Got: {:?}", violation_types);
        assert!(violation_types.contains(&&NamingViolationType::RevisionGap),
            "Expected RevisionGap violation. Got: {:?}", violation_types);

        // Test that reserved tenant names fail at parse time (InvalidFormat)
        let reserved_request = AdapterNameValidation {
            name: "system/engineering/code-review/r001".to_string(),
            tenant_id: "system".to_string(),
            parent_name: None,
            latest_revision: None,
        };

        let reserved_violations = policy.analyze_adapter_name(&reserved_request);
        assert_eq!(reserved_violations.len(), 1);
        assert_eq!(reserved_violations[0].violation_type, NamingViolationType::InvalidFormat);
        assert!(reserved_violations[0].reason.contains("reserved"));
    }

    #[test]
    fn test_hierarchy_enforcement() {
        let policy = NamingPolicy::new(NamingConfig::default());
        let request = AdapterNameValidation {
            name: "tenant-a/domain/purpose/r002".to_string(),
            tenant_id: "tenant-a".to_string(),
            parent_name: Some("tenant-a/other-domain/purpose/r001".to_string()), // Different domain
            latest_revision: Some(1),
        };

        let result = policy.validate_adapter_name(&request);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Hierarchy violation"));
    }
}
