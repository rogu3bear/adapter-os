//! Adapter versioning and ref management types
//!
//! This module provides git-inspired versioning for adapters with:
//! - Content-addressed immutable storage (BLAKE3 hashes)
//! - Named refs (current, previous, draft, v1, v2, etc.)
//! - Adapter naming conventions (subject, domain, specialized, stack)
//!
//! # Naming Convention
//!
//! ```text
//! developer.aos              → Subject (persona)
//! actions.domain.aos         → Domain (portable capability)
//! developer.aos.actions      → Specialized (subject×domain optimized)
//! dev-full.stack.aos         → Stack (saved composition)
//! ```
//!
//! # Filesystem Layout
//!
//! ```text
//! var/adapters/
//! ├── objects/                          # Content-addressed immutable store
//! │   └── {hash[0:2]}/
//! │       └── {hash[2:10]}/
//! │           └── {full_hash}.aos
//! │
//! ├── subjects/{tenant}/{name}/refs/    # Subject adapters
//! ├── domains/{tenant}/{name}/refs/     # Domain adapters
//! ├── specialized/{tenant}/{subj}.{dom}/refs/
//! ├── stacks/{tenant}/{name}/versions/  # Stack definitions
//! │
//! └── index.redb                        # Version metadata
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Adapter classification by purpose
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterKind {
    /// Subject adapter (persona): `developer.aos`
    Subject,
    /// Domain adapter (portable capability): `actions.domain.aos`
    Domain,
    /// Specialized adapter (subject×domain optimized): `developer.aos.actions`
    Specialized,
    /// Stack definition (saved composition): `dev-full.stack.aos`
    Stack,
}

impl AdapterKind {
    /// Get the directory name for this kind
    pub fn dir_name(&self) -> &'static str {
        match self {
            AdapterKind::Subject => "subjects",
            AdapterKind::Domain => "domains",
            AdapterKind::Specialized => "specialized",
            AdapterKind::Stack => "stacks",
        }
    }

    /// Get the file extension suffix used in naming
    pub fn extension_pattern(&self) -> &'static str {
        match self {
            AdapterKind::Subject => ".aos",
            AdapterKind::Domain => ".domain.aos",
            AdapterKind::Specialized => ".aos.",
            AdapterKind::Stack => ".stack.aos",
        }
    }
}

impl fmt::Display for AdapterKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterKind::Subject => write!(f, "subject"),
            AdapterKind::Domain => write!(f, "domain"),
            AdapterKind::Specialized => write!(f, "specialized"),
            AdapterKind::Stack => write!(f, "stack"),
        }
    }
}

/// Parsed adapter name with kind classification
///
/// Supports the naming convention:
/// - `developer.aos` → Subject
/// - `actions.domain.aos` → Domain
/// - `developer.aos.actions` → Specialized
/// - `dev-full.stack.aos` → Stack
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AdapterName {
    /// The adapter classification
    pub kind: AdapterKind,
    /// Subject name (for Subject and Specialized kinds)
    pub subject: Option<String>,
    /// Domain name (for Domain and Specialized kinds)
    pub domain: Option<String>,
    /// Full canonical name
    pub name: String,
}

impl AdapterName {
    /// Create a new subject adapter name
    pub fn subject(name: &str) -> Self {
        Self {
            kind: AdapterKind::Subject,
            subject: Some(name.to_string()),
            domain: None,
            name: format!("{}.aos", name),
        }
    }

    /// Create a new domain adapter name
    pub fn domain(name: &str) -> Self {
        Self {
            kind: AdapterKind::Domain,
            subject: None,
            domain: Some(name.to_string()),
            name: format!("{}.domain.aos", name),
        }
    }

    /// Create a new specialized adapter name
    pub fn specialized(subject: &str, domain: &str) -> Self {
        Self {
            kind: AdapterKind::Specialized,
            subject: Some(subject.to_string()),
            domain: Some(domain.to_string()),
            name: format!("{}.aos.{}", subject, domain),
        }
    }

    /// Create a new stack adapter name
    pub fn stack(name: &str) -> Self {
        Self {
            kind: AdapterKind::Stack,
            subject: None,
            domain: None,
            name: format!("{}.stack.aos", name),
        }
    }

