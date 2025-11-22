//! Bundle Store - Content-Addressed Storage for Telemetry Bundles
//!
//! Implements:
//! - Content-addressed storage with BLAKE3 hashing
//! - Retention policies (Retention Ruleset #10)
//! - Bundle chaining and verification
//! - Garbage collection with policy enforcement
//! - Incident bundle preservation
//! - Promotion bundle tracking

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing;

// Re-export canonical BundleMetadata from adapteros-telemetry-types
pub use adapteros_telemetry_types::BundleMetadata;

/// Bundle Store Manager
pub struct BundleStore {
    /// Root directory for bundle storage
    root_dir: PathBuf,
    /// Bundle index (bundle_hash -> metadata)
    index: HashMap<B3Hash, BundleMetadata>,
    /// Retention policy
    policy: RetentionPolicy,
}

/// Retention policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Keep the last K bundles per CPID
    pub keep_bundles_per_cpid: usize,
    /// Keep all bundles referenced by open incident reports
    pub keep_incident_bundles: bool,
    /// Keep at least one "promotion bundle" per CP promotion for provenance
    pub keep_promotion_bundles: bool,
    /// Eviction strategy: oldest_first_safe (never delete incident/promotion bundles)
    pub evict_strategy: EvictionStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EvictionStrategy {
    OldestFirstSafe,
    LeastRecentlyUsed,
    Custom,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            keep_bundles_per_cpid: 12,
            keep_incident_bundles: true,
            keep_promotion_bundles: true,
            evict_strategy: EvictionStrategy::OldestFirstSafe,
        }
    }
}

impl BundleStore {
    /// Create a new bundle store
    pub fn new<P: AsRef<Path>>(root_dir: P, policy: RetentionPolicy) -> Result<Self> {
        let root_dir = root_dir.as_ref().to_path_buf();
        fs::create_dir_all(&root_dir)?;

        let mut store = Self {
            root_dir,
            index: HashMap::new(),
            policy,
        };

        // Load existing bundle index
        store.rebuild_index()?;

        Ok(store)
    }

    /// Store a bundle with content-addressed naming
    pub fn store_bundle(&mut self, bundle_data: &[u8], metadata: BundleMetadata) -> Result<B3Hash> {
        // Compute bundle hash (content-addressed)
        let bundle_hash = B3Hash::hash(bundle_data);

        // Check if bundle already exists
        if self.index.contains_key(&bundle_hash) {
            tracing::debug!("Bundle {} already exists in store", bundle_hash);
            return Ok(bundle_hash);
        }

        // Store bundle file: root_dir/{tenant_id}/bundles/{hash}.ndjson
        let tenant_dir = self
            .root_dir
            .join(metadata.tenant_id.as_deref().unwrap_or("default"))
            .join("bundles");
        fs::create_dir_all(&tenant_dir)?;

        let bundle_path = tenant_dir.join(format!("{}.ndjson", bundle_hash));
        fs::write(&bundle_path, bundle_data)?;

        // Store metadata file
        let meta_path = tenant_dir.join(format!("{}.meta.json", bundle_hash));
        let meta_json = serde_json::to_string_pretty(&metadata)?;
        fs::write(&meta_path, meta_json)?;

        // Update index
        self.index.insert(bundle_hash, metadata);

        tracing::info!(
            "Stored bundle {} ({} bytes)",
            bundle_hash,
            bundle_data.len()
        );

        Ok(bundle_hash)
    }

    /// Retrieve a bundle by hash
    pub fn get_bundle(&self, bundle_hash: &B3Hash) -> Result<Vec<u8>> {
        let metadata = self
            .index
            .get(bundle_hash)
            .ok_or_else(|| AosError::Telemetry(format!("Bundle {} not found", bundle_hash)))?;

        let bundle_path = self
            .root_dir
            .join(metadata.tenant_id.as_deref().unwrap_or("default"))
            .join("bundles")
            .join(format!("{}.ndjson", bundle_hash));

        let bundle_data = fs::read(&bundle_path)?;

        // Verify content-addressed hash
        let computed_hash = B3Hash::hash(&bundle_data);
        if computed_hash != *bundle_hash {
            return Err(AosError::Telemetry(format!(
                "Bundle hash mismatch: expected {}, got {}",
                bundle_hash, computed_hash
            )));
        }

        Ok(bundle_data)
    }

