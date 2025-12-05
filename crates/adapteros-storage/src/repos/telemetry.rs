//! Telemetry repository backed by KV storage.
//!
//! Provides deterministic ordering via timestamp-normalized sequence keys and
//! maintains tenant-scoped indexes for event type filtering.

use crate::error::StorageError;
use crate::kv::backend::KvBackend;
use crate::kv::indexing::{telemetry_indexes, IndexManager};
use crate::models::{TelemetryBundleKv, TelemetryEventKv, DEFAULT_BUNDLE_CHUNK_SIZE};
use crate::types::{KeyBuilder, VersionedRecord};
use std::sync::Arc;

/// Repository for telemetry events and bundles
pub struct TelemetryRepository {
    backend: Arc<dyn KvBackend>,
    index_manager: Arc<IndexManager>,
    chunk_size: usize,
}

impl TelemetryRepository {
    /// Create a new telemetry repository with default chunk size
    pub fn new(backend: Arc<dyn KvBackend>, index_manager: Arc<IndexManager>) -> Self {
        Self {
            backend,
            index_manager,
            chunk_size: DEFAULT_BUNDLE_CHUNK_SIZE,
        }
    }

    /// Override chunk size (useful for tests)
    pub fn with_chunk_size(
        backend: Arc<dyn KvBackend>,
        index_manager: Arc<IndexManager>,
        chunk_size: usize,
    ) -> Self {
        Self {
            backend,
            index_manager,
            chunk_size,
        }
    }

    /// Store a telemetry event with deterministic seq key
    pub async fn store_event(&self, event: TelemetryEventKv) -> Result<(), StorageError> {
        let key = KeyBuilder::telemetry_event(&event.tenant_id, &event.seq).build();
        let record = VersionedRecord::new(event.clone());
        let bytes = record.serialize()?;

        self.backend.set(&key, bytes).await?;

        // Index by tenant + event_type for filtered scans
        let index_value = format!("{}:{}", event.tenant_id, event.event_type);
        self.index_manager
            .add_to_index(telemetry_indexes::EVENTS_BY_TENANT_TYPE, &index_value, &key)
            .await?;

        Ok(())
    }

    /// List recent telemetry events for a tenant ordered by seq DESC
    pub async fn list_events_by_tenant(
        &self,
        tenant_id: &str,
        limit: usize,
    ) -> Result<Vec<TelemetryEventKv>, StorageError> {
        let prefix = format!("telemetry:{}:events", tenant_id);
        let mut keys = self.backend.scan_prefix(&prefix).await?;
        keys.sort(); // lexicographic by seq (timestamp-normalized)
        keys.reverse();

        let selected = if keys.len() > limit {
            keys.into_iter().take(limit).collect()
        } else {
            keys
        };

        let mut events = Vec::new();
        for key in selected {
            if let Some(bytes) = self.backend.get(&key).await? {
                if let Ok(record) =
                    VersionedRecord::<TelemetryEventKv>::deserialize_and_migrate(&bytes)
                {
                    events.push(record.data);
                }
            }
        }

        Ok(events)
    }

    /// Get a single telemetry event by seq key
    pub async fn get_event(
        &self,
        tenant_id: &str,
        seq: &str,
    ) -> Result<Option<TelemetryEventKv>, StorageError> {
        let key = KeyBuilder::telemetry_event(tenant_id, seq).build();
        let Some(bytes) = self.backend.get(&key).await? else {
            return Ok(None);
        };

        let record = VersionedRecord::<TelemetryEventKv>::deserialize_and_migrate(&bytes)?;
        Ok(Some(record.data))
    }

