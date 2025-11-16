//! Adapter and stack naming validation
//!
//! This module implements the canonical naming taxonomy for AdapterOS adapters and stacks.
//! See docs/ADAPTER_TAXONOMY.md for the complete specification.

use crate::{AosError, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::fmt;

// Reserved tenant namespaces (note: "global" is allowed for shared adapters)
pub const RESERVED_TENANTS: &[&str] = &["system", "admin", "root", "default", "test"];

// Reserved domain namespaces
pub const RESERVED_DOMAINS: &[&str] = &["core", "internal", "deprecated"];

// Reserved stack prefixes
pub const RESERVED_STACKS: &[&str] = &["stack.safe-default"];

// Validation regexes (compiled once)
static TENANT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z0-9][a-z0-9-]{0,30}[a-z0-9]$").unwrap());

static DOMAIN_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z0-9][a-z0-9-]{0,46}[a-z0-9]$").unwrap());

static PURPOSE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$").unwrap());

static REVISION_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^r[0-9]{3,}$").unwrap());

static ADAPTER_NAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^[a-z0-9][a-z0-9-]{0,30}[a-z0-9]/[a-z0-9][a-z0-9-]{0,46}[a-z0-9]/[a-z0-9][a-z0-9-]{0,62}[a-z0-9]/r[0-9]{3,}$",
    )
    .unwrap()
});

static STACK_NAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^stack\.([a-z0-9][a-z0-9-]{0,30}[a-z0-9])(\.[a-z0-9][a-z0-9-]{0,46}[a-z0-9])?$")
        .unwrap()
});

// Component validation regex to reject consecutive hyphens
static NO_CONSECUTIVE_HYPHENS: Lazy<Regex> = Lazy::new(|| Regex::new(r"--+").unwrap());

/// Semantic adapter name: {tenant}/{domain}/{purpose}/{revision}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdapterName {
    tenant: String,
    domain: String,
    purpose: String,
    revision: String,
}

impl AdapterName {
    /// Parse an adapter name from string
    ///
    /// # Example
    /// ```
    /// use adapteros_core::naming::AdapterName;
    ///
    /// let name = AdapterName::parse("shop-floor/hydraulics/troubleshooting/r042")?;
    /// assert_eq!(name.tenant(), "shop-floor");
    /// assert_eq!(name.revision_number()?, 42);
    /// # Ok::<(), adapteros_core::AosError>(())
    /// ```
    pub fn parse(name: &str) -> Result<Self> {
        if name.len() > 200 {
            return Err(AosError::Validation(
                "Adapter name exceeds 200 character limit".to_string(),
            ));
        }

        // Fast path: validate overall format first
        if !ADAPTER_NAME_REGEX.is_match(name) {
            return Err(AosError::Validation(format!(
                "Invalid adapter name format: '{}'. Expected: {{tenant}}/{{domain}}/{{purpose}}/{{revision}}",
                name
            )));
        }

        let parts: Vec<&str> = name.split('/').collect();
        if parts.len() != 4 {
            return Err(AosError::Validation(format!(
                "Invalid adapter name: expected 4 components, got {}",
                parts.len()
            )));
        }

        let adapter = Self {
            tenant: parts[0].to_string(),
            domain: parts[1].to_string(),
            purpose: parts[2].to_string(),
            revision: parts[3].to_string(),
        };

        adapter.validate()?;
        Ok(adapter)
    }

    /// Create adapter name from components (validates on construction)
    pub fn new(tenant: &str, domain: &str, purpose: &str, revision: &str) -> Result<Self> {
        let adapter = Self {
            tenant: tenant.to_string(),
            domain: domain.to_string(),
            purpose: purpose.to_string(),
            revision: revision.to_string(),
        };

        adapter.validate()?;
        Ok(adapter)
    }

