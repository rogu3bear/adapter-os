//! Memory monitoring and eviction

use adapteros_core::Result;
use adapteros_telemetry::TelemetryWriter; // Assume
use chrono::Utc;
use parking_lot::RwLock;
use serde_json::json;
use std::process::Command;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::warn;

/// Memory monitor for enforcing headroom policy
pub struct UmaPressureMonitor {
    min_headroom_pct: u8,
    telemetry: Option<TelemetryWriter>,
    handle: Option<tokio::task::JoinHandle<()>>,
    cached_pressure: Arc<RwLock<MemoryPressureLevel>>,
}

impl UmaPressureMonitor {
    pub fn new(min_headroom_pct: u8, telemetry: Option<TelemetryWriter>) -> Self {
        Self {
            min_headroom_pct,
            telemetry,
            handle: None,
            cached_pressure: Arc::new(RwLock::new(MemoryPressureLevel::Low)),
        }
    }

    pub async fn start_polling(&mut self) {
        let telemetry_clone = self.telemetry.clone();
        let min_headroom = self.min_headroom_pct;
        let pressure_cache = self.cached_pressure.clone();
        self.handle = Some(tokio::spawn(async move {
            // Import backoff utilities from parent crate
            use crate::backoff::{BackoffConfig, CircuitBreaker as BackoffCircuitBreaker};

            let backoff =
                BackoffConfig::new(Duration::from_millis(1000), Duration::from_secs(30), 2.0, 5);
            let circuit_breaker = BackoffCircuitBreaker::new(10, Duration::from_secs(300));
            let mut consecutive_failures = 0u32;

            let mut interval = interval(Duration::from_secs(5));
            loop {
                interval.tick().await;

                // Check circuit breaker state
                if circuit_breaker.is_open() {
                    warn!(
                        failure_count = circuit_breaker.failure_count(),
                        "Memory monitoring circuit breaker is open, pausing"
                    );
                    tokio::time::sleep(circuit_breaker.reset_timeout()).await;
                    continue;
                }

                // Attempt to get memory stats
                match tokio::task::spawn_blocking(|| {
                    // Run potentially blocking system calls in a blocking thread
                    std::panic::catch_unwind(get_uma_stats)
                })
                .await
                {
                    Ok(Ok(stats)) => {
                        // Success - reset backoff and circuit breaker
                        circuit_breaker.record_success();
                        consecutive_failures = 0;

                        let pressure = determine_pressure(&stats, min_headroom as f32);

                        // Update cached pressure level
                        *pressure_cache.write() = pressure;

                        if pressure != MemoryPressureLevel::Low {
                            emit_telemetry(&telemetry_clone, &stats, pressure).await;
                        }
                        if pressure == MemoryPressureLevel::Critical {
                            warn!("Critical UMA pressure: headroom {}%", stats.headroom_pct);
                        }
                    }
                    Ok(Err(panic_err)) => {
                        // Panic in stats collection
                        circuit_breaker.record_failure();
                        consecutive_failures += 1;

                        warn!(
                            error = ?panic_err,
                            consecutive_failures = consecutive_failures,
                            "Memory stats collection panicked"
                        );

                        // Apply backoff
                        let delay = backoff.next_delay(consecutive_failures);
                        tokio::time::sleep(delay).await;
                    }
                    Err(join_err) => {
                        // Task join error
                        circuit_breaker.record_failure();
                        consecutive_failures += 1;

                        warn!(
                            error = %join_err,
                            consecutive_failures = consecutive_failures,
                            "Memory monitoring task failed"
                        );

                        // Apply backoff
                        let delay = backoff.next_delay(consecutive_failures);
                        tokio::time::sleep(delay).await;
                    }
                }

                // Extended backoff if we've exceeded max retries
                if backoff.should_give_up(consecutive_failures) {
                    warn!(
                        "Memory monitoring has failed {} times, entering extended backoff",
                        consecutive_failures
                    );
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    consecutive_failures = 0;
                }
            }
        }));
    }

    pub fn get_current_pressure(&self) -> MemoryPressureLevel {
        *self.cached_pressure.read()
    }

