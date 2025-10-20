//! Federation AOS replication protocol
//!
//! Syncs .aos files between federated nodes with content-addressed deduplication.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_registry::{AosMetadata, AosStore};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, info, warn};

/// AOS sync protocol message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AosSyncMessage {
    /// Announce available AOS files
    Announce(Vec<AosAnnouncement>),
    /// Request specific AOS file
    Request { aos_hash: B3Hash },
    /// Provide requested AOS file
    Provide { aos_hash: B3Hash, data: Vec<u8> },
    /// AOS file not found
    NotFound { aos_hash: B3Hash },
}

/// Announcement of available AOS file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AosAnnouncement {
    pub aos_hash: B3Hash,
    pub adapter_id: String,
    pub version: String,
    pub category: String,
    pub file_size: u64,
    pub format_version: u8,
    pub signature_valid: bool,
}

impl From<&AosMetadata> for AosAnnouncement {
    fn from(metadata: &AosMetadata) -> Self {
        Self {
            aos_hash: metadata.manifest_hash,
            adapter_id: metadata.adapter_id.clone(),
            version: metadata.version.clone(),
            category: metadata.category.clone(),
            file_size: metadata.file_size,
            format_version: metadata.format_version,
            signature_valid: metadata.signature_valid,
        }
    }
}

/// AOS file sync coordinator
pub struct AosSyncCoordinator {
    /// Local AOS store
    local_store: Arc<AosStore>,
    /// Sync strategy
    strategy: SyncStrategy,
}

impl AosSyncCoordinator {
    /// Create new sync coordinator
    pub fn new(local_store: Arc<AosStore>, strategy: SyncStrategy) -> Self {
        Self {
            local_store,
            strategy,
        }
    }

    /// Generate announcements for local AOS files
    pub fn generate_announcements(&self) -> Vec<AosAnnouncement> {
        self.local_store
            .list_all()
            .iter()
            .map(AosAnnouncement::from)
            .collect()
    }

    /// Process remote announcements and determine what to fetch
    pub fn process_announcements(
        &self,
        remote_announcements: Vec<AosAnnouncement>,
    ) -> Vec<B3Hash> {
        let local_hashes: HashSet<B3Hash> = self
            .local_store
            .list_all()
            .iter()
            .map(|m| m.manifest_hash)
            .collect();

        let mut to_fetch = Vec::new();

        for announcement in remote_announcements {
            // Skip if already have it
            if local_hashes.contains(&announcement.aos_hash) {
                continue;
            }

            // Check if should fetch based on strategy
            if self.strategy.should_fetch(&announcement) {
                to_fetch.push(announcement.aos_hash);
            }
        }

        info!(
            "Identified {} AOS files to fetch from {} announcements",
            to_fetch.len(),
            remote_announcements.len()
        );

        to_fetch
    }

    /// Fetch and store remote AOS file
    pub async fn fetch_and_store(
        &self,
        aos_hash: &B3Hash,
        data: Vec<u8>,
    ) -> Result<()> {
        // Write to temporary file
        let temp_path = std::env::temp_dir().join(format!("{}.aos.tmp", aos_hash.to_hex()));
        fs::write(&temp_path, &data)
            .await
            .map_err(|e| AosError::Io(format!("Failed to write temp file: {}", e)))?;

        // Store via AOS store (validates and moves to proper location)
        let stored_hash = self.local_store.store(&temp_path).await?;

        // Verify hash matches
        if stored_hash != *aos_hash {
            return Err(AosError::Verification(format!(
                "Hash mismatch: expected {}, got {}",
                aos_hash.to_hex(),
                stored_hash.to_hex()
            )));
        }

        // Clean up temp file
        let _ = fs::remove_file(temp_path).await;

        info!("Fetched and stored AOS {}", aos_hash.to_hex());
        Ok(())
    }

