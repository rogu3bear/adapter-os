use adapteros_infra_common::B3Hash;
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::warn;

/// Maximum number of retired snapshots to retain before force eviction.
/// When exceeded, oldest snapshots are force-evicted regardless of refcount
/// to prevent unbounded memory growth from slow/stuck clients.
const MAX_RETIRED_SNAPSHOTS: usize = 50;
const MAX_GENERATION_AUDIT_EVENTS: usize = 512;

/// Immutable generation audit event emitted on install/drain transitions.
#[derive(Clone, Debug)]
pub struct AdapterGenerationAuditEvent {
    pub timestamp_ms: u64,
    pub event: &'static str,
    pub generation: u64,
    pub previous_generation: u64,
    pub retired_count: usize,
    pub details: String,
}

/// Health summary for RCU generation loading.
#[derive(Clone, Debug)]
pub struct AdapterStoreHealth {
    pub healthy: bool,
    pub current_generation: u64,
    pub retired_count: usize,
    pub audit_events: usize,
    pub next_action: String,
}

/// Cache key for adapter residency aligned with context manifest identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AdapterCacheKey {
    pub adapter_id: String,
    pub adapter_hash: B3Hash,
    pub base_manifest_hash: Option<B3Hash>,
    pub backend_type: String,
    pub kernel_version_id: String,
    pub tenant_id: Option<String>,
    pub adapter_dir_hash: Option<B3Hash>,
    /// Optional galaxy identifier when adapters are loaded from a shared mmap bundle.
    pub galaxy_id: Option<String>,
}

impl AdapterCacheKey {
    pub fn new(
        adapter_id: impl Into<String>,
        adapter_hash: B3Hash,
        base_manifest_hash: Option<B3Hash>,
        backend_type: impl Into<String>,
        kernel_version_id: impl Into<String>,
        tenant_id: Option<String>,
        adapter_dir_hash: Option<B3Hash>,
    ) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            adapter_hash,
            base_manifest_hash,
            backend_type: backend_type.into(),
            kernel_version_id: kernel_version_id.into(),
            tenant_id,
            adapter_dir_hash,
            galaxy_id: None,
        }
    }

    /// Attach a galaxy identifier to the cache key so adapters that share a
    /// mapped bundle keep that bundle resident while any ref is held.
    pub fn with_galaxy(mut self, galaxy_id: Option<String>) -> Self {
        self.galaxy_id = galaxy_id;
        self
    }
}

/// Ref-counted adapter entry keyed by adapter id.
#[derive(Clone, Debug)]
pub struct AdapterRecord {
    pub hash: B3Hash,
    pub refcount: Arc<AtomicUsize>,
}

/// Snapshot of the current adapter index at a generation boundary.
#[derive(Clone, Debug, Default)]
pub struct AdapterSnapshot {
    pub generation: u64,
    pub entries: Arc<HashMap<AdapterCacheKey, AdapterRecord>>,
}

/// Guard that holds references for a request; decrements on drop.
#[derive(Debug)]
pub struct AdapterPins {
    snapshot: AdapterSnapshot,
    pinned: Vec<(AdapterCacheKey, Arc<AtomicUsize>)>,
}

impl AdapterPins {
    /// Generation that was pinned for the request.
    pub fn generation(&self) -> u64 {
        self.snapshot.generation
    }

    /// Adapter cache entries pinned for the request (identity + hash).
    pub fn hashes(&self) -> &Arc<HashMap<AdapterCacheKey, AdapterRecord>> {
        &self.snapshot.entries
    }
}

impl Drop for AdapterPins {
    fn drop(&mut self) {
        for (_id, rc) in &self.pinned {
            // Use Release ordering: this drop operation publishes the refcount
            // decrement so that drain_retired (using Acquire) sees the update.
            // AcqRel is not needed here since we don't read dependent data.
            let prev = rc.fetch_sub(1, Ordering::Release);

            // prev should always be >= 1 if refcounting is correct.
            // A prev of 0 would indicate a double-free bug.
            assert!(
                prev >= 1,
                "AdapterPins refcount underflow detected (prev={prev})"
            );
        }
    }
}

/// RCU-style adapter store with ref-counted entries.
pub struct AdapterStore {
    current: RwLock<AdapterSnapshot>,
    retired: Mutex<Vec<AdapterSnapshot>>,
    generation_audit: Mutex<VecDeque<AdapterGenerationAuditEvent>>,
}

impl Default for AdapterStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AdapterStore {
    /// Create a new store with an empty generation 0 snapshot.
    pub fn new() -> Self {
        Self {
            current: RwLock::new(AdapterSnapshot {
                generation: 0,
                entries: Arc::new(HashMap::new()),
            }),
            retired: Mutex::new(Vec::new()),
            generation_audit: Mutex::new(VecDeque::new()),
        }
    }

    /// Get a cheap snapshot of the current adapter index.
    pub fn snapshot(&self) -> AdapterSnapshot {
        self.current.read().clone()
    }