    /// Override cached pressure (intended for tests and diagnostics).
    pub fn set_pressure_for_test(&self, level: MemoryPressureLevel) {
        *self.cached_pressure.write() = level;
    }

    /// Compute the current pressure level using live headroom measurement.
    /// Falls back to the same thresholds used by the polling task.
    pub async fn current_pressure_level(&self) -> MemoryPressureLevel {
        let stats =
            match tokio::task::spawn_blocking(|| std::panic::catch_unwind(get_uma_stats)).await {
                Ok(Ok(stats)) => stats,
                Ok(Err(panic_err)) => {
                    warn!(
                        error = ?panic_err,
                        "Memory stats collection panicked during live pressure check"
                    );
                    self.fallback_stats()
                }
                Err(join_err) => {
                    warn!(
                        error = %join_err,
                        "Memory stats task failed during live pressure check"
                    );
                    self.fallback_stats()
                }
            };

        let level = determine_pressure(&stats, self.min_headroom_pct as f32);
        // Update cache so subsequent reads stay consistent even if polling is disabled.
        *self.cached_pressure.write() = level;
        level
    }

    /// Check if headroom meets minimum
    pub fn check_headroom(&self) -> Result<()> {
        let headroom = self.headroom_pct();
        if headroom < self.min_headroom_pct as f32 {
            return Err(adapteros_core::AosError::MemoryPressure(format!(
                "Insufficient memory headroom: {:.1}% < {}%",
                headroom, self.min_headroom_pct
            )));
        }
        Ok(())
    }

    /// Get current headroom percentage
    pub fn headroom_pct(&self) -> f32 {
        #[cfg(target_os = "macos")]
        {
            self.headroom_pct_macos().unwrap_or(20.0)
        }

        #[cfg(target_os = "linux")]
        {
            self.headroom_pct_linux().unwrap_or(20.0)
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            // Fallback for unsupported platforms
            20.0
        }
    }

    fn fallback_stats(&self) -> UmaStats {
        UmaStats {
            headroom_pct: self.headroom_pct(),
            used_mb: 0,
            total_mb: 0,
            available_mb: 0,
            ane_allocated_mb: None,
            ane_used_mb: None,
            ane_available_mb: None,
            ane_usage_percent: None,
        }
    }

    // Existing headroom_pct_macos enhanced with vm_statistics64
    #[cfg(target_os = "macos")]
    fn headroom_pct_macos(&self) -> Option<f32> {
        use libc::{HOST_VM_INFO64, KERN_SUCCESS};
        use mach::kern_return::kern_return_t;
        use mach::mach_types::host_t;
        use mach::message::mach_msg_type_number_t;
        use mach::vm_types::vm_size_t;

        // vm_statistics64 structure
        #[repr(C)]
        struct vm_statistics64 {
            free_count: u32,
            active_count: u32,
            inactive_count: u32,
            wire_count: u32,
            zero_fill_count: u64,
            reactivations: u64,
            pageins: u64,
            pageouts: u64,
            faults: u64,
            cow_faults: u64,
            lookups: u64,
            hits: u64,
            purges: u64,
            purgeable_count: u32,
            speculative_count: u32,
            decompressions: u64,
            compressions: u64,
            swapins: u64,
            swapouts: u64,
            compressor_page_count: u32,
            throttled_count: u32,
            external_page_count: u32,
            internal_page_count: u32,
            total_uncompressed_pages_in_compressor: u64,
        }

        extern "C" {
            fn mach_host_self() -> host_t;
            fn host_statistics64(
                host_priv: host_t,
                flavor: i32,
                host_info_out: *mut i32,
                host_info_outCnt: *mut mach_msg_type_number_t,
            ) -> kern_return_t;
            static vm_kernel_page_size: vm_size_t;
        }

        // SAFETY: mach_host_self() is a Mach API that returns the host port for the current task.
        // It always succeeds and returns a valid port.
        let host: host_t = unsafe { mach_host_self() };
        // SAFETY: vm_statistics64 is a plain C struct; zeroing it is safe initialization.
        let mut stats: vm_statistics64 = unsafe { std::mem::zeroed() };
        let mut count = (std::mem::size_of::<vm_statistics64>() / std::mem::size_of::<u32>())
            as mach_msg_type_number_t;

        // SAFETY: host_statistics64 is a Mach API that populates the stats struct with VM info.
        // The host port is valid (from mach_host_self), HOST_VM_INFO64 is a valid flavor,
        // and stats is properly sized and aligned for vm_statistics64.
        let result = unsafe {
            host_statistics64(
                host,
                HOST_VM_INFO64,
                &mut stats as *mut vm_statistics64 as *mut i32,
                &mut count,
            )
        };

        if result != KERN_SUCCESS {
            return self.fallback_headroom(); // Use existing vm_stat
        }

        // SAFETY: vm_kernel_page_size is a kernel-exported constant, always valid.
        let page_size = unsafe { vm_kernel_page_size as u64 };
        let total_bytes = self.get_total_memory_bytes()?; // sysctl hw.memsize

        let active = (stats.active_count as u64).saturating_mul(page_size);
        let _inactive = (stats.inactive_count as u64).saturating_mul(page_size);
        let wired = (stats.wire_count as u64).saturating_mul(page_size);
        let compressed = (stats.compressor_page_count as u64).saturating_mul(page_size);

        let used_bytes = active + wired + compressed;
        let available_bytes = total_bytes.saturating_sub(used_bytes);
        let headroom_pct = (available_bytes as f32 / total_bytes as f32) * 100.0;

        Some(headroom_pct)
    }

