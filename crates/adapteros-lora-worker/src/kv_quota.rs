//! Per-tenant KV cache quota enforcement
//!
//! Provides reservation-based quota tracking for KV cache allocations.
//! Quotas are enforced at insertion time to prevent cache starvation.

use adapteros_core::{AosError, Result};
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};
use tracing::{debug, warn};

/// Reservation timeout: 5 minutes
const RESERVATION_TIMEOUT: Duration = Duration::from_secs(300);

/// Frequency threshold for HOT promotion (number of accesses)
pub const HOT_PROMOTION_THRESHOLD: u32 = 3;

/// Recency window for HOT classification
pub const HOT_RECENCY_WINDOW: Duration = Duration::from_secs(60);

/// Space reservation for KV cache allocation
#[derive(Debug, Clone)]
pub struct KvReservation {
    /// Unique reservation ID
    pub id: String,
    /// Reserved size in bytes
    pub size_bytes: u64,
    /// Timestamp when reservation was created
    pub created_at: Instant,
    /// Timestamp when reservation expires
    pub expires_at: Instant,
}

impl KvReservation {
    /// Create new reservation
    pub fn new(id: String, size_bytes: u64) -> Self {
        let now = Instant::now();
        Self {
            id,
            size_bytes,
            created_at: now,
            expires_at: now + RESERVATION_TIMEOUT,
        }
    }

    /// Check if reservation has expired
    pub fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }
}

/// KV quota usage statistics
#[derive(Debug, Clone)]
pub struct KvQuotaUsage {
    pub tenant_id: String,
    pub used_bytes: u64,
    pub reserved_bytes: u64,
    pub quota_bytes: Option<u64>,
    pub available_bytes: u64,
    pub usage_pct: f64,
}

/// Per-tenant KV cache quota manager
///
/// Tracks bytes used and reserved, enforcing quota at reservation time.
/// Workers are single-tenant, so one manager per worker.
pub struct TenantKvQuotaManager {
    tenant_id: String,
    /// Quota in bytes (None = unlimited)
    quota_bytes: Option<u64>,
    /// Currently used bytes (finalized allocations)
    used_bytes: AtomicU64,
    /// Currently reserved bytes (pending allocations)
    reserved_bytes: AtomicU64,
    /// Active reservations
    reservations: RwLock<Vec<KvReservation>>,
    /// Serializes quota state updates for consistent snapshots
    quota_lock: Mutex<()>,
    /// Eviction counter for current session
    evictions: AtomicU32,
    /// Whether quota enforcement is active
    quota_enforced: bool,
}

impl TenantKvQuotaManager {
    /// Create new quota manager
    ///
    /// # Arguments
    /// * `tenant_id` - Tenant identifier
    /// * `quota_bytes` - Maximum KV cache bytes (None = unlimited)
    pub fn new(tenant_id: String, quota_bytes: Option<u64>) -> Self {
        let quota_enforced = quota_bytes.is_some();
        Self {
            tenant_id,
            quota_bytes,
            used_bytes: AtomicU64::new(0),
            reserved_bytes: AtomicU64::new(0),
            reservations: RwLock::new(Vec::new()),
            quota_lock: Mutex::new(()),
            evictions: AtomicU32::new(0),
            quota_enforced,
        }
    }

    /// Get tenant ID
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    /// Get configured quota (None = unlimited)
    pub fn quota_bytes(&self) -> Option<u64> {
        self.quota_bytes
    }

    /// Check if quota enforcement is active
    pub fn is_quota_enforced(&self) -> bool {
        self.quota_enforced
    }

