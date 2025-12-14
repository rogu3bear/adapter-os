//! Prefix Resolver Service
//!
//! Resolves prefix templates for a tenant + mode combination and tokenizes them.
//! This is the main integration point between prefix template configuration
//! and the PrefixKvCache.
//!
//! See PRD: PrefixKvCache v1

use adapteros_api_types::prefix_templates::{PrefixMode, PrefixTemplate};
use adapteros_core::{AosError, B3Hash};
use adapteros_db::Db;
use std::sync::Arc;

/// Result of resolving a prefix template.
#[derive(Debug, Clone)]
pub struct ResolvedPrefix {
    /// The prefix template that was resolved
    pub template: PrefixTemplate,
    /// Tokenized prefix (sequence of token IDs)
    pub token_ids: Vec<u32>,
    /// Hash of the tokenized template (for cache key computation)
    pub tokenized_hash: B3Hash,
}

/// Prefix resolver service.
///
/// Resolves prefix templates from the database and tokenizes them
/// for use with the PrefixKvCache.
pub struct PrefixResolver {
    db: Arc<Db>,
}

impl PrefixResolver {
    /// Create a new prefix resolver.
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// Resolve and tokenize a prefix for a tenant and mode.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant to resolve prefix for
    /// * `mode` - The mode (user, builder, audit, etc.)
    /// * `tokenize_fn` - Function to tokenize text into token IDs
    ///
    /// # Returns
    /// * `Ok(Some(ResolvedPrefix))` - Successfully resolved and tokenized prefix
    /// * `Ok(None)` - No prefix template configured for this tenant/mode
    /// * `Err(...)` - Database or tokenization error
    pub async fn resolve_prefix<F>(
        &self,
        tenant_id: &str,
        mode: &PrefixMode,
        tokenize_fn: F,
    ) -> Result<Option<ResolvedPrefix>, AosError>
    where
        F: FnOnce(&str) -> Result<Vec<u32>, AosError>,
    {
        // Look up the best matching template
        let template = match self
            .db
            .get_prefix_template_for_mode(tenant_id, mode)
            .await?
        {
            Some(t) => t,
            None => {
                tracing::debug!(
                    tenant_id = tenant_id,
                    mode = %mode,
                    "No prefix template configured"
                );
                return Ok(None);
            }
        };

        // Tokenize the template text
        let token_ids = tokenize_fn(&template.template_text)?;

        if token_ids.is_empty() {
            tracing::warn!(
                tenant_id = tenant_id,
                mode = %mode,
                template_id = %template.id,
                "Prefix template tokenized to empty sequence"
            );
            return Ok(None);
        }

        // Compute hash of tokenized result for cache key
        let tokenized_hash = compute_tokenized_hash(&token_ids);

        tracing::debug!(
            tenant_id = tenant_id,
            mode = %mode,
            template_id = %template.id,
            token_count = token_ids.len(),
            "Resolved prefix template"
        );

        Ok(Some(ResolvedPrefix {
            template,
            token_ids,
            tokenized_hash,
        }))
    }

    /// Resolve prefix without tokenization (for inspection/debugging).
    pub async fn resolve_template(
        &self,
        tenant_id: &str,
        mode: &PrefixMode,
    ) -> Result<Option<PrefixTemplate>, AosError> {
        self.db.get_prefix_template_for_mode(tenant_id, mode).await
    }

    /// Check if a tenant has any prefix templates configured.
    pub async fn has_prefix_templates(&self, tenant_id: &str) -> Result<bool, AosError> {
        let templates = self.db.list_prefix_templates(tenant_id).await?;
        Ok(!templates.is_empty())
    }

    /// Get all prefix templates for a tenant.
    pub async fn list_templates(&self, tenant_id: &str) -> Result<Vec<PrefixTemplate>, AosError> {
        self.db.list_prefix_templates(tenant_id).await
    }
}

/// Compute a hash of the tokenized prefix for cache key computation.
///
/// This hash is used as part of the prefix_kv_key to ensure that
/// different tokenizations (due to different tokenizers) produce
/// different cache keys.
fn compute_tokenized_hash(token_ids: &[u32]) -> B3Hash {
    use adapteros_core::prefix_kv_key::encode_prefix_tokens;
    let bytes = encode_prefix_tokens(token_ids);
    B3Hash::hash(&bytes)
}

/// Builder for constructing ResolvedPrefix manually (for testing/mocking).
#[derive(Debug, Default)]
pub struct ResolvedPrefixBuilder {
    template_id: Option<String>,
    tenant_id: Option<String>,
    mode: Option<PrefixMode>,
    template_text: Option<String>,
    token_ids: Option<Vec<u32>>,
}