    /// Provide local AOS file for sync
    pub async fn provide_aos(&self, aos_hash: &B3Hash) -> Result<Vec<u8>> {
        let aos_path = self.local_store.get(aos_hash)?;
        let data = fs::read(&aos_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read AOS file: {}", e)))?;

        debug!("Providing AOS {} ({} bytes)", aos_hash.to_hex(), data.len());
        Ok(data)
    }

    /// Full sync with remote peer
    pub async fn sync_with_peer<P: AosSyncPeer>(&self, peer: &mut P) -> Result<SyncResult> {
        let start = std::time::Instant::now();

        // Send local announcements
        let local_announcements = self.generate_announcements();
        peer.send(AosSyncMessage::Announce(local_announcements))
            .await?;

        // Receive remote announcements
        let remote_announcements = match peer.receive().await? {
            AosSyncMessage::Announce(ann) => ann,
            _ => {
                return Err(AosError::Federation(
                    "Expected Announce message".to_string(),
                ))
            }
        };

        // Determine what to fetch
        let to_fetch = self.process_announcements(remote_announcements);
        let mut fetched = 0;
        let mut failed = 0;

        // Fetch each needed AOS file
        for aos_hash in &to_fetch {
            peer.send(AosSyncMessage::Request {
                aos_hash: *aos_hash,
            })
            .await?;

            match peer.receive().await? {
                AosSyncMessage::Provide { aos_hash, data } => {
                    if let Err(e) = self.fetch_and_store(&aos_hash, data).await {
                        warn!("Failed to fetch AOS {}: {}", aos_hash.to_hex(), e);
                        failed += 1;
                    } else {
                        fetched += 1;
                    }
                }
                AosSyncMessage::NotFound { aos_hash } => {
                    warn!("Remote peer does not have AOS {}", aos_hash.to_hex());
                    failed += 1;
                }
                _ => {
                    return Err(AosError::Federation(
                        "Unexpected sync message".to_string(),
                    ))
                }
            }
        }

        let elapsed = start.elapsed();
        info!(
            "Sync complete: fetched {}, failed {}, in {:?}",
            fetched, failed, elapsed
        );

        Ok(SyncResult {
            fetched,
            failed,
            duration: elapsed,
        })
    }

    /// Export AOS files for offline transfer
    pub async fn export_to_directory<P: AsRef<Path>>(
        &self,
        export_dir: P,
        filter: Option<&str>,
    ) -> Result<usize> {
        fs::create_dir_all(&export_dir)
            .await
            .map_err(|e| AosError::Io(format!("Failed to create export directory: {}", e)))?;

        let mut count = 0;

        for metadata in self.local_store.list_all() {
            // Apply category filter if specified
            if let Some(category) = filter {
                if metadata.category != category {
                    continue;
                }
            }

            let aos_path = self.local_store.get(&metadata.manifest_hash)?;
            let export_path = export_dir
                .as_ref()
                .join(format!("{}.aos", metadata.manifest_hash.to_hex()));

            fs::copy(&aos_path, &export_path)
                .await
                .map_err(|e| AosError::Io(format!("Failed to export AOS: {}", e)))?;

            count += 1;
        }

        info!("Exported {} AOS files to {:?}", count, export_dir.as_ref());
        Ok(count)
    }

    /// Import AOS files from offline transfer
    pub async fn import_from_directory<P: AsRef<Path>>(&self, import_dir: P) -> Result<usize> {
        let mut count = 0;
        let mut entries = fs::read_dir(&import_dir)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read import directory: {}", e)))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("aos") {
                continue;
            }

            match self.local_store.store(&path).await {
                Ok(hash) => {
                    info!("Imported AOS {}", hash.to_hex());
                    count += 1;
                }
                Err(e) => {
                    warn!("Failed to import {:?}: {}", path, e);
                }
            }
        }

        info!("Imported {} AOS files from {:?}", count, import_dir.as_ref());
        Ok(count)
    }
}

/// Strategy for deciding which AOS files to sync
#[derive(Debug, Clone)]
pub enum SyncStrategy {
    /// Sync all available AOS files
    All,
    /// Sync only specific categories
    Categories(Vec<String>),
    /// Sync only signed AOS files
    SignedOnly,
    /// Sync files matching custom predicate
    Custom,
}