    /// Check if allocation is within quota (without reserving)
    pub fn check_quota(&self, bytes: u64) -> Result<()> {
        let _guard = self.quota_lock.lock();
        let Some(quota) = self.quota_bytes else {
            return Ok(()); // Unlimited
        };

        let current = self.used_bytes.load(Ordering::Acquire);
        let reserved = self.reserved_bytes.load(Ordering::Acquire);
        let total_needed = current.saturating_add(reserved).saturating_add(bytes);

        if total_needed > quota {
            return Err(AosError::MemoryPressure(format!(
                "KV quota exceeded for tenant {}: need {} bytes, quota {} bytes (used: {}, reserved: {})",
                self.tenant_id, total_needed, quota, current, reserved
            )));
        }

        Ok(())
    }

    /// Reserve bytes for upcoming allocation
    ///
    /// Returns a reservation handle that must be finalized or rolled back.
    pub fn reserve(&self, bytes: u64) -> Result<KvReservation> {
        let _guard = self.quota_lock.lock();

        // Clean up expired reservations first (caller holds quota lock)
        self.cleanup_expired();

        // Reserve bytes with a consistent snapshot of used + reserved
        if let Some(quota) = self.quota_bytes {
            let current = self.used_bytes.load(Ordering::Acquire);
            let reserved = self.reserved_bytes.load(Ordering::Acquire);
            let total_needed = current.saturating_add(reserved).saturating_add(bytes);

            if total_needed > quota {
                return Err(AosError::MemoryPressure(format!(
                    "KV quota exceeded for tenant {}: need {} bytes, quota {} bytes (used: {}, reserved: {})",
                    self.tenant_id, total_needed, quota, current, reserved
                )));
            }
        }

        self.reserved_bytes.fetch_add(bytes, Ordering::AcqRel);

        // Create reservation ID
        let reservation_id = format!(
            "kvres_{}_{}",
            self.tenant_id,
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );

        let reservation = KvReservation::new(reservation_id.clone(), bytes);

        self.reservations.write().push(reservation.clone());

        debug!(
            tenant_id = %self.tenant_id,
            reservation_id = %reservation_id,
            bytes = bytes,
            "KV quota reservation created"
        );

        Ok(reservation)
    }

    /// Finalize reservation (commit the allocation)
    pub fn finalize(&self, reservation: KvReservation) -> Result<()> {
        let _guard = self.quota_lock.lock();
        // Remove from reservations
        {
            let mut reservations = self.reservations.write();
            reservations.retain(|r| r.id != reservation.id);
        }

        // Move from reserved to used
        self.reserved_bytes
            .fetch_sub(reservation.size_bytes, Ordering::AcqRel);
        self.used_bytes
            .fetch_add(reservation.size_bytes, Ordering::AcqRel);

        debug!(
            tenant_id = %self.tenant_id,
            reservation_id = %reservation.id,
            bytes = reservation.size_bytes,
            "KV quota reservation finalized"
        );

        Ok(())
    }

    /// Rollback reservation (cancel without allocation)
    pub fn rollback(&self, reservation: KvReservation) {
        let _guard = self.quota_lock.lock();
        // Remove from reservations
        {
            let mut reservations = self.reservations.write();
            reservations.retain(|r| r.id != reservation.id);
        }

        // Release reserved bytes
        self.reserved_bytes
            .fetch_sub(reservation.size_bytes, Ordering::AcqRel);

        debug!(
            tenant_id = %self.tenant_id,
            reservation_id = %reservation.id,
            bytes = reservation.size_bytes,
            "KV quota reservation rolled back"
        );
    }

    /// Release used bytes (on sequence free)
    pub fn release(&self, bytes: u64) {
        let _guard = self.quota_lock.lock();
        self.used_bytes.fetch_sub(bytes, Ordering::AcqRel);

        debug!(
            tenant_id = %self.tenant_id,
            bytes = bytes,
            "KV quota bytes released"
        );
    }

    /// Increment eviction counter
    pub fn record_eviction(&self) {
        self.evictions.fetch_add(1, Ordering::Relaxed);
    }

    /// Get eviction count
    pub fn evictions(&self) -> u32 {
        self.evictions.load(Ordering::Relaxed)
    }