    /// Parse an adapter name from a string
    ///
    /// Naming conventions:
    /// - `developer.aos` → Subject
    /// - `actions.domain.aos` → Domain
    /// - `developer.aos.actions` → Specialized
    /// - `dev-full.stack.aos` → Stack
    pub fn parse(input: &str) -> Result<Self, AdapterNameError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AdapterNameError::Empty);
        }

        // Validate no path separators
        if input.contains('/') || input.contains('\\') {
            return Err(AdapterNameError::InvalidCharacters(
                "path separators not allowed".to_string(),
            ));
        }

        // Check for stack: name.stack.aos
        if input.ends_with(".stack.aos") {
            let name = input.trim_end_matches(".stack.aos");
            validate_name_segment(name)?;
            return Ok(Self::stack(name));
        }

        // Check for domain: name.domain.aos
        if input.ends_with(".domain.aos") {
            let name = input.trim_end_matches(".domain.aos");
            validate_name_segment(name)?;
            return Ok(Self::domain(name));
        }

        // Check for specialized: subject.aos.domain
        // Pattern: {subject}.aos.{domain}
        if let Some(aos_pos) = input.find(".aos.") {
            let subject = &input[..aos_pos];
            let domain = &input[aos_pos + 5..]; // Skip ".aos."
            validate_name_segment(subject)?;
            validate_name_segment(domain)?;
            return Ok(Self::specialized(subject, domain));
        }

        // Check for subject: name.aos
        if input.ends_with(".aos") {
            let name = input.trim_end_matches(".aos");
            validate_name_segment(name)?;
            return Ok(Self::subject(name));
        }

        // If no extension, treat as subject and add .aos
        validate_name_segment(input)?;
        Ok(Self::subject(input))
    }

    /// Get the refs directory path relative to adapters root
    pub fn refs_dir(&self, tenant_id: &str) -> PathBuf {
        let base_name = match self.kind {
            AdapterKind::Subject => self.subject.as_ref().unwrap(),
            AdapterKind::Domain => self.domain.as_ref().unwrap(),
            AdapterKind::Specialized => {
                // Use "subject.domain" as directory name
                return PathBuf::from(self.kind.dir_name())
                    .join(tenant_id)
                    .join(format!(
                        "{}.{}",
                        self.subject.as_ref().unwrap(),
                        self.domain.as_ref().unwrap()
                    ))
                    .join("refs");
            }
            AdapterKind::Stack => {
                // Stacks use "versions" not "refs"
                let stem = self.name.trim_end_matches(".stack.aos");
                return PathBuf::from(self.kind.dir_name())
                    .join(tenant_id)
                    .join(stem)
                    .join("versions");
            }
        };

        PathBuf::from(self.kind.dir_name())
            .join(tenant_id)
            .join(base_name)
            .join("refs")
    }

    /// Get the full filename
    pub fn filename(&self) -> &str {
        &self.name
    }
}

impl FromStr for AdapterName {
    type Err = AdapterNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for AdapterName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Errors that can occur when parsing adapter names
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterNameError {
    /// Empty name
    Empty,
    /// Invalid characters in name
    InvalidCharacters(String),
    /// Invalid segment (reserved name, etc.)
    InvalidSegment(String),
}

impl fmt::Display for AdapterNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterNameError::Empty => write!(f, "adapter name cannot be empty"),
            AdapterNameError::InvalidCharacters(msg) => {
                write!(f, "invalid characters in adapter name: {}", msg)
            }
            AdapterNameError::InvalidSegment(msg) => {
                write!(f, "invalid name segment: {}", msg)
            }
        }
    }
}

impl std::error::Error for AdapterNameError {}

/// Validate a name segment (subject, domain, stack name)
fn validate_name_segment(segment: &str) -> Result<(), AdapterNameError> {
    if segment.is_empty() {
        return Err(AdapterNameError::InvalidSegment("empty segment".to_string()));
    }

    if segment == "." || segment == ".." {
        return Err(AdapterNameError::InvalidSegment(
            "reserved name".to_string(),
        ));
    }

    // Check for valid characters: alphanumeric, dash, underscore
    for c in segment.chars() {
        if !c.is_alphanumeric() && c != '-' && c != '_' {
            return Err(AdapterNameError::InvalidCharacters(format!(
                "invalid character '{}' in segment '{}'",
                c, segment
            )));
        }
    }

    Ok(())
}