    /// Get bundle events by bundle ID (hex hash)
    pub fn get_bundle_events(&self, bundle_id_hex: &str) -> Result<Vec<Value>> {
        let bundle_hash = B3Hash::from_hex(bundle_id_hex)
            .map_err(|_| AosError::Parse("Invalid bundle ID format".to_string()))?;

        let data = self.get_bundle(&bundle_hash)?;

        let data_str = std::str::from_utf8(&data)
            .map_err(|e| AosError::Telemetry(format!("Invalid UTF-8 in bundle: {}", e)))?;

        let mut events = Vec::new();

        for line in data_str.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                match serde_json::from_str::<Value>(trimmed) {
                    Ok(event) => events.push(event),
                    Err(e) => tracing::warn!(
                        "Failed to parse event line in bundle {}: {}",
                        bundle_id_hex,
                        e
                    ),
                }
            }
        }

        if events.is_empty() {
            return Err(AosError::Telemetry(
                "No valid events found in bundle".to_string(),
            ));
        }

        Ok(events)
    }

    /// Get bundle metadata
    pub fn get_metadata(&self, bundle_hash: &B3Hash) -> Option<&BundleMetadata> {
        self.index.get(bundle_hash)
    }

    /// List all bundles for a CPID
    pub fn list_bundles_for_cpid(&self, cpid: &str) -> Vec<&BundleMetadata> {
        let mut bundles: Vec<&BundleMetadata> = self
            .index
            .values()
            .filter(|m| m.cpid.as_ref().map(|c| c == cpid).unwrap_or(false))
            .collect();

        // Sort by sequence number (oldest first)
        bundles.sort_by_key(|m| m.sequence_no);

        bundles
    }

    /// List all bundles for a tenant
    pub fn list_bundles_for_tenant(&self, tenant_id: &str) -> Vec<&BundleMetadata> {
        let mut bundles: Vec<&BundleMetadata> = self
            .index
            .values()
            .filter(|m| m.tenant_id.as_deref() == Some(tenant_id))
            .collect();

        bundles.sort_by_key(|m| m.sequence_no);

        bundles
    }

    /// Mark bundle as incident-related (protect from GC)
    pub fn mark_incident_bundle(&mut self, bundle_hash: &B3Hash) -> Result<()> {
        {
            let metadata = self
                .index
                .get_mut(bundle_hash)
                .ok_or_else(|| AosError::Telemetry(format!("Bundle {} not found", bundle_hash)))?;

            metadata.is_incident_bundle = true;
        }

        // Update metadata file after releasing mutable borrow
        let metadata = self.index.get(bundle_hash).unwrap();
        self.update_metadata_file(bundle_hash, metadata)?;

        tracing::info!("Marked bundle {} as incident bundle", bundle_hash);

        Ok(())
    }

    /// Mark bundle as promotion bundle (protect from GC)
    pub fn mark_promotion_bundle(&mut self, bundle_hash: &B3Hash) -> Result<()> {
        {
            let metadata = self
                .index
                .get_mut(bundle_hash)
                .ok_or_else(|| AosError::Telemetry(format!("Bundle {} not found", bundle_hash)))?;

            metadata.is_promotion_bundle = true;
        }

        // Update metadata file after releasing mutable borrow
        let metadata = self.index.get(bundle_hash).unwrap();
        self.update_metadata_file(bundle_hash, metadata)?;

        tracing::info!("Marked bundle {} as promotion bundle", bundle_hash);

        Ok(())
    }

    /// Run garbage collection based on retention policy
    pub fn run_gc(&mut self) -> Result<GarbageCollectionReport> {
        let mut report = GarbageCollectionReport {
            total_bundles: self.index.len(),
            evicted_bundles: Vec::new(),
            retained_bundles: self.index.len(),
            bytes_freed: 0,
        };

        // Group bundles by CPID
        let mut bundles_by_cpid: HashMap<String, Vec<B3Hash>> = HashMap::new();
        for (hash, metadata) in &self.index {
            if let Some(cpid) = &metadata.cpid {
                bundles_by_cpid.entry(cpid.clone()).or_default().push(*hash);
            }
        }

        // Apply retention policy per CPID
        for (_cpid, mut bundles) in bundles_by_cpid {
            // Sort by sequence number (oldest first)
            bundles.sort_by_key(|hash| self.index.get(hash).unwrap().sequence_no);

            // Determine which bundles to evict
            if bundles.len() > self.policy.keep_bundles_per_cpid {
                let to_evict = bundles.len() - self.policy.keep_bundles_per_cpid;

                for bundle_hash in bundles.iter().take(to_evict) {
                    let metadata = self.index.get(bundle_hash).unwrap();

                    // Check if bundle is protected
                    if self.policy.keep_incident_bundles && metadata.is_incident_bundle {
                        tracing::debug!("Skipping incident bundle {} from eviction", bundle_hash);
                        continue;
                    }

                    if self.policy.keep_promotion_bundles && metadata.is_promotion_bundle {
                        tracing::debug!("Skipping promotion bundle {} from eviction", bundle_hash);
                        continue;
                    }

                    // Safe to evict
                    let bytes_freed = self.evict_bundle(bundle_hash)?;
                    report.evicted_bundles.push(*bundle_hash);
                    report.bytes_freed += bytes_freed;
                }
            }
        }

        report.retained_bundles = self.index.len();

        tracing::info!(
            "GC complete: evicted {} bundles, freed {} bytes, retained {} bundles",
            report.evicted_bundles.len(),
            report.bytes_freed,
            report.retained_bundles
        );

        Ok(report)
    }

    /// Evict a bundle from storage
    fn evict_bundle(&mut self, bundle_hash: &B3Hash) -> Result<u64> {
        let metadata = self
            .index
            .remove(bundle_hash)
            .ok_or_else(|| AosError::Telemetry(format!("Bundle {} not found", bundle_hash)))?;

        let tenant_dir = self
            .root_dir
            .join(metadata.tenant_id.as_deref().unwrap_or("default"))
            .join("bundles");
        let bundle_path = tenant_dir.join(format!("{}.ndjson", bundle_hash));
        let meta_path = tenant_dir.join(format!("{}.meta.json", bundle_hash));

        // Get file size before deletion
        let bundle_size = fs::metadata(&bundle_path)?.len();

        // Delete files
        fs::remove_file(&bundle_path)?;
        fs::remove_file(&meta_path)?;

        tracing::debug!("Evicted bundle {} ({} bytes)", bundle_hash, bundle_size);

        Ok(bundle_size)
    }

    /// Rebuild index from disk
    fn rebuild_index(&mut self) -> Result<()> {
        self.index.clear();

        // Scan all tenant directories
        for entry in fs::read_dir(&self.root_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let tenant_dir = entry.path().join("bundles");
            if !tenant_dir.exists() {
                continue;
            }

            // Load all .meta.json files
            for meta_entry in fs::read_dir(&tenant_dir)? {
                let meta_entry = meta_entry?;
                let path = meta_entry.path();

                if path.extension().and_then(|s| s.to_str()) == Some("json")
                    && path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.ends_with(".meta"))
                        .unwrap_or(false)
                {
                    let meta_json = fs::read_to_string(&path)?;
                    let metadata: BundleMetadata = serde_json::from_str(&meta_json)?;
                    self.index.insert(metadata.bundle_hash, metadata);
                }
            }
        }

        tracing::info!("Rebuilt index with {} bundles", self.index.len());

        Ok(())
    }

    /// Update metadata file on disk
    fn update_metadata_file(&self, bundle_hash: &B3Hash, metadata: &BundleMetadata) -> Result<()> {
        let tenant_dir = self
            .root_dir
            .join(metadata.tenant_id.as_deref().unwrap_or("default"))
            .join("bundles");
        let meta_path = tenant_dir.join(format!("{}.meta.json", bundle_hash));
        let meta_json = serde_json::to_string_pretty(metadata)?;
        fs::write(&meta_path, meta_json)?;
        Ok(())
    }

    /// Verify bundle chain integrity
    pub fn verify_chain(&self, cpid: &str) -> Result<ChainVerificationReport> {
        let bundles = self.list_bundles_for_cpid(cpid);

        let mut report = ChainVerificationReport {
            cpid: cpid.to_string(),
            total_bundles: bundles.len(),
            verified_bundles: 0,
            broken_links: Vec::new(),
        };

        for i in 1..bundles.len() {
            let current = bundles[i];
            let prev = bundles[i - 1];

            // Verify chain link
            if current.prev_bundle_hash.as_ref() != Some(&prev.merkle_root) {
                report.broken_links.push(format!(
                    "Bundle {} does not link to previous bundle {}",
                    current.bundle_hash, prev.bundle_hash
                ));
            } else {
                report.verified_bundles += 1;
            }
        }

        Ok(report)
    }

    /// Get storage statistics
    pub fn get_stats(&self) -> StorageStats {
        let mut stats = StorageStats {
            total_bundles: self.index.len(),
            incident_bundles: 0,
            promotion_bundles: 0,
            total_bytes: 0,
        };

        for metadata in self.index.values() {
            if metadata.is_incident_bundle {
                stats.incident_bundles += 1;
            }
            if metadata.is_promotion_bundle {
                stats.promotion_bundles += 1;
            }

            // Try to get bundle size
            let bundle_path = self
                .root_dir
                .join(metadata.tenant_id.as_deref().unwrap_or("default"))
                .join("bundles")
                .join(format!("{}.ndjson", metadata.bundle_hash));

            if let Ok(meta) = fs::metadata(&bundle_path) {
                stats.total_bytes += meta.len();
            }
        }

        stats
    }
}