    /// Reset eviction counter (at request start)
    pub fn reset_evictions(&self) {
        self.evictions.store(0, Ordering::Relaxed);
    }

    /// Get current usage statistics
    pub fn usage(&self) -> KvQuotaUsage {
        let _guard = self.quota_lock.lock();
        let used = self.used_bytes.load(Ordering::Acquire);
        let reserved = self.reserved_bytes.load(Ordering::Acquire);
        let quota = self.quota_bytes;
        let total = used + reserved;

        KvQuotaUsage {
            tenant_id: self.tenant_id.clone(),
            used_bytes: used,
            reserved_bytes: reserved,
            quota_bytes: quota,
            available_bytes: quota.map(|q| q.saturating_sub(total)).unwrap_or(u64::MAX),
            usage_pct: quota
                .map(|q| {
                    if q > 0 {
                        (total as f64 / q as f64) * 100.0
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0),
        }
    }

    /// Clean up expired reservations
    fn cleanup_expired(&self) {
        let mut reservations = self.reservations.write();
        let before = reservations.len();

        let mut expired_bytes = 0u64;
        reservations.retain(|r| {
            if r.is_expired() {
                expired_bytes += r.size_bytes;
                false
            } else {
                true
            }
        });

        if expired_bytes > 0 {
            self.reserved_bytes
                .fetch_sub(expired_bytes, Ordering::AcqRel);
            warn!(
                tenant_id = %self.tenant_id,
                expired_count = before - reservations.len(),
                expired_bytes = expired_bytes,
                "Cleaned up expired KV reservations"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_check_within_limit() {
        let qm = TenantKvQuotaManager::new("tenant-a".to_string(), Some(1024));
        assert!(qm.check_quota(512).is_ok());
    }

    #[test]
    fn test_quota_check_exceeds_limit() {
        let qm = TenantKvQuotaManager::new("tenant-a".to_string(), Some(1024));
        let result = qm.check_quota(2048);
        assert!(result.is_err());
    }

    #[test]
    fn test_reservation_flow() {
        let qm = TenantKvQuotaManager::new("tenant-a".to_string(), Some(1024));

        // Reserve 512 bytes
        let res = qm.reserve(512).unwrap();
        assert_eq!(qm.usage().reserved_bytes, 512);

        // Finalize reservation
        qm.finalize(res).unwrap();
        assert_eq!(qm.usage().used_bytes, 512);
        assert_eq!(qm.usage().reserved_bytes, 0);
    }

    #[test]
    fn test_reservation_rollback() {
        let qm = TenantKvQuotaManager::new("tenant-a".to_string(), Some(1024));

        let res = qm.reserve(512).unwrap();
        qm.rollback(res);

        assert_eq!(qm.usage().reserved_bytes, 0);
        assert_eq!(qm.usage().used_bytes, 0);
    }

    #[test]
    fn test_unlimited_quota() {
        let qm = TenantKvQuotaManager::new("tenant-a".to_string(), None);
        assert!(qm.check_quota(u64::MAX / 2).is_ok());
        assert!(!qm.is_quota_enforced());
    }

    #[test]
    fn test_release_bytes() {
        let qm = TenantKvQuotaManager::new("tenant-a".to_string(), Some(1024));

        let res = qm.reserve(512).unwrap();
        qm.finalize(res).unwrap();
        assert_eq!(qm.usage().used_bytes, 512);

        qm.release(256);
        assert_eq!(qm.usage().used_bytes, 256);
    }

    #[test]
    fn test_eviction_counter() {
        let qm = TenantKvQuotaManager::new("tenant-a".to_string(), Some(1024));

        assert_eq!(qm.evictions(), 0);
        qm.record_eviction();
        qm.record_eviction();
        assert_eq!(qm.evictions(), 2);

        qm.reset_evictions();
        assert_eq!(qm.evictions(), 0);
    }
}
