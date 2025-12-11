use crate::adapter_hotswap::{AdapterTable, Stack};
use adapteros_core::{adapter_store::AdapterPins, AosError, B3Hash};
use std::sync::Arc;

/// Pins the current adapter snapshot for the duration of a request.
pub struct RequestPinner {
    table: Arc<AdapterTable>,
}

/// RAII guard that holds adapter pins until dropped.
pub struct PinnedRequest {
    pins: AdapterPins,
    stack: Arc<Stack>,
    stack_hash: B3Hash,
}

impl RequestPinner {
    pub fn new(table: Arc<AdapterTable>) -> Self {
        Self { table }
    }

    /// Capture a snapshot and bump refcounts so mid-request swaps cannot evict adapters.
    pub fn pin(&self) -> Result<PinnedRequest, AosError> {
        for _ in 0..2 {
            let stack = self.table.get_current_stack_handle();
            if stack.active.is_empty() {
                return Err(AosError::Worker(
                    "No active adapters available to pin".to_string(),
                ));
            }

            let pins = self.table.store().pin_current();
            if pins.generation() == stack.generation {
                let stack_hash = self.table.compute_stack_hash();
                return Ok(PinnedRequest {
                    pins,
                    stack,
                    stack_hash,
                });
            }
        }

        Err(AosError::Worker(
            "Adapter stack changed while pinning request".to_string(),
        ))
    }
}

impl PinnedRequest {
    pub fn generation(&self) -> u64 {
        self.pins.generation()
    }

    pub fn stack(&self) -> &Arc<Stack> {
        &self.stack
    }

    pub fn stack_hash(&self) -> B3Hash {
        self.stack_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::B3Hash;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn swap_waits_until_pins_released() {
        let table = Arc::new(AdapterTable::new());
        let h1 = B3Hash::hash(b"a1");
        table
            .preload("a".to_string(), h1, 1)
            .await
            .expect("preload");
        table
            .swap(&["a".to_string()], &[])
            .await
            .expect("initial swap");

        let pinner = RequestPinner::new(table.clone());
        let pinned = pinner.pin().expect("pin should succeed");

        // Swap should time out while refcount is held.
        let blocked = table
            .wait_for_zero_refs(&["a".to_string()], Duration::from_millis(50))
            .await;
        assert!(blocked.is_err(), "swap must block while request is pinned");

        drop(pinned);
        let drained = table
            .wait_for_zero_refs(&["a".to_string()], Duration::from_millis(200))
            .await;
        assert!(drained.is_ok(), "swap should proceed after release");
    }

    #[tokio::test]
    async fn swap_only_affects_new_requests() {
        let table = Arc::new(AdapterTable::new());
        let h1 = B3Hash::hash(b"a1");
        table
            .preload("a".to_string(), h1, 1)
            .await
            .expect("preload");
        table
            .swap(&["a".to_string()], &[])
            .await
            .expect("initial swap");

        let pinner = RequestPinner::new(table.clone());
        let pinned_old = pinner.pin().expect("pin should succeed");
        let old_hashes: Vec<B3Hash> = pinned_old.stack().active.values().map(|s| s.hash).collect();
        assert!(
            old_hashes.contains(&h1),
            "pinned request should see the original adapter"
        );

        // Stage a new adapter and attempt swap while old pins are held.
        let h2 = B3Hash::hash(b"b2");
        table
            .preload("b".to_string(), h2, 1)
            .await
            .expect("preload b");

        let table_for_swap = table.clone();
        let swap_task = tokio::spawn(async move {
            table_for_swap
                .swap(&["b".to_string()], &["a".to_string()])
                .await
        });
        sleep(Duration::from_millis(50)).await;
        assert!(
            !swap_task.is_finished(),
            "swap should wait on in-flight request pins"
        );

        // Release old request pins before swapping.
        drop(pinned_old);
        let swap_result = swap_task.await.expect("swap task should join");
        assert!(swap_result.is_ok(), "swap should succeed after release");

        let pinned_new = pinner.pin().expect("pin new should succeed");
        let hashes: Vec<B3Hash> = pinned_new.stack().active.values().map(|s| s.hash).collect();
        assert!(
            hashes.contains(&h2),
            "new request must see freshly swapped adapter"
        );
    }
}