    /// Validate all components
    pub fn validate(&self) -> Result<()> {
        self.validate_tenant()?;
        self.validate_domain()?;
        self.validate_purpose()?;
        self.validate_revision()?;

        // Check total length
        let full_name = self.to_string();
        if full_name.len() > 200 {
            return Err(AosError::Validation(
                "Adapter name exceeds 200 character limit".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_tenant(&self) -> Result<()> {
        if !TENANT_REGEX.is_match(&self.tenant) {
            return Err(AosError::Validation(format!(
                "Invalid tenant component: '{}'. Must be 2-32 chars, alphanumeric + hyphens",
                self.tenant
            )));
        }

        if NO_CONSECUTIVE_HYPHENS.is_match(&self.tenant) {
            return Err(AosError::Validation(
                "Tenant component cannot contain consecutive hyphens".to_string(),
            ));
        }

        if RESERVED_TENANTS.contains(&self.tenant.as_str()) {
            return Err(AosError::Validation(format!(
                "Tenant '{}' is reserved",
                self.tenant
            )));
        }

        Ok(())
    }

    fn validate_domain(&self) -> Result<()> {
        if !DOMAIN_REGEX.is_match(&self.domain) {
            return Err(AosError::Validation(format!(
                "Invalid domain component: '{}'. Must be 2-48 chars, alphanumeric + hyphens",
                self.domain
            )));
        }

        if NO_CONSECUTIVE_HYPHENS.is_match(&self.domain) {
            return Err(AosError::Validation(
                "Domain component cannot contain consecutive hyphens".to_string(),
            ));
        }

        // Check reserved domains
        if RESERVED_DOMAINS.contains(&self.domain.as_str()) {
            return Err(AosError::Validation(format!(
                "Domain '{}' is reserved for system use",
                self.domain
            )));
        }

        Ok(())
    }

    fn validate_purpose(&self) -> Result<()> {
        if !PURPOSE_REGEX.is_match(&self.purpose) {
            return Err(AosError::Validation(format!(
                "Invalid purpose component: '{}'. Must be 2-64 chars, alphanumeric + hyphens",
                self.purpose
            )));
        }

        if NO_CONSECUTIVE_HYPHENS.is_match(&self.purpose) {
            return Err(AosError::Validation(
                "Purpose component cannot contain consecutive hyphens".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_revision(&self) -> Result<()> {
        if !REVISION_REGEX.is_match(&self.revision) {
            return Err(AosError::Validation(format!(
                "Invalid revision component: '{}'. Must match pattern: r{{NNN}} (e.g., r001, r042)",
                self.revision
            )));
        }

        Ok(())
    }

    /// Get tenant component
    pub fn tenant(&self) -> &str {
        &self.tenant
    }

    /// Get domain component
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// Get purpose component
    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    /// Get revision string (e.g., "r042")
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// Get revision as numeric value
    pub fn revision_number(&self) -> Result<u32> {
        self.revision[1..]
            .parse::<u32>()
            .map_err(|e| AosError::Validation(format!("Invalid revision number: {}", e)))
    }

    /// Get base path (tenant/domain/purpose) without revision
    pub fn base_path(&self) -> String {
        format!("{}/{}/{}", self.tenant, self.domain, self.purpose)
    }

    /// Check if this adapter is in the same lineage (same base path)
    pub fn is_same_lineage(&self, other: &AdapterName) -> bool {
        self.tenant == other.tenant && self.domain == other.domain && self.purpose == other.purpose
    }

    /// Check if adapter is in global namespace
    pub fn is_global(&self) -> bool {
        self.tenant == "global"
    }

    /// Create next revision in sequence
    pub fn next_revision(&self) -> Result<Self> {
        let current = self.revision_number()?;
        let next = current + 1;
        let next_rev = format!("r{:03}", next);

        Self::new(&self.tenant, &self.domain, &self.purpose, &next_rev)
    }

    /// Get display name for UI (includes "rev" prefix)
    pub fn display_name(&self) -> String {
        format!(
            "{}/{}/{} (rev {})",
            self.tenant,
            self.domain,
            self.purpose,
            self.revision_number().unwrap_or(0)
        )
    }
}

impl fmt::Display for AdapterName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}/{}/{}",
            self.tenant, self.domain, self.purpose, self.revision
        )
    }
}

impl std::str::FromStr for AdapterName {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

/// Semantic stack name: stack.{namespace}[.{identifier}]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StackName {
    namespace: String,
    identifier: Option<String>,
}

impl StackName {
    /// Parse a stack name from string
    ///
    /// # Example
    /// ```
    /// use adapteros_core::naming::StackName;
    ///
    /// let name = StackName::parse("stack.shop-floor-nightshift")?;
    /// assert_eq!(name.namespace(), "shop-floor-nightshift");
    ///
    /// let name2 = StackName::parse("stack.acme-corp.production")?;
    /// assert_eq!(name2.namespace(), "acme-corp");
    /// assert_eq!(name2.identifier(), Some("production"));
    /// # Ok::<(), adapteros_core::AosError>(())
    /// ```
    pub fn parse(name: &str) -> Result<Self> {
        if name.len() > 100 {
            return Err(AosError::Validation(
                "Stack name exceeds 100 character limit".to_string(),
            ));
        }

        if !name.starts_with("stack.") {
            return Err(AosError::Validation(
                "Stack name must start with 'stack.'".to_string(),
            ));
        }

        if !STACK_NAME_REGEX.is_match(name) {
            return Err(AosError::Validation(format!(
                "Invalid stack name format: '{}'. Expected: stack.{{namespace}}[.{{identifier}}]",
                name
            )));
        }

        let parts: Vec<&str> = name.split('.').collect();

        let stack = match parts.len() {
            2 => {
                // stack.namespace
                Self {
                    namespace: parts[1].to_string(),
                    identifier: None,
                }
            }
            3 => {
                // stack.namespace.identifier
                Self {
                    namespace: parts[1].to_string(),
                    identifier: Some(parts[2].to_string()),
                }
            }
            _ => {
                return Err(AosError::Validation(format!(
                    "Invalid stack name: too many components in '{}'",
                    name
                )))
            }
        };

        stack.validate()?;
        Ok(stack)
    }

    /// Create stack name from components
    pub fn new(namespace: &str, identifier: Option<&str>) -> Result<Self> {
        let stack = Self {
            namespace: namespace.to_string(),
            identifier: identifier.map(|s| s.to_string()),
        };

        stack.validate()?;
        Ok(stack)
    }

    /// Validate stack name
    pub fn validate(&self) -> Result<()> {
        // Check reserved names first (only exact matches)
        let full_name = self.to_string();
        for reserved in RESERVED_STACKS {
            if &full_name == reserved {
                return Err(AosError::Validation(format!(
                    "Stack name '{}' is reserved",
                    full_name
                )));
            }
        }

        // Check total length
        if full_name.len() > 100 {
            return Err(AosError::Validation(
                "Stack name exceeds 100 character limit".to_string(),
            ));
        }

        // Validate namespace
        if !TENANT_REGEX.is_match(&self.namespace) {
            return Err(AosError::Validation(format!(
                "Invalid stack namespace: '{}'. Must be 2-32 chars, alphanumeric + hyphens",
                self.namespace
            )));
        }

        if NO_CONSECUTIVE_HYPHENS.is_match(&self.namespace) {
            return Err(AosError::Validation(
                "Stack namespace cannot contain consecutive hyphens".to_string(),
            ));
        }

        // Validate identifier if present
        if let Some(ref id) = self.identifier {
            if !DOMAIN_REGEX.is_match(id) {
                return Err(AosError::Validation(format!(
                    "Invalid stack identifier: '{}'. Must be 2-48 chars, alphanumeric + hyphens",
                    id
                )));
            }

            if NO_CONSECUTIVE_HYPHENS.is_match(id) {
                return Err(AosError::Validation(
                    "Stack identifier cannot contain consecutive hyphens".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Get namespace component
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Get identifier component
    pub fn identifier(&self) -> Option<&str> {
        self.identifier.as_deref()
    }

    /// Check if stack is in system namespace
    pub fn is_system(&self) -> bool {
        self.namespace == "system"
    }
}

impl fmt::Display for StackName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref id) = self.identifier {
            write!(f, "stack.{}.{}", self.namespace, id)
        } else {
            write!(f, "stack.{}", self.namespace)
        }
    }
}

impl std::str::FromStr for StackName {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

/// Fork type for adapter lineage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkType {
    /// Independent fork with divergent use case
    Independent,
    /// Incremental extension maintaining compatibility
    Extension,
}

impl ForkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ForkType::Independent => "independent",
            ForkType::Extension => "extension",
        }
    }

    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "independent" => Ok(ForkType::Independent),
            "extension" => Ok(ForkType::Extension),
            _ => Err(AosError::Validation(format!(
                "Invalid fork type: '{}'. Expected 'independent' or 'extension'",
                s
            ))),
        }
    }

    /// Validate fork type semantics between parent and child
    ///
    /// # Rules
    /// - **Independent**: Child must have different purpose (divergent use case)
    /// - **Extension**: Child must be in same lineage (same tenant/domain/purpose, only revision changes)
    pub fn validate_fork(&self, parent: &AdapterName, child: &AdapterName) -> Result<()> {
        match self {
            ForkType::Independent => {
                // Independent fork: purpose must be different (indicates divergent use case)
                if parent.purpose() == child.purpose() {
                    return Err(AosError::Validation(format!(
                        "Independent fork must have different purpose: parent='{}', child='{}'",
                        parent.purpose(),
                        child.purpose()
                    )));
                }
                // Domain can be same or different for independent forks
                Ok(())
            }
            ForkType::Extension => {
                // Extension: must be in same lineage (same tenant/domain/purpose)
                if !parent.is_same_lineage(child) {
                    return Err(AosError::Validation(format!(
                        "Extension fork must stay in same lineage: parent='{}', child='{}'",
                        parent.base_path(),
                        child.base_path()
                    )));
                }
                // Revision must be different (already enforced by uniqueness)
                Ok(())
            }
        }
    }
}

impl fmt::Display for ForkType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_adapter_names() {
        let valid = [
            "shop-floor/hydraulics/troubleshooting/r001",
            "dentist-office/scheduling/appointment-booking/r042",
            "global/code/rust-analyzer/r015",
            "acme-corp/legal/contract-review/r003",
            "ab/cd/ef/r123",
            "tenant-123/domain-456/purpose-789/r999",
        ];