    fn get_total_memory_bytes(&self) -> Option<u64> {
        let output = Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&output.stdout).trim().parse().ok()
    }

    fn fallback_headroom(&self) -> Option<f32> {
        // Existing vm_stat logic
        use std::fs;

        let meminfo = fs::read_to_string("/proc/meminfo").ok()?;

        let mut mem_total = None;
        let mut mem_available = None;

        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                mem_total = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse::<u64>().ok());
            } else if line.starts_with("MemAvailable:") {
                mem_available = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse::<u64>().ok());
            }

            if mem_total.is_some() && mem_available.is_some() {
                break;
            }
        }

        let total = mem_total?;
        let available = mem_available?;

        if total == 0 {
            return None;
        }

        let free_pct = (available as f32 / total as f32) * 100.0;
        Some(free_pct)
    }

    /// Check if eviction needed
    pub fn should_evict(&self) -> bool {
        self.headroom_pct() < self.min_headroom_pct as f32
    }

    /// Calculate UMA statistics from current state
    fn calculate_uma_stats(&self) -> UmaStats {
        let headroom_pct = self.headroom_pct();
        let total_mb = self
            .get_total_memory_bytes()
            .map(|b| b / (1024 * 1024))
            .unwrap_or(0);
        let used_mb = ((100.0 - headroom_pct) / 100.0 * total_mb as f32) as u64;
        let available_mb = total_mb - used_mb;

        // Collect ANE metrics if on macOS
        let (ane_allocated_mb, ane_used_mb, ane_available_mb, ane_usage_percent) =
            self.get_ane_metrics();

        UmaStats {
            headroom_pct,
            used_mb,
            total_mb,
            available_mb,
            ane_allocated_mb,
            ane_used_mb,
            ane_available_mb,
            ane_usage_percent,
        }
    }

    /// Get UMA statistics (async version)
    pub async fn get_uma_stats(&self) -> UmaStats {
        self.calculate_uma_stats()
    }

    /// Get current memory stats (synchronous version)
    pub fn get_stats(&self) -> UmaStats {
        self.calculate_uma_stats()
    }

    /// Get available memory in MB
    ///
    /// Returns the current available memory (headroom) in megabytes.
    /// Used for VRAM validation before adapter preload.
    pub fn get_available_mb(&self) -> u64 {
        let stats = self.calculate_uma_stats();
        stats.available_mb
    }

    /// Get ANE-specific metrics
    fn get_ane_metrics(&self) -> (Option<u64>, Option<u64>, Option<u64>, Option<f32>) {
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;

            // Check if we're on Apple Silicon
            let is_apple_silicon = Command::new("sysctl")
                .args(["-n", "machdep.cpu.brand_string"])
                .output()
                .ok()
                .map(|output| {
                    let brand = String::from_utf8_lossy(&output.stdout);
                    brand.contains("Apple")
                })
                .unwrap_or(false);

            if !is_apple_silicon {
                return (None, None, None, None);
            }

            // Get total system memory
            let total_bytes = self.get_total_memory_bytes().unwrap_or(0);
            if total_bytes == 0 {
                return (None, None, None, None);
            }

            // Estimate ANE allocation: 15-20% of system memory on Apple Silicon
            let ane_allocated_bytes = (total_bytes as f64 * 0.18) as u64;
            let ane_allocated_mb = ane_allocated_bytes / (1024 * 1024);

            // Estimate ANE usage based on compressor activity (proxy for ML workload)
            let ane_usage_pct = self.estimate_ane_usage_pct().unwrap_or(0.0);
            let ane_used_mb = (ane_allocated_mb as f64 * ane_usage_pct as f64 / 100.0) as u64;
            let ane_available_mb = ane_allocated_mb.saturating_sub(ane_used_mb);

            (
                Some(ane_allocated_mb),
                Some(ane_used_mb),
                Some(ane_available_mb),
                Some(ane_usage_pct),
            )
        }

        #[cfg(not(target_os = "macos"))]
        {
            (None, None, None, None)
        }
    }

    #[cfg(target_os = "macos")]
    fn estimate_ane_usage_pct(&self) -> Option<f32> {
        use std::process::Command;

        let vm_stat = Command::new("vm_stat")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())?;

        let mut pages_compressed = 0u64;
        let mut pages_total = 0u64;

        for line in vm_stat.lines() {
            if line.contains("Pages occupied by compressor:") {
                pages_compressed = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().trim_end_matches('.').parse().ok())
                    .unwrap_or(0);
            } else if line.contains("Pages active:") || line.contains("Pages wired down:") {
                let pages: u64 = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().trim_end_matches('.').parse().ok())
                    .unwrap_or(0);
                pages_total += pages;
            }
        }

        if pages_total == 0 {
            return Some(0.0);
        }

        // Estimate ANE usage based on compression activity
        let compression_ratio = pages_compressed as f64 / pages_total as f64;
        let estimated_usage = (compression_ratio * 100.0).min(100.0) as f32;

        Some(estimated_usage)
    }
}