    /// Install a new adapter index at the provided generation.
    ///
    /// The previous snapshot is moved to `retired` and will stay there
    /// until all refcounts drop to zero and `drain_retired` is called.
    pub fn install(
        &self,
        generation: u64,
        entries: HashMap<AdapterCacheKey, AdapterRecord>,
    ) -> AdapterSnapshot {
        let mut guard = self.current.write();
        if generation <= guard.generation {
            self.push_generation_audit(
                "generation_rejected",
                generation,
                guard.generation,
                self.retired_count(),
                format!(
                    "Rejected non-monotonic install request (requested={}, current={})",
                    generation, guard.generation
                ),
            );
            warn!(
                requested_generation = generation,
                current_generation = guard.generation,
                "Rejected non-monotonic adapter generation install"
            );
            return guard.clone();
        }

        let new_snapshot = AdapterSnapshot {
            generation,
            entries: Arc::new(entries),
        };
        let previous_generation = guard.generation;
        let old = std::mem::replace(&mut *guard, new_snapshot.clone());
        if old.generation != new_snapshot.generation {
            self.retired.lock().push(old);
        }
        self.push_generation_audit(
            "generation_installed",
            generation,
            previous_generation,
            self.retired_count(),
            format!(
                "Installed generation {} with {} entries",
                generation,
                new_snapshot.entries.len()
            ),
        );
        new_snapshot
    }

    /// Force-install an adapter index at an explicit generation.
    ///
    /// This bypasses monotonic generation checks and is intended only for
    /// trusted recovery flows (for example, swap rollback) where the caller
    /// must restore an earlier generation snapshot atomically.
    pub fn install_force(
        &self,
        generation: u64,
        entries: HashMap<AdapterCacheKey, AdapterRecord>,
    ) -> AdapterSnapshot {
        let mut guard = self.current.write();
        let previous_generation = guard.generation;
        let new_snapshot = AdapterSnapshot {
            generation,
            entries: Arc::new(entries),
        };
        let old = std::mem::replace(&mut *guard, new_snapshot.clone());
        if old.generation != new_snapshot.generation {
            self.retired.lock().push(old);
        }
        self.push_generation_audit(
            "generation_forced_install",
            generation,
            previous_generation,
            self.retired_count(),
            format!(
                "Force-installed generation {} with {} entries",
                generation,
                new_snapshot.entries.len()
            ),
        );
        warn!(
            requested_generation = generation,
            previous_generation = previous_generation,
            "Force-installed adapter generation (recovery path)"
        );
        new_snapshot
    }

    /// Install with bounded retry for transient generation races.
    ///
    /// If a non-monotonic generation is supplied, retries will keep returning
    /// the current snapshot and finally emit a warning with the latest state.
    pub fn install_with_retry(
        &self,
        generation: u64,
        entries: HashMap<AdapterCacheKey, AdapterRecord>,
        max_attempts: u32,
    ) -> AdapterSnapshot {
        let attempts = max_attempts.max(1);
        for attempt in 1..=attempts {
            let snapshot = self.install(generation, entries.clone());
            if snapshot.generation == generation {
                if attempt > 1 {
                    self.push_generation_audit(
                        "generation_retry_succeeded",
                        generation,
                        snapshot.generation,
                        self.retired_count(),
                        format!("Install succeeded after {} attempts", attempt),
                    );
                }
                return snapshot;
            }

            if attempt < attempts {
                std::thread::sleep(std::time::Duration::from_millis(
                    5u64.saturating_mul(attempt as u64),
                ));
            }
        }

        let current = self.snapshot();
        self.push_generation_audit(
            "generation_retry_exhausted",
            generation,
            current.generation,
            self.retired_count(),
            format!(
                "Install retry exhausted after {} attempts; keeping generation {}",
                attempts, current.generation
            ),
        );
        warn!(
            requested_generation = generation,
            current_generation = current.generation,
            attempts = attempts,
            "Adapter generation install retry exhausted"
        );
        current
    }

    /// Pin the current snapshot for a request and increment refcounts.
    pub fn pin_current(&self) -> AdapterPins {
        let snapshot = self.snapshot();
        let mut pinned = Vec::with_capacity(snapshot.entries.len());
        for (key, record) in snapshot.entries.iter() {
            record.refcount.fetch_add(1, Ordering::AcqRel);
            pinned.push((key.clone(), record.refcount.clone()));
        }
        AdapterPins { snapshot, pinned }
    }