        for name in &valid {
            let parsed = AdapterName::parse(name);
            assert!(
                parsed.is_ok(),
                "Failed to parse valid name '{}': {:?}",
                name,
                parsed.err()
            );
        }
    }

    #[test]
    fn test_invalid_adapter_names() {
        let invalid = [
            "too-short/a/b/r001",           // domain too short
            "tenant/domain/purpose/r1",     // revision too short
            "tenant/domain/purpose/001",    // missing 'r' prefix
            "Tenant/domain/purpose/r001",   // uppercase
            "tenant/domain/purpose/r1.2.3", // invalid revision format
            "tenant/domain/purpose",        // missing revision
            "tenant/domain",                // too few components
            "a--b/domain/purpose/r001",     // consecutive hyphens
            "-tenant/domain/purpose/r001",  // starts with hyphen
            "tenant-/domain/purpose/r001",  // ends with hyphen
        ];

        for name in &invalid {
            let parsed = AdapterName::parse(name);
            assert!(parsed.is_err(), "Should reject invalid name '{}'", name);
        }
    }

    #[test]
    fn test_reserved_tenant_rejection() {
        let result = AdapterName::parse("system/domain/purpose/r001");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Tenant 'system' is reserved"));
    }

    #[test]
    fn test_reserved_domain_rejection() {
        let reserved_domains = ["core", "internal", "deprecated"];
        for domain in &reserved_domains {
            let name = format!("tenant/{}/purpose/r001", domain);
            let result = AdapterName::parse(&name);
            assert!(
                result.is_err(),
                "Should reject reserved domain '{}'",
                domain
            );
            assert!(
                result.unwrap_err().to_string().contains("reserved"),
                "Error should mention domain is reserved"
            );
        }

        // Non-reserved domains should work
        let result = AdapterName::parse("tenant/hydraulics/purpose/r001");
        assert!(result.is_ok(), "Non-reserved domain should be accepted");
    }