    /// List recent telemetry events filtered by event type for a tenant
    pub async fn list_events_by_type(
        &self,
        tenant_id: &str,
        event_type: &str,
        limit: usize,
    ) -> Result<Vec<TelemetryEventKv>, StorageError> {
        let index_value = format!("{}:{}", tenant_id, event_type);
        let mut keys = self
            .index_manager
            .query_index(telemetry_indexes::EVENTS_BY_TENANT_TYPE, &index_value)
            .await?;

        keys.sort();
        keys.reverse();
        let selected = if keys.len() > limit {
            keys.into_iter().take(limit).collect()
        } else {
            keys
        };

        let mut events = Vec::new();
        for key in selected {
            if let Some(bytes) = self.backend.get(&key).await? {
                if let Ok(record) =
                    VersionedRecord::<TelemetryEventKv>::deserialize_and_migrate(&bytes)
                {
                    events.push(record.data);
                }
            }
        }

        Ok(events)
    }

    /// Store bundle metadata in KV
    pub async fn store_bundle_metadata(
        &self,
        bundle: TelemetryBundleKv,
    ) -> Result<(), StorageError> {
        let key = KeyBuilder::telemetry_bundle(&bundle.tenant_id, &bundle.id).build();
        let record = VersionedRecord::new(bundle);
        let bytes = record.serialize()?;
        self.backend.set(&key, bytes).await
    }

    /// Store bundle payload with chunking
    pub async fn store_bundle_chunks(
        &self,
        tenant_id: &str,
        bundle_id: &str,
        payload: &[u8],
    ) -> Result<u32, StorageError> {
        let chunks = Self::chunk_payload(payload, self.chunk_size);

        for (idx, chunk) in chunks.iter().enumerate() {
            let chunk_key = format!(
                "{}:chunk:{:04}",
                KeyBuilder::telemetry_bundle(tenant_id, bundle_id).build(),
                idx
            );
            self.backend.set(&chunk_key, chunk.clone()).await?;
        }

        Ok(chunks.len() as u32)
    }

    /// Load bundle metadata
    pub async fn get_bundle_metadata(
        &self,
        tenant_id: &str,
        bundle_id: &str,
    ) -> Result<Option<TelemetryBundleKv>, StorageError> {
        let key = KeyBuilder::telemetry_bundle(tenant_id, bundle_id).build();
        let Some(bytes) = self.backend.get(&key).await? else {
            return Ok(None);
        };

        let record = VersionedRecord::<TelemetryBundleKv>::deserialize_and_migrate(&bytes)?;
        Ok(Some(record.data))
    }

    /// Load bundle payload by concatenating stored chunks
    pub async fn load_bundle_chunks(
        &self,
        tenant_id: &str,
        bundle_id: &str,
        chunk_count: u32,
    ) -> Result<Vec<u8>, StorageError> {
        let mut payload = Vec::new();
        for idx in 0..chunk_count {
            let chunk_key = format!(
                "{}:chunk:{:04}",
                KeyBuilder::telemetry_bundle(tenant_id, bundle_id).build(),
                idx
            );
            if let Some(bytes) = self.backend.get(&chunk_key).await? {
                payload.extend_from_slice(&bytes);
            }
        }
        Ok(payload)
    }

    /// List bundle metadata for a tenant ordered by created_at DESC
    pub async fn list_bundles_by_tenant(
        &self,
        tenant_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<TelemetryBundleKv>, StorageError> {
        let prefix = format!("telemetry:{}:", tenant_id);
        let mut keys = self.backend.scan_prefix(&prefix).await?;
        // Filter to bundle metadata only (exclude events and chunked payloads)
        keys.retain(|k| !k.contains(":events:") && !k.contains(":chunk:"));
        keys.sort();

        let mut bundles = Vec::new();
        for key in keys {
            if let Some(bytes) = self.backend.get(&key).await? {
                if let Ok(record) =
                    VersionedRecord::<TelemetryBundleKv>::deserialize_and_migrate(&bytes)
                {
                    bundles.push(record.data);
                }
            }
        }

        bundles.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let sliced = bundles.into_iter().skip(offset).take(limit).collect();
        Ok(sliced)
    }

    fn chunk_payload(payload: &[u8], chunk_size: usize) -> Vec<Vec<u8>> {
        if payload.is_empty() {
            return vec![Vec::new()];
        }

        payload
            .chunks(chunk_size)
            .map(|c| c.to_vec())
            .collect::<Vec<_>>()
    }
}