// Memory pressure level ordering: Low < Medium < High < Critical
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryPressureLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for MemoryPressureLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "Low"),
            Self::Medium => write!(f, "Medium"),
            Self::High => write!(f, "High"),
            Self::Critical => write!(f, "Critical"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tiered eviction policy (manifest `memory.evict_order`)
// ---------------------------------------------------------------------------

/// Eviction tier matching the manifest's three-tier cascade:
/// `["ephemeral_ttl", "cold_lru", "warm_lru"]`
///
/// Lower-ordinal tiers are evicted first. Within a tier, candidates are
/// sorted by heat score ascending (coldest first).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EvictionTier {
    /// Ephemeral adapters whose TTL has expired — evict immediately.
    EphemeralTtl = 0,
    /// Cold adapters: fewer than `COLD_THRESHOLD_UPM` uses per minute.
    ColdLru = 1,
    /// Warm adapters: above cold threshold but still eligible for eviction.
    WarmLru = 2,
}

impl EvictionTier {
    /// Parse from the manifest string representation.
    pub fn from_manifest_str(s: &str) -> Option<Self> {
        match s {
            "ephemeral_ttl" => Some(Self::EphemeralTtl),
            "cold_lru" => Some(Self::ColdLru),
            "warm_lru" => Some(Self::WarmLru),
            _ => None,
        }
    }

    /// Manifest string representation.
    pub fn as_manifest_str(&self) -> &'static str {
        match self {
            Self::EphemeralTtl => "ephemeral_ttl",
            Self::ColdLru => "cold_lru",
            Self::WarmLru => "warm_lru",
        }
    }
}

impl std::fmt::Display for EvictionTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_manifest_str())
    }
}

/// Uses-per-minute threshold below which an adapter is considered "cold".
const COLD_THRESHOLD_UPM: u32 = 2;

