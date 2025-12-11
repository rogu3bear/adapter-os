use crate::hash::B3Hash;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Ref-counted adapter entry keyed by adapter id.
#[derive(Clone, Debug)]
pub struct AdapterRecord {
    pub hash: B3Hash,
    pub refcount: Arc<AtomicUsize>,
}

/// Snapshot of the current adapter index at a generation boundary.
#[derive(Clone, Debug)]
pub struct AdapterSnapshot {
    pub generation: u64,
    pub entries: Arc<HashMap<String, AdapterRecord>>,
}

/// Guard that holds references for a request; decrements on drop.
#[derive(Debug)]
pub struct AdapterPins {
    snapshot: AdapterSnapshot,
    pinned: Vec<(String, Arc<AtomicUsize>)>,
}

impl AdapterPins {
    /// Generation that was pinned for the request.
    pub fn generation(&self) -> u64 {
        self.snapshot.generation
    }

    /// Adapter hashes pinned for the request.
    pub fn hashes(&self) -> &Arc<HashMap<String, AdapterRecord>> {
        &self.snapshot.entries
    }
}

impl Drop for AdapterPins {
    fn drop(&mut self) {
        for (_id, rc) in &self.pinned {
            let prev = rc.fetch_sub(1, Ordering::AcqRel);
            if prev == 0 {
                rc.store(0, Ordering::Release);
            }
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
        entries: HashMap<String, AdapterRecord>,
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
        for (id, record) in snapshot.entries.iter() {
            record.refcount.fetch_add(1, Ordering::AcqRel);
            pinned.push((id.clone(), record.refcount.clone()));
        }
        AdapterPins { snapshot, pinned }
    }

    /// Drop retired snapshots whose refcounts have reached zero.
    ///
    /// Returns the generations that were freed.
    pub fn drain_retired(&self) -> Vec<u64> {
        let mut retired = self.retired.lock();
        let mut drained = Vec::new();
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
        drained
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
        entries.insert(
            "a".to_string(),
            AdapterRecord {
                hash: B3Hash::hash(b"a"),
                refcount: rc.clone(),
            },
        );

        // Install generation 1; generation 0 moves to retired.
        let snap = store.install(1, entries);
        assert_eq!(snap.generation, 1);
        assert_eq!(store.snapshot().generation, 1);

        // Pin current snapshot; refcount increments.
        let pins = store.pin_current();
        assert_eq!(pins.generation(), 1);
        assert_eq!(rc.load(Ordering::Relaxed), 1);

        // Retired gen0 should stay until pins are dropped.
        assert!(store.drain_retired().is_empty());
        drop(pins);

        // After release, retired gen0 drains.
        assert_eq!(store.drain_retired(), vec![0]);
        assert_eq!(rc.load(Ordering::Relaxed), 0);
    }
}
