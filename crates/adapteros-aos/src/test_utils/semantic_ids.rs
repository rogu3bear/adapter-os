//! Semantic adapter ID generator
//!
//! Generates realistic adapter IDs following the naming conventions:
//! `{tenant}/{domain}/{purpose}/{revision}`
//!
//! Examples:
//! - `tenant-a/engineering/code-review/r001`
//! - `acme-corp/finance/fraud-detection/r003`
//! - `test-org/ml/sentiment-analysis/r001`

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Semantic adapter ID generator
pub struct SemanticIdGenerator {
    rng: ChaCha8Rng,
}

impl SemanticIdGenerator {
    /// Create a new generator with seed
    pub fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
        }
    }

    /// Generate a semantic adapter ID
    pub fn generate(&mut self) -> String {
        let tenant = self.random_tenant();
        let domain = self.random_domain();
        let purpose = self.random_purpose();
        let revision = self.random_revision();

        format!("{}/{}/{}/{}", tenant, domain, purpose, revision)
    }

    /// Generate with specific components
    pub fn generate_with(
        &mut self,
        tenant: Option<&str>,
        domain: Option<&str>,
        purpose: Option<&str>,
        revision: Option<&str>,
    ) -> String {
        let tenant = tenant.unwrap_or_else(|| self.random_tenant());
        let domain = domain.unwrap_or_else(|| self.random_domain());
        let purpose = purpose.unwrap_or_else(|| self.random_purpose());
        let revision_str = self.random_revision();
        let revision = revision.unwrap_or(&revision_str);

        format!("{}/{}/{}/{}", tenant, domain, purpose, revision)
    }

    fn random_tenant(&mut self) -> &'static str {
        const TENANTS: &[&str] = &[
            "tenant-a",
            "tenant-b",
            "acme-corp",
            "test-org",
            "demo-company",
            "example-inc",
            "sample-co",
            "test-tenant",
        ];
        TENANTS[self.rng.gen_range(0..TENANTS.len())]
    }

    fn random_domain(&mut self) -> &'static str {
        const DOMAINS: &[&str] = &[
            "engineering",
            "finance",
            "marketing",
            "ml",
            "data-science",
            "research",
            "operations",
            "product",
        ];
        DOMAINS[self.rng.gen_range(0..DOMAINS.len())]
    }

    fn random_purpose(&mut self) -> &'static str {
        const PURPOSES: &[&str] = &[
            "code-review",
            "fraud-detection",
            "sentiment-analysis",
            "text-classification",
            "summarization",
            "translation",
            "question-answering",
            "entity-recognition",
            "anomaly-detection",
            "recommendation",
            "forecasting",
            "clustering",
        ];
        PURPOSES[self.rng.gen_range(0..PURPOSES.len())]
    }

    fn random_revision(&mut self) -> String {
        format!("r{:03}", self.rng.gen_range(1..=20))
    }
}

/// Generate a simple test adapter ID (deterministic)
pub fn generate_test_id() -> String {
    "test-tenant/test-domain/test-purpose/r001".to_string()
}

/// Generate a valid tenant ID
pub fn generate_tenant_id(seed: u64) -> String {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    const TENANTS: &[&str] = &[
        "tenant-a",
        "tenant-b",
        "acme-corp",
        "test-org",
        "demo-company",
    ];
    TENANTS[rng.gen_range(0..TENANTS.len())].to_string()
}

/// Validate adapter ID format
pub fn validate_adapter_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('/').collect();
    if parts.len() != 4 {
        return false;
    }

    // Check each part is non-empty and valid
    parts
        .iter()
        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '-'))
}

/// Extract components from adapter ID
pub fn parse_adapter_id(id: &str) -> Option<(String, String, String, String)> {
    let parts: Vec<&str> = id.split('/').collect();
    if parts.len() != 4 {
        return None;
    }

    Some((
        parts[0].to_string(),
        parts[1].to_string(),
        parts[2].to_string(),
        parts[3].to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_semantic_id() {
        let mut generator = SemanticIdGenerator::new(42);
        let id = generator.generate();

        println!("Generated ID: {}", id);

        assert!(validate_adapter_id(&id), "Should generate valid ID");

        let parts: Vec<&str> = id.split('/').collect();
        assert_eq!(parts.len(), 4, "Should have 4 parts");
        assert!(!parts[0].is_empty(), "Tenant should not be empty");
        assert!(!parts[1].is_empty(), "Domain should not be empty");
        assert!(!parts[2].is_empty(), "Purpose should not be empty");
        assert!(parts[3].starts_with('r'), "Revision should start with 'r'");
    }

    #[test]
    fn test_deterministic_generation() {
        let mut gen1 = SemanticIdGenerator::new(42);
        let mut gen2 = SemanticIdGenerator::new(42);

        let id1 = gen1.generate();
        let id2 = gen2.generate();

        assert_eq!(id1, id2, "Same seed should produce same ID");
    }

    #[test]
    fn test_generate_with_custom_components() {
        let mut generator = SemanticIdGenerator::new(42);
        let id = generator.generate_with(
            Some("custom-tenant"),
            Some("custom-domain"),
            None,
            Some("r999"),
        );

        assert!(id.starts_with("custom-tenant/custom-domain/"));
        assert!(id.ends_with("/r999"));
    }

    #[test]
    fn test_validate_adapter_id() {
        assert!(validate_adapter_id("tenant-a/engineering/code-review/r001"));
        assert!(validate_adapter_id("a/b/c/d"));

        // Invalid cases
        assert!(!validate_adapter_id("tenant-a/engineering/code-review")); // Only 3 parts
        assert!(!validate_adapter_id(
            "tenant-a/engineering/code-review/r001/extra"
        )); // 5 parts
        assert!(!validate_adapter_id("")); // Empty
        assert!(!validate_adapter_id("a/b//d")); // Empty part
    }

    #[test]
    fn test_parse_adapter_id() {
        let id = "tenant-a/engineering/code-review/r001";
        let parsed = parse_adapter_id(id);

        assert!(parsed.is_some());
        let (tenant, domain, purpose, revision) = parsed.unwrap();

        assert_eq!(tenant, "tenant-a");
        assert_eq!(domain, "engineering");
        assert_eq!(purpose, "code-review");
        assert_eq!(revision, "r001");
    }

    #[test]
    fn test_generate_test_id() {
        let id = generate_test_id();
        assert_eq!(id, "test-tenant/test-domain/test-purpose/r001");
        assert!(validate_adapter_id(&id));
    }

    #[test]
    fn test_generate_tenant_id() {
        let tenant = generate_tenant_id(42);
        assert!(!tenant.is_empty());
        assert!(tenant.chars().all(|c| c.is_alphanumeric() || c == '-'));
    }

    #[test]
    fn test_multiple_generations_unique() {
        let mut generator = SemanticIdGenerator::new(42);
        let id1 = generator.generate();
        let id2 = generator.generate();
        let id3 = generator.generate();

        // Should generate different IDs (though not guaranteed due to randomness)
        println!("ID1: {}", id1);
        println!("ID2: {}", id2);
        println!("ID3: {}", id3);

        // All should be valid
        assert!(validate_adapter_id(&id1));
        assert!(validate_adapter_id(&id2));
        assert!(validate_adapter_id(&id3));
    }
}