/// Snapshot of adapter metadata needed for tier classification.
///
/// Designed to be built from `AdapterLoadState` + manifest `Adapter` metadata
/// without coupling directly to either type.
#[derive(Debug, Clone)]
pub struct AdapterEvictionMeta {
    pub adapter_id: String,
    /// Is this adapter ephemeral (has a TTL)?
    pub is_ephemeral: bool,
    /// Configured TTL in seconds (from manifest `Adapter.ttl`).
    pub ttl_secs: Option<u32>,
    /// Seconds since this adapter was loaded.
    pub loaded_age_secs: u64,
    /// Uses per minute from the sliding window in `AdapterLoadState`.
    pub uses_per_minute: u32,
    /// VRAM footprint in bytes.
    pub vram_bytes: u64,
}

impl AdapterEvictionMeta {
    /// Compute the heat score.
    ///
    /// Higher = hotter (used more recently / frequently). Within a tier,
    /// candidates are sorted ascending by heat so coldest are evicted first.
    pub fn heat_score(&self) -> u32 {
        self.uses_per_minute
    }

    /// Has this adapter's TTL expired?
    pub fn ttl_expired(&self) -> bool {
        match (self.is_ephemeral, self.ttl_secs) {
            (true, Some(ttl)) => self.loaded_age_secs >= ttl as u64,
            _ => false,
        }
    }
}

/// Classify an adapter into its eviction tier.
pub fn classify_eviction_tier(meta: &AdapterEvictionMeta) -> EvictionTier {
    if meta.is_ephemeral && meta.ttl_expired() {
        return EvictionTier::EphemeralTtl;
    }
    if meta.uses_per_minute < COLD_THRESHOLD_UPM {
        return EvictionTier::ColdLru;
    }
    EvictionTier::WarmLru
}

/// A tier-classified eviction candidate ready for sorting.
#[derive(Debug, Clone)]
pub struct TieredEvictionCandidate {
    pub adapter_id: String,
    pub tier: EvictionTier,
    pub heat_score: u32,
    pub vram_bytes: u64,
}

/// Sort eviction candidates according to the manifest's `evict_order`.
///
/// The `evict_order` slice defines tier priority (first entry = evict first).
/// Within a tier, candidates are sorted by heat ascending (coldest first),
/// then by adapter ID for determinism.
///
/// Candidates whose tier does not appear in `evict_order` are pushed to the end.
pub fn sort_eviction_candidates(
    candidates: &mut [TieredEvictionCandidate],
    evict_order: &[String],
) {
    // Build tier → priority index from the manifest order.
    let tier_priority: std::collections::HashMap<EvictionTier, usize> = evict_order
        .iter()
        .enumerate()
        .filter_map(|(i, s)| EvictionTier::from_manifest_str(s).map(|t| (t, i)))
        .collect();

    let max_priority = evict_order.len();

    candidates.sort_by(|a, b| {
        let pa = tier_priority.get(&a.tier).copied().unwrap_or(max_priority);
        let pb = tier_priority.get(&b.tier).copied().unwrap_or(max_priority);
        pa.cmp(&pb)
            .then_with(|| a.heat_score.cmp(&b.heat_score))
            .then_with(|| a.adapter_id.cmp(&b.adapter_id))
    });
}

/// Build tier-classified eviction candidates from raw metadata.
///
/// Filters out adapters that should never be evicted (e.g., persistent
/// non-expired adapters could be excluded upstream). This function only
/// classifies — the caller decides which adapters are eligible.
pub fn classify_candidates(
    metas: impl IntoIterator<Item = AdapterEvictionMeta>,
) -> Vec<TieredEvictionCandidate> {
    metas
        .into_iter()
        .map(|m| {
            let tier = classify_eviction_tier(&m);
            let heat_score = m.heat_score();
            TieredEvictionCandidate {
                adapter_id: m.adapter_id,
                tier,
                heat_score,
                vram_bytes: m.vram_bytes,
            }
        })
        .collect()
}