    #[test]
    fn test_adapter_name_components() {
        let name = AdapterName::parse("shop-floor/hydraulics/troubleshooting/r042").unwrap();
        assert_eq!(name.tenant(), "shop-floor");
        assert_eq!(name.domain(), "hydraulics");
        assert_eq!(name.purpose(), "troubleshooting");
        assert_eq!(name.revision(), "r042");
        assert_eq!(name.revision_number().unwrap(), 42);
        assert_eq!(name.base_path(), "shop-floor/hydraulics/troubleshooting");
    }

    #[test]
    fn test_adapter_lineage() {
        let name1 = AdapterName::parse("tenant/domain/purpose/r001").unwrap();
        let name2 = AdapterName::parse("tenant/domain/purpose/r002").unwrap();
        let name3 = AdapterName::parse("tenant/domain/other/r001").unwrap();

        assert!(name1.is_same_lineage(&name2));
        assert!(!name1.is_same_lineage(&name3));
    }

    #[test]
    fn test_next_revision() {
        let name = AdapterName::parse("tenant/domain/purpose/r042").unwrap();
        let next = name.next_revision().unwrap();
        assert_eq!(next.revision(), "r043");
        assert_eq!(next.revision_number().unwrap(), 43);
    }

    #[test]
    fn test_valid_stack_names() {
        let valid = [
            "stack.dentist-office",
            "stack.shop-floor-nightshift",
            "stack.acme-corp.production",
            "stack.global.code-review",
            "stack.ab.cd",
            "stack.my-stack",
        ];

        for name in &valid {
            let parsed = StackName::parse(name);
            assert!(
                parsed.is_ok(),
                "Failed to parse valid stack name '{}': {:?}",
                name,
                parsed.err()
            );
        }
    }