    /// Drop retired snapshots whose refcounts have reached zero.
    ///
    /// When the retired list exceeds `MAX_RETIRED_SNAPSHOTS`, force-evicts the
    /// oldest snapshots regardless of refcount to prevent unbounded memory growth.
    ///
    /// Returns the generations that were freed.
    pub fn drain_retired(&self) -> Vec<u64> {
        let mut retired = self.retired.lock();
        let mut drained = Vec::new();

        // First pass: drain snapshots with zero refcount
        retired.retain(|snapshot| {
            let in_use = snapshot
                .entries
                .values()
                .any(|rec| rec.refcount.load(Ordering::Acquire) > 0);
            if !in_use {
                drained.push(snapshot.generation);
            }
            in_use
        });

        // Second pass: force evict oldest snapshots if over limit
        // This prevents unbounded memory growth from slow/stuck clients
        if retired.len() > MAX_RETIRED_SNAPSHOTS {
            let excess = retired.len() - MAX_RETIRED_SNAPSHOTS;
            warn!(
                retired_count = retired.len(),
                max_retired = MAX_RETIRED_SNAPSHOTS,
                force_evicting = excess,
                "Retired snapshot limit exceeded, force-evicting oldest snapshots"
            );

            // Drain the oldest (first N) snapshots regardless of refcount
            let force_evicted: Vec<AdapterSnapshot> = retired.drain(0..excess).collect();
            for snapshot in force_evicted {
                drained.push(snapshot.generation);
                self.push_generation_audit(
                    "retired_force_evicted",
                    snapshot.generation,
                    self.snapshot().generation,
                    retired.len(),
                    "Force-evicted retired snapshot due to retention cap".to_string(),
                );
                // Note: Any holders of pins to these force-evicted snapshots will
                // continue to work (they hold Arc refs), but the snapshot memory
                // will be reclaimed when they drop their pins.
            }
        }

        if !drained.is_empty() {
            self.push_generation_audit(
                "retired_drained",
                self.snapshot().generation,
                0,
                retired.len(),
                format!("Drained retired generations: {:?}", drained),
            );
        }

        drained
    }

    /// Get the current count of retired snapshots (for monitoring).
    pub fn retired_count(&self) -> usize {
        self.retired.lock().len()
    }

    /// Immutable generation audit log snapshot.
    pub fn generation_audit_log(&self) -> Vec<AdapterGenerationAuditEvent> {
        self.generation_audit.lock().iter().cloned().collect()
    }

    /// Health summary for generation loading and retirement pressure.
    pub fn generation_health(&self) -> AdapterStoreHealth {
        let current_generation = self.snapshot().generation;
        let retired_count = self.retired_count();
        let audit_events = self.generation_audit.lock().len();
        let healthy = retired_count <= MAX_RETIRED_SNAPSHOTS;
        let next_action = if healthy {
            "none".to_string()
        } else {
            "drain retired snapshots or inspect stuck request pins".to_string()
        };
        AdapterStoreHealth {
            healthy,
            current_generation,
            retired_count,
            audit_events,
            next_action,
        }
    }

    fn push_generation_audit(
        &self,
        event: &'static str,
        generation: u64,
        previous_generation: u64,
        retired_count: usize,
        details: String,
    ) {
        let mut log = self.generation_audit.lock();
        if log.len() == MAX_GENERATION_AUDIT_EVENTS {
            log.pop_front();
        }
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        log.push_back(AdapterGenerationAuditEvent {
            timestamp_ms,
            event,
            generation,
            previous_generation,
            retired_count,
            details,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_adapter_cache_key_not_reused_on_hash_change() {
        let key_h1 = AdapterCacheKey::new(
            "adapter-x",
            B3Hash::hash(b"version-1-weights"),
            None,
            "mlx",
            "k1",
            None,
            None,
        );
        let key_h2 = AdapterCacheKey::new(
            "adapter-x",
            B3Hash::hash(b"version-2-weights"),
            None,
            "mlx",
            "k1",
            None,
            None,
        );

        assert_ne!(
            key_h1, key_h2,
            "Same adapter path with different hash must produce different cache keys"
        );

        let mut cache: HashMap<AdapterCacheKey, &str> = HashMap::new();
        cache.insert(key_h1, "stale-entry");
        assert_eq!(
            cache.get(&key_h2),
            None,
            "HashMap lookup with updated hash must not return stale entry"
        );
    }

    #[test]
    fn drain_waits_for_refs() {
        let store = AdapterStore::new();
        let rc = Arc::new(AtomicUsize::new(0));
        let mut entries = HashMap::new();
        let cache_key =
            AdapterCacheKey::new("a", B3Hash::hash(b"a"), None, "mock", "k1", None, None);
        entries.insert(
            cache_key,
            AdapterRecord {
                hash: B3Hash::hash(b"a"),
                refcount: rc.clone(),
            },
        );

        // Install generation 1; generation 0 (empty) moves to retired.
        let snap = store.install(1, entries);
        assert_eq!(snap.generation, 1);
        assert_eq!(store.snapshot().generation, 1);

        // Pin current snapshot (gen 1); refcount increments.
        let pins = store.pin_current();
        assert_eq!(pins.generation(), 1);
        assert_eq!(rc.load(Ordering::Relaxed), 1);

        // Install generation 2 (empty) to retire gen 1 (which has our pinned entries).
        store.install(2, HashMap::new());
        assert_eq!(store.snapshot().generation, 2);

        // Gen 1 should stay in retired while pins are held.
        // Gen 0 (empty) drains immediately, but gen 1 has refs.
        let drained = store.drain_retired();
        assert!(!drained.contains(&1), "gen 1 should not drain while pinned");

        drop(pins);

        // After release, gen 1 drains.
        let drained = store.drain_retired();
        assert!(
            drained.contains(&1),
            "gen 1 should drain after pins dropped"
        );
        assert_eq!(rc.load(Ordering::Relaxed), 0);
    }
}