/// Garbage collection report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GarbageCollectionReport {
    pub total_bundles: usize,
    pub evicted_bundles: Vec<B3Hash>,
    pub retained_bundles: usize,
    pub bytes_freed: u64,
}

/// Chain verification report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainVerificationReport {
    pub cpid: String,
    pub total_bundles: usize,
    pub verified_bundles: usize,
    pub broken_links: Vec<String>,
}

/// Storage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_bundles: usize,
    pub incident_bundles: usize,
    pub promotion_bundles: usize,
    pub total_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    use tempfile::TempDir;

    #[test]
    fn test_bundle_store_content_addressing() {
        let temp_dir = TempDir::new().unwrap();
        let policy = RetentionPolicy::default();
        let mut store = BundleStore::new(temp_dir.path(), policy).unwrap();

        let bundle_data = b"test bundle content";
        let metadata = BundleMetadata {
            bundle_hash: B3Hash::hash(bundle_data),
            cpid: Some("cpid-001".to_string()),
            tenant_id: Some("tenant-test".to_string()),
            event_count: 10,
            sequence_no: Some(1),
            merkle_root: B3Hash::hash(b"merkle"),
            signature: "sig".to_string(),
            public_key: "test_pubkey".to_string(),
            key_id: "test_key_id".to_string(),
            schema_version: 1,
            signed_at_us: 0,
            created_at: SystemTime::now(),
            prev_bundle_hash: None,
            is_incident_bundle: false,
            is_promotion_bundle: false,
            tags: vec![],
            stack_id: None,
            stack_version: None,
        };

        // Store bundle
        let bundle_hash = store.store_bundle(bundle_data, metadata.clone()).unwrap();

        // Retrieve bundle
        let retrieved = store.get_bundle(&bundle_hash).unwrap();
        assert_eq!(retrieved, bundle_data);

        // Verify metadata
        let meta = store.get_metadata(&bundle_hash).unwrap();
        assert_eq!(meta.cpid, Some("cpid-001".to_string()));
        assert_eq!(meta.event_count, 10);
    }

    #[test]
    fn test_retention_policy() {
        let temp_dir = TempDir::new().unwrap();
        let policy = RetentionPolicy {
            keep_bundles_per_cpid: 2,
            ..Default::default()
        };
        let mut store = BundleStore::new(temp_dir.path(), policy).unwrap();

        // Add 3 bundles for same CPID
        for i in 0..3 {
            let bundle_data = format!("bundle {}", i);
            let metadata = BundleMetadata {
                bundle_hash: B3Hash::hash(bundle_data.as_bytes()),
                cpid: Some("cpid-001".to_string()),
                tenant_id: Some("tenant-test".to_string()),
                event_count: 10,
                sequence_no: Some(i),
                merkle_root: B3Hash::hash(b"merkle"),
                signature: "sig".to_string(),
                public_key: "test_pubkey".to_string(),
                key_id: "test_key_id".to_string(),
                schema_version: 1,
                signed_at_us: 0,
                created_at: SystemTime::now(),
                prev_bundle_hash: None,
                is_incident_bundle: false,
                is_promotion_bundle: false,
                tags: vec![],
                stack_id: None,
                stack_version: None,
            };
            store
                .store_bundle(bundle_data.as_bytes(), metadata)
                .unwrap();
        }

        // Run GC
        let report = store.run_gc().unwrap();

        // Should evict oldest bundle (keep_bundles_per_cpid = 2)
        assert_eq!(report.evicted_bundles.len(), 1);
        assert_eq!(report.retained_bundles, 2);
    }

    #[test]
    fn test_incident_bundle_protection() {
        let temp_dir = TempDir::new().unwrap();
        let policy = RetentionPolicy {
            keep_bundles_per_cpid: 1,
            keep_incident_bundles: true,
            ..Default::default()
        };
        let mut store = BundleStore::new(temp_dir.path(), policy).unwrap();

        // Add 2 bundles, mark first as incident
        let bundle1_data = b"bundle 1";
        let bundle1_hash = B3Hash::hash(bundle1_data);
        let metadata1 = BundleMetadata {
            bundle_hash: bundle1_hash,
            cpid: Some("cpid-001".to_string()),
            tenant_id: Some("tenant-test".to_string()),
            event_count: 10,
            sequence_no: Some(1),
            merkle_root: B3Hash::hash(b"merkle1"),
            signature: "sig1".to_string(),
            public_key: "test_pubkey1".to_string(),
            key_id: "test_key_id1".to_string(),
            schema_version: 1,
            signed_at_us: 0,
            created_at: SystemTime::now(),
            prev_bundle_hash: None,
            is_incident_bundle: false,
            is_promotion_bundle: false,
            tags: vec![],
            stack_id: None,
            stack_version: None,
        };
        store.store_bundle(bundle1_data, metadata1).unwrap();
        store.mark_incident_bundle(&bundle1_hash).unwrap();

        let bundle2_data = b"bundle 2";
        let metadata2 = BundleMetadata {
            bundle_hash: B3Hash::hash(bundle2_data),
            cpid: Some("cpid-001".to_string()),
            tenant_id: Some("tenant-test".to_string()),
            event_count: 10,
            sequence_no: Some(2),
            merkle_root: B3Hash::hash(b"merkle2"),
            signature: "sig2".to_string(),
            public_key: "test_pubkey2".to_string(),
            key_id: "test_key_id2".to_string(),
            schema_version: 1,
            signed_at_us: 0,
            created_at: SystemTime::now(),
            prev_bundle_hash: Some(B3Hash::hash(b"merkle1")),
            is_incident_bundle: false,
            is_promotion_bundle: false,
            tags: vec![],
            stack_id: None,
            stack_version: None,
        };
        store.store_bundle(bundle2_data, metadata2).unwrap();

        // Run GC - should keep both (incident bundle protected)
        let report = store.run_gc().unwrap();
        assert_eq!(report.evicted_bundles.len(), 0);
        assert_eq!(report.retained_bundles, 2);
    }
}