impl ResolvedPrefixBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the template ID.
    pub fn template_id(mut self, id: impl Into<String>) -> Self {
        self.template_id = Some(id.into());
        self
    }

    /// Set the tenant ID.
    pub fn tenant_id(mut self, id: impl Into<String>) -> Self {
        self.tenant_id = Some(id.into());
        self
    }

    /// Set the mode.
    pub fn mode(mut self, mode: PrefixMode) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Set the template text.
    pub fn template_text(mut self, text: impl Into<String>) -> Self {
        self.template_text = Some(text.into());
        self
    }

    /// Set the token IDs.
    pub fn token_ids(mut self, ids: Vec<u32>) -> Self {
        self.token_ids = Some(ids);
        self
    }

    /// Build the ResolvedPrefix.
    ///
    /// # Panics
    /// Panics if required fields are not set.
    pub fn build(self) -> ResolvedPrefix {
        let template_text = self.template_text.expect("template_text required");
        let token_ids = self.token_ids.expect("token_ids required");

        ResolvedPrefix {
            template: PrefixTemplate {
                id: self
                    .template_id
                    .unwrap_or_else(|| "test-template".to_string()),
                tenant_id: self.tenant_id.unwrap_or_else(|| "test-tenant".to_string()),
                mode: self.mode.unwrap_or(PrefixMode::System),
                template_text: template_text.clone(),
                template_hash_b3: B3Hash::hash(template_text.as_bytes()),
                priority: 0,
                enabled: true,
            },
            tokenized_hash: compute_tokenized_hash(&token_ids),
            token_ids,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_tokenized_hash_deterministic() {
        let tokens = vec![1u32, 2, 3, 4, 5];

        let hash1 = compute_tokenized_hash(&tokens);
        let hash2 = compute_tokenized_hash(&tokens);

        assert_eq!(hash1, hash2, "Hash should be deterministic");
    }

    #[test]
    fn test_compute_tokenized_hash_different_tokens() {
        let tokens_a = vec![1u32, 2, 3];
        let tokens_b = vec![1u32, 2, 4];

        let hash_a = compute_tokenized_hash(&tokens_a);
        let hash_b = compute_tokenized_hash(&tokens_b);

        assert_ne!(
            hash_a, hash_b,
            "Different tokens should produce different hashes"
        );
    }

    #[test]
    fn test_resolved_prefix_builder() {
        let resolved = ResolvedPrefixBuilder::new()
            .template_id("tpl-123")
            .tenant_id("tenant-1")
            .mode(PrefixMode::User)
            .template_text("You are helpful.")
            .token_ids(vec![100, 200, 300])
            .build();

        assert_eq!(resolved.template.id, "tpl-123");
        assert_eq!(resolved.template.tenant_id, "tenant-1");
        assert_eq!(resolved.template.mode, PrefixMode::User);
        assert_eq!(resolved.token_ids, vec![100, 200, 300]);
    }

    #[tokio::test]
    async fn test_prefix_resolver_no_template() {
        let db = Arc::new(Db::new_in_memory().await.unwrap());
        let resolver = PrefixResolver::new(db);

        let result = resolver
            .resolve_prefix("nonexistent-tenant", &PrefixMode::User, |_| {
                Ok(vec![1, 2, 3])
            })
            .await
            .unwrap();

        assert!(result.is_none(), "Should return None for missing tenant");
    }

    #[tokio::test]
    async fn test_prefix_resolver_with_template() {
        use adapteros_api_types::prefix_templates::CreatePrefixTemplateRequest;

        let db = Arc::new(Db::new_in_memory().await.unwrap());

        // Create a template
        db.create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "You are a helpful assistant.".to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .unwrap();

        let resolver = PrefixResolver::new(db);

        // Mock tokenizer that returns fixed tokens
        let result = resolver
            .resolve_prefix("tenant-1", &PrefixMode::User, |text| {
                assert_eq!(text, "You are a helpful assistant.");
                Ok(vec![100, 200, 300, 400])
            })
            .await
            .unwrap();

        let resolved = result.expect("Should resolve template");
        assert_eq!(resolved.token_ids, vec![100, 200, 300, 400]);
        assert_eq!(resolved.template.mode, PrefixMode::User);
    }

    #[tokio::test]
    async fn test_has_prefix_templates() {
        use adapteros_api_types::prefix_templates::CreatePrefixTemplateRequest;

        let db = Arc::new(Db::new_in_memory().await.unwrap());
        let resolver = PrefixResolver::new(Arc::clone(&db));

        // Initially no templates
        assert!(!resolver.has_prefix_templates("tenant-1").await.unwrap());

        // Add a template
        db.create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::System,
            template_text: "System prefix".to_string(),
            priority: None,
            enabled: None,
        })
        .await
        .unwrap();

        // Now has templates
        assert!(resolver.has_prefix_templates("tenant-1").await.unwrap());

        // Different tenant still has none
        assert!(!resolver.has_prefix_templates("tenant-2").await.unwrap());
    }
}
