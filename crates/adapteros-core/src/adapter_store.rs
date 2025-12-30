use crate::hash::B3Hash;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::warn;

/// Maximum number of retired snapshots to retain before force eviction.
/// When exceeded, oldest snapshots are force-evicted regardless of refcount
/// to prevent unbounded memory growth from slow/stuck clients.
const MAX_RETIRED_SNAPSHOTS: usize = 50;

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

            // Debug assertion: prev should always be >= 1 if refcounting is correct.
            // A prev of 0 would indicate a double-free bug.
            debug_assert!(
                prev >= 1,
                "AdapterPins refcount underflow detected (prev={prev})"
            );
        }
    }
}

/// RCU-style adapter store with ref-counted entries.
#[derive(Default)]
pub struct AdapterStore {
    current: RwLock<AdapterSnapshot>,
    retired: Mutex<Vec<AdapterSnapshot>>,
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
        let new_snapshot = AdapterSnapshot {
            generation,
            entries: Arc::new(entries),
        };
        let old = std::mem::replace(&mut *guard, new_snapshot.clone());
        if old.generation != new_snapshot.generation {
            self.retired.lock().push(old);
        }
        new_snapshot
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
                // Note: Any holders of pins to these force-evicted snapshots will
                // continue to work (they hold Arc refs), but the snapshot memory
                // will be reclaimed when they drop their pins.
            }
        }

        drained
    }

    /// Get the current count of retired snapshots (for monitoring).
    pub fn retired_count(&self) -> usize {
        self.retired.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
