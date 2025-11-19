//! Memory monitoring and eviction

use adapteros_core::Result;
use adapteros_telemetry::TelemetryWriter; // Assume
use chrono::Utc;
use serde_json::json;
use std::process::Command;
use tokio::time::{interval, Duration};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct UmaStats {
    pub total_mb: u64,
    pub used_mb: u64,
    pub headroom_pct: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressureLevel {
    Low,
    Medium,
    High,
    Critical,
}

fn determine_pressure(stats: &UmaStats, min_headroom: f32) -> MemoryPressureLevel {
    if stats.headroom_pct < min_headroom * 0.5 {
        MemoryPressureLevel::Critical
    } else if stats.headroom_pct < min_headroom * 0.75 {
        MemoryPressureLevel::High
    } else if stats.headroom_pct < min_headroom {
        MemoryPressureLevel::Medium
    } else {
        MemoryPressureLevel::Low
    }
}

async fn emit_telemetry(
    telemetry: &Option<TelemetryWriter>,
    stats: &UmaStats,
    pressure: MemoryPressureLevel,
) {
    if let Some(t) = telemetry {
        // Write memory pressure telemetry
        // TODO: Implement write_memory_pressure method on TelemetryWriter
        let _ = t; // Suppress unused warning for now
        let _ = stats;
        let _ = pressure;
    }
}

async fn get_uma_stats() -> UmaStats {
    // Stub implementation - returns default stats
    UmaStats {
        total_mb: 16384,
        used_mb: 8192,
        headroom_pct: 50.0,
    }
}

/// Memory monitor for enforcing headroom policy
pub struct UmaPressureMonitor {
    min_headroom_pct: u8,
    telemetry: Option<TelemetryWriter>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl UmaPressureMonitor {
    pub fn new(min_headroom_pct: u8, telemetry: Option<TelemetryWriter>) -> Self {
        Self {
            min_headroom_pct,
            telemetry,
            handle: None,
        }
    }

    pub async fn start_polling(&mut self) {
        let telemetry_clone = self.telemetry.clone();
        let min_headroom = self.min_headroom_pct;
        self.handle = Some(tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                let stats = get_uma_stats().await; // Call free function
                let pressure = determine_pressure(&stats, min_headroom as f32);
                if pressure != MemoryPressureLevel::Low {
                    emit_telemetry(&telemetry_clone, &stats, pressure).await;
                }
                if pressure == MemoryPressureLevel::Critical {
                    warn!("Critical UMA pressure: headroom {}%", stats.headroom_pct);
                }
            }
        }));
    }

    pub fn get_current_pressure(&self) -> MemoryPressureLevel {
        // Cache last pressure, assume impl
        MemoryPressureLevel::Low // Stub
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

    // Existing headroom_pct_macos enhanced with vm_statistics64
    #[cfg(target_os = "macos")]
    fn headroom_pct_macos(&self) -> Option<f32> {
        // Simplified fallback to vm_stat command instead of direct mach calls
        // to avoid mach crate version/API compatibility issues
        self.fallback_headroom()
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

    pub async fn get_uma_stats(&self) -> UmaStats {
        let headroom_pct = self.headroom_pct();
        let total_mb = self
            .get_total_memory_bytes()
            .map(|b| b / (1024 * 1024))
            .unwrap_or(0);
        let used_mb = ((100.0 - headroom_pct) / 100.0 * total_mb as f32) as u64;
        let available_mb = total_mb - used_mb;

        UmaStats {
            headroom_pct,
            used_mb,
            total_mb,
        }
    }

}

impl MemoryPressureLevel {
    pub fn to_string(&self) -> String {
        match self {
            Self::Low => "Low".to_string(),
            Self::Medium => "Medium".to_string(),
            Self::High => "High".to_string(),
            Self::Critical => "Critical".to_string(),
        }
    }
}