impl SyncStrategy {
    fn should_fetch(&self, announcement: &AosAnnouncement) -> bool {
        match self {
            SyncStrategy::All => true,
            SyncStrategy::Categories(cats) => cats.contains(&announcement.category),
            SyncStrategy::SignedOnly => announcement.signature_valid,
            SyncStrategy::Custom => true, // Would call custom predicate
        }
    }
}

/// Result of sync operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub fetched: usize,
    pub failed: usize,
    pub duration: std::time::Duration,
}

/// Trait for AOS sync peer (network, local file, etc.)
#[async_trait::async_trait]
pub trait AosSyncPeer {
    async fn send(&mut self, message: AosSyncMessage) -> Result<()>;
    async fn receive(&mut self) -> Result<AosSyncMessage>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_single_file_adapter::{
        LineageInfo, SingleFileAdapter, SingleFileAdapterPackager, TrainingConfig,
    };
    use std::collections::HashMap;
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    /// Mock peer for testing
    struct MockPeer {
        tx: mpsc::UnboundedSender<AosSyncMessage>,
        rx: mpsc::UnboundedReceiver<AosSyncMessage>,
    }

    #[async_trait::async_trait]
    impl AosSyncPeer for MockPeer {
        async fn send(&mut self, message: AosSyncMessage) -> Result<()> {
            self.tx
                .send(message)
                .map_err(|e| AosError::Federation(format!("Send failed: {}", e)))
        }

        async fn receive(&mut self) -> Result<AosSyncMessage> {
            self.rx
                .recv()
                .await
                .ok_or_else(|| AosError::Federation("Channel closed".to_string()))
        }
    }

    async fn create_test_aos(dir: &Path, adapter_id: &str) -> std::path::PathBuf {
        let aos_path = dir.join(format!("{}.aos", adapter_id));

        let adapter = SingleFileAdapter::create(
            adapter_id.to_string(),
            vec![1, 2, 3, 4, 5],
            vec![],
            TrainingConfig::default(),
            LineageInfo {
                adapter_id: adapter_id.to_string(),
                version: "1.0.0".to_string(),
                parent_version: None,
                parent_hash: None,
                mutations: vec![],
                quality_delta: 0.0,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .unwrap();

        SingleFileAdapterPackager::save(&adapter, &aos_path)
            .await
            .unwrap();

        aos_path
    }

    #[tokio::test]
    async fn test_aos_announcements() {
        let temp_dir = TempDir::new().unwrap();
        let store_dir = temp_dir.path().join("store");
        let store = Arc::new(AosStore::new(store_dir).await.unwrap());

        // Store AOS
        let aos_path = create_test_aos(temp_dir.path(), "test").await;
        store.store(&aos_path).await.unwrap();

        // Generate announcements
        let coordinator = AosSyncCoordinator::new(store, SyncStrategy::All);
        let announcements = coordinator.generate_announcements();

        assert_eq!(announcements.len(), 1);
        assert_eq!(announcements[0].adapter_id, "test");
    }

    #[tokio::test]
    async fn test_export_import() {
        let temp_dir = TempDir::new().unwrap();
        let store_dir = temp_dir.path().join("store");
        let export_dir = temp_dir.path().join("export");
        let store = Arc::new(AosStore::new(store_dir).await.unwrap());

        // Store AOS
        let aos_path = create_test_aos(temp_dir.path(), "test").await;
        let hash = store.store(&aos_path).await.unwrap();

        // Export
        let coordinator = AosSyncCoordinator::new(store.clone(), SyncStrategy::All);
        let exported = coordinator
            .export_to_directory(&export_dir, None)
            .await
            .unwrap();
        assert_eq!(exported, 1);

        // Import to new store
        let store2_dir = temp_dir.path().join("store2");
        let store2 = Arc::new(AosStore::new(store2_dir).await.unwrap());
        let coordinator2 = AosSyncCoordinator::new(store2.clone(), SyncStrategy::All);

        let imported = coordinator2.import_from_directory(&export_dir).await.unwrap();
        assert_eq!(imported, 1);

        // Verify imported
        assert!(store2.exists(&hash));
    }
}