    #[test]
    fn test_invalid_stack_names() {
        let invalid = [
            "not-a-stack",           // missing prefix
            "stack",                 // no namespace
            "stack.",                // empty namespace
            "stack.a",               // namespace too short
            "stack.A-B",             // uppercase
            "stack.tenant.id.extra", // too many components
            "stack.tenant--id",      // consecutive hyphens
            "stack.-tenant",         // starts with hyphen
        ];

        for name in &invalid {
            let parsed = StackName::parse(name);
            assert!(
                parsed.is_err(),
                "Should reject invalid stack name '{}'",
                name
            );
        }
    }

    #[test]
    fn test_reserved_stack_rejection() {
        // Test exact match on reserved names
        let result = StackName::parse("stack.safe-default");
        assert!(result.is_err(), "Reserved stack name should be rejected");
        assert!(result.unwrap_err().to_string().contains("reserved"));

        // Similar but not exact match should be allowed
        let result2 = StackName::parse("stack.safe-default-v2");
        assert!(result2.is_ok(), "Non-reserved variation should be allowed");
    }

    #[test]
    fn test_stack_name_components() {
        let name1 = StackName::parse("stack.shop-floor-nightshift").unwrap();
        assert_eq!(name1.namespace(), "shop-floor-nightshift");
        assert_eq!(name1.identifier(), None);

        let name2 = StackName::parse("stack.acme-corp.production").unwrap();
        assert_eq!(name2.namespace(), "acme-corp");
        assert_eq!(name2.identifier(), Some("production"));
    }

    #[test]
    fn test_fork_type() {
        assert_eq!(
            ForkType::from_str("independent").unwrap(),
            ForkType::Independent
        );
        assert_eq!(
            ForkType::from_str("extension").unwrap(),
            ForkType::Extension
        );
        assert!(ForkType::from_str("invalid").is_err());
    }

    #[test]
    fn test_fork_type_validation() {
        let parent = AdapterName::parse("tenant/domain/purpose/r001").unwrap();

        // Extension fork: must stay in same lineage
        let valid_extension = AdapterName::parse("tenant/domain/purpose/r002").unwrap();
        assert!(ForkType::Extension
            .validate_fork(&parent, &valid_extension)
            .is_ok());

        let invalid_extension = AdapterName::parse("tenant/domain/other-purpose/r001").unwrap();
        assert!(ForkType::Extension
            .validate_fork(&parent, &invalid_extension)
            .is_err());

        // Independent fork: must have different purpose
        let valid_independent = AdapterName::parse("tenant/domain/other-purpose/r001").unwrap();
        assert!(ForkType::Independent
            .validate_fork(&parent, &valid_independent)
            .is_ok());

        let invalid_independent = AdapterName::parse("tenant/domain/purpose/r002").unwrap();
        assert!(ForkType::Independent
            .validate_fork(&parent, &invalid_independent)
            .is_err());
    }

    #[test]
    fn test_adapter_display_name() {
        let name = AdapterName::parse("shop-floor/hydraulics/troubleshooting/r042").unwrap();
        assert_eq!(
            name.display_name(),
            "shop-floor/hydraulics/troubleshooting (rev 42)"
        );
    }

    #[test]
    fn test_max_length_limits() {
        // Test max length adapter name (should be just under 200 chars)
        let long_name = format!(
            "{}/{}/{}/r001",
            "a".repeat(32),
            "b".repeat(48),
            "c".repeat(64)
        );
        let parsed = AdapterName::parse(&long_name);
        assert!(parsed.is_ok(), "Should accept name at max length");

        // Test exceeds limit
        let too_long = format!(
            "{}/{}/{}/r001",
            "a".repeat(100),
            "b".repeat(100),
            "c".repeat(100)
        );
        assert!(AdapterName::parse(&too_long).is_err());

        // Test max length stack name
        let long_stack = format!("stack.{}.{}", "a".repeat(32), "b".repeat(48));
        assert!(StackName::parse(&long_stack).is_ok());
    }
}