// Unit test
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pressure_levels() {
        let stats = UmaStats {
            headroom_pct: 25.0,
            used_mb: 12000,
            total_mb: 16000,
            available_mb: 4000,
            ane_allocated_mb: None,
            ane_used_mb: None,
            ane_available_mb: None,
            ane_usage_percent: None,
        };
        let level = determine_pressure(&stats, 15.0);
        assert_eq!(level, MemoryPressureLevel::Medium);

        let critical = UmaStats {
            headroom_pct: 10.0,
            used_mb: 14400,
            total_mb: 16000,
            available_mb: 1600,
            ane_allocated_mb: None,
            ane_used_mb: None,
            ane_available_mb: None,
            ane_usage_percent: None,
        };
        let level = determine_pressure(&critical, 15.0);
        assert_eq!(level, MemoryPressureLevel::Critical);
    }

    // --- Eviction tier tests ---

    fn make_meta(
        id: &str,
        is_ephemeral: bool,
        ttl_secs: Option<u32>,
        loaded_age_secs: u64,
        uses_per_minute: u32,
        vram_bytes: u64,
    ) -> AdapterEvictionMeta {
        AdapterEvictionMeta {
            adapter_id: id.to_string(),
            is_ephemeral,
            ttl_secs,
            loaded_age_secs,
            uses_per_minute,
            vram_bytes,
        }
    }

    #[test]
    fn test_classify_ephemeral_ttl_expired() {
        let meta = make_meta("eph-1", true, Some(300), 600, 10, 1024);
        assert!(meta.ttl_expired());
        assert_eq!(classify_eviction_tier(&meta), EvictionTier::EphemeralTtl);
    }

    #[test]
    fn test_classify_ephemeral_not_expired() {
        let meta = make_meta("eph-2", true, Some(300), 100, 0, 1024);
        assert!(!meta.ttl_expired());
        // Under cold threshold, so ColdLru
        assert_eq!(classify_eviction_tier(&meta), EvictionTier::ColdLru);
    }

    #[test]
    fn test_classify_cold_lru() {
        let meta = make_meta("cold-1", false, None, 1000, 1, 2048);
        assert_eq!(classify_eviction_tier(&meta), EvictionTier::ColdLru);
    }

    #[test]
    fn test_classify_warm_lru() {
        let meta = make_meta("warm-1", false, None, 500, 5, 4096);
        assert_eq!(classify_eviction_tier(&meta), EvictionTier::WarmLru);
    }

    #[test]
    fn test_heat_score_is_upm() {
        let meta = make_meta("a", false, None, 0, 42, 0);
        assert_eq!(meta.heat_score(), 42);
    }

    #[test]
    fn test_sort_eviction_candidates_tier_order() {
        let order = vec![
            "ephemeral_ttl".to_string(),
            "cold_lru".to_string(),
            "warm_lru".to_string(),
        ];

        let mut candidates = vec![
            TieredEvictionCandidate {
                adapter_id: "warm-a".into(),
                tier: EvictionTier::WarmLru,
                heat_score: 5,
                vram_bytes: 100,
            },
            TieredEvictionCandidate {
                adapter_id: "cold-b".into(),
                tier: EvictionTier::ColdLru,
                heat_score: 1,
                vram_bytes: 200,
            },
            TieredEvictionCandidate {
                adapter_id: "eph-c".into(),
                tier: EvictionTier::EphemeralTtl,
                heat_score: 0,
                vram_bytes: 50,
            },
        ];

        sort_eviction_candidates(&mut candidates, &order);

        assert_eq!(candidates[0].adapter_id, "eph-c");
        assert_eq!(candidates[1].adapter_id, "cold-b");
        assert_eq!(candidates[2].adapter_id, "warm-a");
    }

    #[test]
    fn test_sort_within_tier_by_heat_then_id() {
        let order = vec!["cold_lru".to_string()];

        let mut candidates = vec![
            TieredEvictionCandidate {
                adapter_id: "z-cold".into(),
                tier: EvictionTier::ColdLru,
                heat_score: 1,
                vram_bytes: 100,
            },
            TieredEvictionCandidate {
                adapter_id: "a-cold".into(),
                tier: EvictionTier::ColdLru,
                heat_score: 1,
                vram_bytes: 200,
            },
            TieredEvictionCandidate {
                adapter_id: "m-cold".into(),
                tier: EvictionTier::ColdLru,
                heat_score: 0,
                vram_bytes: 50,
            },
        ];

        sort_eviction_candidates(&mut candidates, &order);

        // m-cold has lowest heat (0), then a-cold and z-cold tie on heat (1) broken by ID
        assert_eq!(candidates[0].adapter_id, "m-cold");
        assert_eq!(candidates[1].adapter_id, "a-cold");
        assert_eq!(candidates[2].adapter_id, "z-cold");
    }

    #[test]
    fn test_classify_candidates_integration() {
        let metas = vec![
            make_meta("eph-expired", true, Some(60), 120, 10, 1000),
            make_meta("eph-alive", true, Some(300), 10, 0, 500),
            make_meta("persistent-hot", false, None, 3600, 20, 2000),
            make_meta("persistent-cold", false, None, 7200, 0, 3000),
        ];

        let mut candidates = classify_candidates(metas);
        let order = vec![
            "ephemeral_ttl".to_string(),
            "cold_lru".to_string(),
            "warm_lru".to_string(),
        ];
        sort_eviction_candidates(&mut candidates, &order);

        assert_eq!(candidates.len(), 4);
        // First: expired ephemeral
        assert_eq!(candidates[0].adapter_id, "eph-expired");
        assert_eq!(candidates[0].tier, EvictionTier::EphemeralTtl);
        // Then cold adapters (eph-alive has 0 upm, persistent-cold has 0 upm)
        assert_eq!(candidates[1].tier, EvictionTier::ColdLru);
        assert_eq!(candidates[2].tier, EvictionTier::ColdLru);
        // Finally warm
        assert_eq!(candidates[3].adapter_id, "persistent-hot");
        assert_eq!(candidates[3].tier, EvictionTier::WarmLru);
    }

    #[test]
    fn test_eviction_tier_from_manifest_str() {
        assert_eq!(
            EvictionTier::from_manifest_str("ephemeral_ttl"),
            Some(EvictionTier::EphemeralTtl)
        );
        assert_eq!(
            EvictionTier::from_manifest_str("cold_lru"),
            Some(EvictionTier::ColdLru)
        );
        assert_eq!(
            EvictionTier::from_manifest_str("warm_lru"),
            Some(EvictionTier::WarmLru)
        );
        assert_eq!(EvictionTier::from_manifest_str("unknown"), None);
    }

    #[test]
    fn test_eviction_tier_ordering() {
        assert!(EvictionTier::EphemeralTtl < EvictionTier::ColdLru);
        assert!(EvictionTier::ColdLru < EvictionTier::WarmLru);
    }

    #[test]
    fn test_ttl_none_never_expires() {
        // Ephemeral without a TTL never expires via TTL logic
        let meta = make_meta("no-ttl", true, None, 999999, 0, 100);
        assert!(!meta.ttl_expired());
        assert_eq!(classify_eviction_tier(&meta), EvictionTier::ColdLru);
    }

    #[test]
    fn test_persistent_never_ephemeral_tier() {
        // Even with loaded_age > some hypothetical TTL, persistent adapters
        // should never classify as EphemeralTtl
        let meta = make_meta("persistent", false, Some(60), 9999, 0, 100);
        assert!(!meta.ttl_expired());
        assert_eq!(classify_eviction_tier(&meta), EvictionTier::ColdLru);
    }
}

