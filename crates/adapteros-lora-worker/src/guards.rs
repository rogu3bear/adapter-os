//! RAII guards for adapter refcount management during inference
//!
//! Implements AdapterUseGuard to ensure adapter refcounts are properly
//! decremented even on error paths, preventing resource leaks and enabling
//! safe RCU-based adapter retirement.

use adapteros_core::GuardLogLevel;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::adapter_hotswap::AdapterTable;

/// RAII guard for adapter refcount management during inference.
///
/// Automatically decrements adapter refcounts when dropped, ensuring
/// correct lifecycle tracking even on panic or early return paths.
///
/// # Usage
///
/// ```ignore
/// // Option 1: Pre-fetch refs and increment manually
/// let adapter_refs = table.get_refcount_refs(&adapter_names).await;
/// for (_, rc) in &adapter_refs {
///     rc.fetch_add(1, Ordering::Relaxed);
/// }
/// let guard = AdapterUseGuard::new(
///     adapter_refs,
///     table.retirement_sender(),
///     GuardLogLevel::Warn,
/// );
///
/// // Option 2: Use from_table helper (fetches + increments)
/// let guard = AdapterUseGuard::from_table(
///     &table,
///     &adapter_names,
///     GuardLogLevel::Warn,
/// ).await?;
///
/// // ... perform inference ...
///
/// guard.mark_completed(); // Mark as normal completion
/// drop(guard); // Cleanup happens here
/// ```
///
/// # Determinism
///
/// Refcount operations use Relaxed ordering as they don't require
/// cross-thread synchronization beyond the atomic update itself.
/// The retirement signal is sent via try_send to avoid blocking.
pub struct AdapterUseGuard {
    /// List of (adapter_id, refcount_ptr) for each adapter in use
    adapter_refs: Vec<(String, Arc<AtomicUsize>)>,
    /// Channel to signal retirement task when refcount hits 0
    retirement_sender: Option<mpsc::Sender<()>>,
    /// Whether cleanup has been manually performed (disarm)
    disarmed: bool,
    /// Log level for cleanup warnings
    log_level: GuardLogLevel,
    /// Whether drop is happening due to normal completion or error
    completed_normally: bool,
}

impl AdapterUseGuard {
    /// Create a new guard with pre-fetched adapter references.
    ///
    /// This assumes the caller has already incremented refcounts.
    /// For automatic increment, use `from_table()` instead.
    ///
    /// # Arguments
    ///
    /// * `adapter_refs` - List of (adapter_id, refcount_ptr) tuples
    /// * `retirement_sender` - Channel to signal retirement task
    /// * `log_level` - Log level for cleanup warnings
    pub fn new(
        adapter_refs: Vec<(String, Arc<AtomicUsize>)>,
        retirement_sender: Option<mpsc::Sender<()>>,
        log_level: GuardLogLevel,
    ) -> Self {
        Self {
            adapter_refs,
            retirement_sender,
            disarmed: false,
            log_level,
            completed_normally: false,
        }
    }

    /// Create a guard from an AdapterTable, fetching refs and incrementing.
    ///
    /// This is the recommended factory method as it handles increment atomically.
    ///
    /// # Arguments
    ///
    /// * `table` - The adapter table to fetch refcounts from
    /// * `names` - Adapter IDs to guard
    /// * `log_level` - Log level for cleanup warnings
    ///
    /// # Returns
    ///
    /// A guard that will automatically decrement refcounts on drop
    pub async fn from_table(
        table: &AdapterTable,
        names: &[String],
        log_level: GuardLogLevel,
    ) -> Self {
        // Fetch refcount references
        let adapter_refs = table.get_refcount_refs(names).await;

        // Increment all refcounts
        for (_, rc) in &adapter_refs {
            rc.fetch_add(1, Ordering::Relaxed);
        }

        Self::new(adapter_refs, table.retirement_sender(), log_level)
    }

    /// Mark the guarded operation as completed normally.
    ///
    /// Call this before dropping the guard on a successful path to avoid
    /// logging warnings about incomplete operations.
    pub fn mark_completed(&mut self) {
        self.completed_normally = true;
    }

    /// Disarm the guard, preventing automatic cleanup on drop.
    ///
    /// Use this if refcount management has been transferred elsewhere
    /// or if cleanup should be deferred.
    pub fn disarm(&mut self) {
        self.disarmed = true;
    }