/// Well-known ref names
pub mod refs {
    /// The current/active version
    pub const CURRENT: &str = "current";
    /// The previous version (before last promotion)
    pub const PREVIOUS: &str = "previous";
    /// Draft/work-in-progress version
    pub const DRAFT: &str = "draft";
    /// Stable release marker
    pub const STABLE: &str = "stable";
}

/// A named reference to an adapter version
///
/// Refs are lightweight pointers to content-addressed adapter objects.
/// Common refs: `current`, `previous`, `draft`, `v1`, `v2`, `stable`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterRef {
    /// The adapter name this ref belongs to
    pub adapter_name: AdapterName,
    /// The ref name (e.g., "current", "v1", "draft")
    pub ref_name: String,
    /// Target content hash (BLAKE3)
    pub target_hash: String,
    /// Last update timestamp (RFC 3339)
    pub updated_at: String,
}

impl AdapterRef {
    /// Create a new ref
    pub fn new(
        adapter_name: AdapterName,
        ref_name: impl Into<String>,
        target_hash: impl Into<String>,
    ) -> Self {
        Self {
            adapter_name,
            ref_name: ref_name.into(),
            target_hash: target_hash.into(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Check if this is a version tag (v1, v2, etc.)
    pub fn is_version_tag(&self) -> bool {
        self.ref_name.starts_with('v')
            && self.ref_name[1..].chars().all(|c| c.is_ascii_digit() || c == '.')
    }

    /// Parse a version tag (v1, v1.2, v1.2.3) into components
    pub fn parse_version(&self) -> Option<(u32, u32, u32)> {
        if !self.is_version_tag() {
            return None;
        }

        let version_str = &self.ref_name[1..]; // Skip 'v'
        let parts: Vec<&str> = version_str.split('.').collect();

        let major = parts.first()?.parse().ok()?;
        let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

        Some((major, minor, patch))
    }
}

/// Adapter version metadata stored in the index
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterVersion {
    /// Content hash (BLAKE3) - also the object identity
    pub hash: String,
    /// The adapter name
    pub name: AdapterName,
    /// Semantic version string (e.g., "1.0.0")
    pub version: String,
    /// Parent version hash (for lineage tracking)
    pub parent_hash: Option<String>,
    /// Creation timestamp (RFC 3339)
    pub created_at: String,
    /// Size in bytes
    pub size_bytes: u64,
    /// Base model this adapter was trained on
    pub base_model: Option<String>,
    /// Training metrics (loss, etc.)
    pub training_metrics: Option<TrainingMetrics>,
    /// Arbitrary metadata
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

impl AdapterVersion {
    /// Create a new adapter version
    pub fn new(hash: impl Into<String>, name: AdapterName, version: impl Into<String>) -> Self {
        Self {
            hash: hash.into(),
            name,
            version: version.into(),
            parent_hash: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            size_bytes: 0,
            base_model: None,
            training_metrics: None,
            metadata: Default::default(),
        }
    }

    /// Set the parent hash for lineage tracking
    pub fn with_parent(mut self, parent_hash: impl Into<String>) -> Self {
        self.parent_hash = Some(parent_hash.into());
        self
    }

    /// Set the size in bytes
    pub fn with_size(mut self, size_bytes: u64) -> Self {
        self.size_bytes = size_bytes;
        self
    }

    /// Set the base model
    pub fn with_base_model(mut self, base_model: impl Into<String>) -> Self {
        self.base_model = Some(base_model.into());
        self
    }

    /// Get the object path relative to the objects directory
    ///
    /// Layout: `{hash[0:2]}/{hash[2:10]}/{full_hash}.aos`
    pub fn object_path(&self) -> PathBuf {
        object_path_from_hash(&self.hash)
    }
}

/// Training metrics for an adapter version
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrainingMetrics {
    /// Final training loss
    pub final_loss: f64,
    /// Number of training epochs
    pub epochs: u32,
    /// Number of training steps
    pub steps: u32,
    /// Learning rate used
    pub learning_rate: Option<f64>,
    /// Validation loss if available
    pub validation_loss: Option<f64>,
}

/// Stack definition (composition of adapters)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackDefinition {
    /// Stack name
    pub name: AdapterName,
    /// Stack version
    pub version: String,
    /// Component adapters with their versions
    pub components: Vec<StackComponent>,
    /// Creation timestamp
    pub created_at: String,
    /// Description of the stack
    pub description: Option<String>,
}

/// A component in a stack definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackComponent {
    /// Adapter name
    pub adapter: AdapterName,
    /// Pinned ref name (e.g., "current", "v1", "stable")
    pub ref_name: String,
    /// Resolved hash at stack creation time
    pub resolved_hash: String,
    /// Blend weight (0.0 - 1.0)
    #[serde(default = "default_weight")]
    pub weight: f64,
}

fn default_weight() -> f64 {
    1.0
}

impl StackDefinition {
    /// Create a new stack definition
    pub fn new(name: AdapterName, version: impl Into<String>) -> Self {
        Self {
            name,
            version: version.into(),
            components: Vec::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            description: None,
        }
    }

    /// Add a component to the stack
    pub fn add_component(
        mut self,
        adapter: AdapterName,
        ref_name: impl Into<String>,
        resolved_hash: impl Into<String>,
    ) -> Self {
        self.components.push(StackComponent {
            adapter,
            ref_name: ref_name.into(),
            resolved_hash: resolved_hash.into(),
            weight: 1.0,
        });
        self
    }

    /// Add a weighted component to the stack
    pub fn add_weighted_component(
        mut self,
        adapter: AdapterName,
        ref_name: impl Into<String>,
        resolved_hash: impl Into<String>,
        weight: f64,
    ) -> Self {
        self.components.push(StackComponent {
            adapter,
            ref_name: ref_name.into(),
            resolved_hash: resolved_hash.into(),
            weight,
        });
        self
    }
}

/// Compute the object storage path from a content hash
///
/// Layout: `{hash[0:2]}/{hash[2:10]}/{full_hash}.aos`
pub fn object_path_from_hash(hash: &str) -> PathBuf {
    let prefix_2 = hash.get(0..2).unwrap_or("00");
    let prefix_8 = hash.get(2..10).unwrap_or("00000000");
    PathBuf::from(prefix_2)
        .join(prefix_8)
        .join(format!("{}.aos", hash))
}

/// Adapter storage layout paths
#[derive(Debug, Clone)]
pub struct AdapterLayout {
    /// Root directory (e.g., var/adapters)
    pub root: PathBuf,
}

impl AdapterLayout {
    /// Create a new layout with the given root
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Get the objects directory (content-addressed store)
    pub fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    /// Get the path for an object by hash
    pub fn object_path(&self, hash: &str) -> PathBuf {
        self.objects_dir().join(object_path_from_hash(hash))
    }

    /// Get the subjects directory
    pub fn subjects_dir(&self) -> PathBuf {
        self.root.join("subjects")
    }

    /// Get the domains directory
    pub fn domains_dir(&self) -> PathBuf {
        self.root.join("domains")
    }

    /// Get the specialized directory
    pub fn specialized_dir(&self) -> PathBuf {
        self.root.join("specialized")
    }

    /// Get the stacks directory
    pub fn stacks_dir(&self) -> PathBuf {
        self.root.join("stacks")
    }

    /// Get the refs directory for an adapter
    pub fn refs_dir(&self, adapter: &AdapterName, tenant_id: &str) -> PathBuf {
        self.root.join(adapter.refs_dir(tenant_id))
    }

    /// Get the path for a specific ref
    pub fn ref_path(&self, adapter: &AdapterName, tenant_id: &str, ref_name: &str) -> PathBuf {
        self.refs_dir(adapter, tenant_id).join(ref_name)
    }

    /// Get the index database path
    pub fn index_path(&self) -> PathBuf {
        self.root.join("index.redb")
    }

    /// Get the legacy repo directory (for migration)
    pub fn legacy_repo_dir(&self) -> PathBuf {
        self.root.join("repo")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_subject_adapter() {
        let name = AdapterName::parse("developer.aos").unwrap();
        assert_eq!(name.kind, AdapterKind::Subject);
        assert_eq!(name.subject, Some("developer".to_string()));
        assert_eq!(name.domain, None);
        assert_eq!(name.name, "developer.aos");
    }

    #[test]
    fn parse_domain_adapter() {
        let name = AdapterName::parse("actions.domain.aos").unwrap();
        assert_eq!(name.kind, AdapterKind::Domain);
        assert_eq!(name.subject, None);
        assert_eq!(name.domain, Some("actions".to_string()));
        assert_eq!(name.name, "actions.domain.aos");
    }

    #[test]
    fn parse_specialized_adapter() {
        let name = AdapterName::parse("developer.aos.actions").unwrap();
        assert_eq!(name.kind, AdapterKind::Specialized);
        assert_eq!(name.subject, Some("developer".to_string()));
        assert_eq!(name.domain, Some("actions".to_string()));
        assert_eq!(name.name, "developer.aos.actions");
    }

    #[test]
    fn parse_stack_adapter() {
        let name = AdapterName::parse("dev-full.stack.aos").unwrap();
        assert_eq!(name.kind, AdapterKind::Stack);
        assert_eq!(name.subject, None);
        assert_eq!(name.domain, None);
        assert_eq!(name.name, "dev-full.stack.aos");
    }

    #[test]
    fn parse_bare_name_defaults_to_subject() {
        let name = AdapterName::parse("mymodel").unwrap();
        assert_eq!(name.kind, AdapterKind::Subject);
        assert_eq!(name.name, "mymodel.aos");
    }

    #[test]
    fn parse_empty_fails() {
        assert!(AdapterName::parse("").is_err());
    }

    #[test]
    fn parse_path_separator_fails() {
        assert!(AdapterName::parse("my/adapter.aos").is_err());
        assert!(AdapterName::parse("my\\adapter.aos").is_err());
    }

    #[test]
    fn refs_dir_subject() {
        let name = AdapterName::subject("developer");
        let refs_dir = name.refs_dir("tenant-1");
        assert_eq!(refs_dir, PathBuf::from("subjects/tenant-1/developer/refs"));
    }

    #[test]
    fn refs_dir_domain() {
        let name = AdapterName::domain("actions");
        let refs_dir = name.refs_dir("tenant-1");
        assert_eq!(refs_dir, PathBuf::from("domains/tenant-1/actions/refs"));
    }

    #[test]
    fn refs_dir_specialized() {
        let name = AdapterName::specialized("developer", "actions");
        let refs_dir = name.refs_dir("tenant-1");
        assert_eq!(
            refs_dir,
            PathBuf::from("specialized/tenant-1/developer.actions/refs")
        );
    }

    #[test]
    fn refs_dir_stack() {
        let name = AdapterName::stack("dev-full");
        let refs_dir = name.refs_dir("tenant-1");
        assert_eq!(refs_dir, PathBuf::from("stacks/tenant-1/dev-full/versions"));
    }

    #[test]
    fn object_path_from_hash_layout() {
        let hash = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let path = object_path_from_hash(hash);
        assert_eq!(
            path,
            PathBuf::from("ab/cdef1234/abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890.aos")
        );
    }

    #[test]
    fn adapter_ref_version_parsing() {
        let name = AdapterName::subject("test");

        let ref_v1 = AdapterRef::new(name.clone(), "v1", "hash1");
        assert!(ref_v1.is_version_tag());
        assert_eq!(ref_v1.parse_version(), Some((1, 0, 0)));

        let ref_v1_2_3 = AdapterRef::new(name.clone(), "v1.2.3", "hash2");
        assert!(ref_v1_2_3.is_version_tag());
        assert_eq!(ref_v1_2_3.parse_version(), Some((1, 2, 3)));

        let ref_current = AdapterRef::new(name, "current", "hash3");
        assert!(!ref_current.is_version_tag());
        assert_eq!(ref_current.parse_version(), None);
    }

    #[test]
    fn adapter_layout_paths() {
        let layout = AdapterLayout::new("/var/adapters");

        assert_eq!(layout.objects_dir(), PathBuf::from("/var/adapters/objects"));
        assert_eq!(
            layout.subjects_dir(),
            PathBuf::from("/var/adapters/subjects")
        );
        assert_eq!(layout.domains_dir(), PathBuf::from("/var/adapters/domains"));
        assert_eq!(
            layout.index_path(),
            PathBuf::from("/var/adapters/index.redb")
        );

        let hash = "abcdef1234567890abcdef1234567890";
        let obj_path = layout.object_path(hash);
        assert_eq!(
            obj_path,
            PathBuf::from("/var/adapters/objects/ab/cdef1234/abcdef1234567890abcdef1234567890.aos")
        );
    }
}