fn determine_pressure(stats: &UmaStats, min_headroom: f32) -> MemoryPressureLevel {
    let headroom = stats.headroom_pct;
    if headroom < min_headroom {
        MemoryPressureLevel::Critical
    } else if headroom < 20.0 {
        MemoryPressureLevel::High
    } else if headroom < 30.0 {
        MemoryPressureLevel::Medium
    } else {
        MemoryPressureLevel::Low
    }
}

async fn emit_telemetry(
    telemetry: &Option<TelemetryWriter>,
    stats: &UmaStats,
    level: MemoryPressureLevel,
) {
    if let Some(t) = telemetry {
        let _ = t.log(
            "uma.pressure",
            json!({
                "level": level.to_string(),
                "headroom_pct": stats.headroom_pct,
                "used_mb": stats.used_mb,
                "available_mb": stats.total_mb - stats.used_mb, // Calculate available_mb
                "total_mb": stats.total_mb,
                "timestamp": Utc::now().timestamp()
            }),
        );
    }
}

#[derive(Clone)]
pub struct UmaStats {
    pub headroom_pct: f32,
    pub used_mb: u64,
    pub total_mb: u64,
    pub available_mb: u64,
    /// ANE-specific memory statistics (populated on macOS with Apple Silicon)
    pub ane_allocated_mb: Option<u64>,
    pub ane_used_mb: Option<u64>,
    pub ane_available_mb: Option<u64>,
    pub ane_usage_percent: Option<f32>,
}