    /// Get the number of adapters being guarded
    pub fn adapter_count(&self) -> usize {
        self.adapter_refs.len()
    }

    /// Check if the guard has been disarmed
    pub fn is_disarmed(&self) -> bool {
        self.disarmed
    }
}

impl Drop for AdapterUseGuard {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }

        // Log warning if dropping on error path (based on log level)
        if !self.completed_normally {
            match self.log_level {
                GuardLogLevel::Warn => {
                    warn!(
                        adapter_count = self.adapter_refs.len(),
                        "AdapterUseGuard dropped without mark_completed - possible error path"
                    );
                }
                GuardLogLevel::Debug => {
                    debug!(
                        adapter_count = self.adapter_refs.len(),
                        "AdapterUseGuard dropped without mark_completed"
                    );
                }
                GuardLogLevel::Off => {}
            }
        }

        // Decrement all refcounts
        for (adapter_id, rc) in &self.adapter_refs {
            let old = rc.fetch_sub(1, Ordering::Relaxed);

            // Send retirement signal if refcount reached 0
            if old == 1 {
                if let Some(ref tx) = self.retirement_sender {
                    // Use try_send to avoid blocking Drop
                    if let Err(e) = tx.try_send(()) {
                        match self.log_level {
                            GuardLogLevel::Warn => {
                                warn!(
                                    adapter_id = %adapter_id,
                                    error = ?e,
                                    "Failed to send retirement signal"
                                );
                            }
                            GuardLogLevel::Debug => {
                                debug!(
                                    adapter_id = %adapter_id,
                                    error = ?e,
                                    "Failed to send retirement signal"
                                );
                            }
                            GuardLogLevel::Off => {}
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter_hotswap::AdapterTable;
    use adapteros_core::B3Hash;

    #[tokio::test]
    async fn test_guard_basic_lifecycle() {
        let table = AdapterTable::new();
        let hash = B3Hash::hash(b"test_adapter");

        // Preload and swap adapter
        table
            .preload("test_adapter".to_string(), hash, 10)
            .await
            .unwrap();
        table
            .swap(&["test_adapter".to_string()], &[])
            .await
            .unwrap();

        // Create guard
        let guard =
            AdapterUseGuard::from_table(&table, &["test_adapter".to_string()], GuardLogLevel::Off)
                .await;

        // Verify refcount was incremented
        {
            let refcounts = table.refcounts().lock().await;
            let rc = refcounts.get("test_adapter").unwrap();
            assert_eq!(rc.load(Ordering::Relaxed), 1);
        }

        // Drop guard
        drop(guard);

        // Verify refcount was decremented
        {
            let refcounts = table.refcounts().lock().await;
            let rc = refcounts.get("test_adapter").unwrap();
            assert_eq!(rc.load(Ordering::Relaxed), 0);
        }
    }

    #[tokio::test]
    async fn test_guard_mark_completed() {
        let table = AdapterTable::new();
        let hash = B3Hash::hash(b"test_adapter");

        table
            .preload("test_adapter".to_string(), hash, 10)
            .await
            .unwrap();
        table
            .swap(&["test_adapter".to_string()], &[])
            .await
            .unwrap();

        let mut guard =
            AdapterUseGuard::from_table(&table, &["test_adapter".to_string()], GuardLogLevel::Warn)
                .await;

        // Mark as completed before drop
        guard.mark_completed();
        drop(guard);

        // Should not log warning (test would need to capture logs to verify)
    }

    #[tokio::test]
    async fn test_guard_disarm() {
        let table = AdapterTable::new();
        let hash = B3Hash::hash(b"test_adapter");

        table
            .preload("test_adapter".to_string(), hash, 10)
            .await
            .unwrap();
        table
            .swap(&["test_adapter".to_string()], &[])
            .await
            .unwrap();

        let mut guard =
            AdapterUseGuard::from_table(&table, &["test_adapter".to_string()], GuardLogLevel::Off)
                .await;

        // Disarm guard
        guard.disarm();
        drop(guard);

        // Verify refcount was NOT decremented
        {
            let refcounts = table.refcounts().lock().await;
            let rc = refcounts.get("test_adapter").unwrap();
            assert_eq!(rc.load(Ordering::Relaxed), 1); // Still 1 because guard was disarmed
        }
    }

    #[tokio::test]
    async fn test_guard_multiple_adapters() {
        let table = AdapterTable::new();
        let hash1 = B3Hash::hash(b"adapter1");
        let hash2 = B3Hash::hash(b"adapter2");

        // Preload two adapters
        table
            .preload("adapter1".to_string(), hash1, 10)
            .await
            .unwrap();
        table
            .preload("adapter2".to_string(), hash2, 15)
            .await
            .unwrap();
        table
            .swap(&["adapter1".to_string(), "adapter2".to_string()], &[])
            .await
            .unwrap();

        // Create guard for both
        let guard = AdapterUseGuard::from_table(
            &table,
            &["adapter1".to_string(), "adapter2".to_string()],
            GuardLogLevel::Off,
        )
        .await;

        // Verify both refcounts incremented
        {
            let refcounts = table.refcounts().lock().await;
            assert_eq!(
                refcounts.get("adapter1").unwrap().load(Ordering::Relaxed),
                1
            );
            assert_eq!(
                refcounts.get("adapter2").unwrap().load(Ordering::Relaxed),
                1
            );
        }

        drop(guard);

        // Verify both decremented
        {
            let refcounts = table.refcounts().lock().await;
            assert_eq!(
                refcounts.get("adapter1").unwrap().load(Ordering::Relaxed),
                0
            );
            assert_eq!(
                refcounts.get("adapter2").unwrap().load(Ordering::Relaxed),
                0
            );
        }
    }

    #[tokio::test]
    async fn test_guard_retirement_signal() {
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::channel(10);
        let mut table = AdapterTable::new();
        table.set_retirement_sender(tx);

        let hash = B3Hash::hash(b"test_adapter");
        table
            .preload("test_adapter".to_string(), hash, 10)
            .await
            .unwrap();
        table
            .swap(&["test_adapter".to_string()], &[])
            .await
            .unwrap();

        let guard =
            AdapterUseGuard::from_table(&table, &["test_adapter".to_string()], GuardLogLevel::Off)
                .await;

        drop(guard);

        // Should receive retirement signal
        tokio::select! {
            Some(()) = rx.recv() => {
                // Signal received as expected
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                panic!("Did not receive retirement signal");
            }
        }
    }

    #[tokio::test]
    async fn test_guard_no_retirement_signal_if_not_zero() {
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::channel(10);
        let mut table = AdapterTable::new();
        table.set_retirement_sender(tx);

        let hash = B3Hash::hash(b"test_adapter");
        table
            .preload("test_adapter".to_string(), hash, 10)
            .await
            .unwrap();
        table
            .swap(&["test_adapter".to_string()], &[])
            .await
            .unwrap();

        // Create two guards (refcount will be 2)
        let guard1 =
            AdapterUseGuard::from_table(&table, &["test_adapter".to_string()], GuardLogLevel::Off)
                .await;

        let guard2 =
            AdapterUseGuard::from_table(&table, &["test_adapter".to_string()], GuardLogLevel::Off)
                .await;

        // Drop first guard (refcount goes to 1, no signal)
        drop(guard1);

        // Should NOT receive signal yet
        tokio::select! {
            Some(()) = rx.recv() => {
                panic!("Should not receive signal when refcount > 0");
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => {
                // Expected timeout
            }
        }

        // Drop second guard (refcount goes to 0, signal sent)
        drop(guard2);

        // Now should receive signal
        tokio::select! {
            Some(()) = rx.recv() => {
                // Signal received as expected
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                panic!("Did not receive retirement signal after refcount reached 0");
            }
        }
    }

    #[test]
    fn test_guard_is_disarmed() {
        let guard = AdapterUseGuard::new(vec![], None, GuardLogLevel::Off);
        assert!(!guard.is_disarmed());

        let mut guard = AdapterUseGuard::new(vec![], None, GuardLogLevel::Off);
        guard.disarm();
        assert!(guard.is_disarmed());
    }

    #[test]
    fn test_guard_adapter_count() {
        let refs = vec![
            ("adapter1".to_string(), Arc::new(AtomicUsize::new(0))),
            ("adapter2".to_string(), Arc::new(AtomicUsize::new(0))),
        ];
        let guard = AdapterUseGuard::new(refs, None, GuardLogLevel::Off);
        assert_eq!(guard.adapter_count(), 2);
    }
}