/// Standalone function to get UMA stats for use in spawned tasks
fn get_uma_stats() -> UmaStats {
    // Use sysctl to get memory info on macOS
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let total_bytes = Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .unwrap_or(0);

        // Get used memory from vm_stat
        let vm_stat = Command::new("vm_stat")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let page_size: u64 = 4096; // Typical page size
        let mut pages_active = 0u64;
        let mut pages_wired = 0u64;
        let mut pages_compressed = 0u64;

        for line in vm_stat.lines() {
            if line.contains("Pages active:") {
                pages_active = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().trim_end_matches('.').parse().ok())
                    .unwrap_or(0);
            } else if line.contains("Pages wired down:") {
                pages_wired = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().trim_end_matches('.').parse().ok())
                    .unwrap_or(0);
            } else if line.contains("Pages occupied by compressor:") {
                pages_compressed = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().trim_end_matches('.').parse().ok())
                    .unwrap_or(0);
            }
        }

        let used_bytes = (pages_active + pages_wired + pages_compressed) * page_size;
        let total_mb = total_bytes / (1024 * 1024);
        let used_mb = used_bytes / (1024 * 1024);
        let available_mb = total_mb.saturating_sub(used_mb);
        let headroom_bytes = total_bytes.saturating_sub(used_bytes);
        let headroom_pct = if total_bytes > 0 {
            (headroom_bytes as f32 / total_bytes as f32) * 100.0
        } else {
            20.0
        };

        // ANE metrics for Apple Silicon
        let (ane_allocated_mb, ane_used_mb, ane_available_mb, ane_usage_percent) =
            get_ane_metrics_standalone(total_bytes, pages_compressed, pages_active + pages_wired);

        UmaStats {
            headroom_pct,
            used_mb,
            total_mb,
            available_mb,
            ane_allocated_mb,
            ane_used_mb,
            ane_available_mb,
            ane_usage_percent,
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Fallback for other platforms
        UmaStats {
            headroom_pct: 20.0,
            used_mb: 0,
            total_mb: 0,
            available_mb: 0,
            ane_allocated_mb: None,
            ane_used_mb: None,
            ane_available_mb: None,
            ane_usage_percent: None,
        }
    }
}

/// Get ANE metrics for standalone function
#[cfg(target_os = "macos")]
fn get_ane_metrics_standalone(
    total_bytes: u64,
    pages_compressed: u64,
    pages_total: u64,
) -> (Option<u64>, Option<u64>, Option<u64>, Option<f32>) {
    use std::process::Command;

    // Check if we're on Apple Silicon
    let is_apple_silicon = Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()
        .map(|output| {
            let brand = String::from_utf8_lossy(&output.stdout);
            brand.contains("Apple")
        })
        .unwrap_or(false);

    if !is_apple_silicon || total_bytes == 0 {
        return (None, None, None, None);
    }

    // Estimate ANE allocation: 15-20% of system memory
    let ane_allocated_bytes = (total_bytes as f64 * 0.18) as u64;
    let ane_allocated_mb = ane_allocated_bytes / (1024 * 1024);

    // Estimate ANE usage based on compression activity
    let ane_usage_pct = if pages_total > 0 {
        let compression_ratio = pages_compressed as f64 / pages_total as f64;
        ((compression_ratio * 100.0).min(100.0)) as f32
    } else {
        0.0
    };

    let ane_used_mb = (ane_allocated_mb as f64 * ane_usage_pct as f64 / 100.0) as u64;
    let ane_available_mb = ane_allocated_mb.saturating_sub(ane_used_mb);

    (
        Some(ane_allocated_mb),
        Some(ane_used_mb),
        Some(ane_available_mb),
        Some(ane_usage_pct),
    )
}
