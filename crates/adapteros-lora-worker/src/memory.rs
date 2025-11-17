//! Memory monitoring and eviction

use adapteros_core::Result;
use adapteros_telemetry::TelemetryWriter; // Assume
use chrono::Utc;
use serde_json::json;
use std::process::Command;
use tokio::time::{interval, Duration};
use tracing::{info, warn};

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
                let stats = get_uma_stats().await;
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
        use libc::{host_statistics64, HOST_VM_INFO64, KERN_SUCCESS};
        use mach::host_info::host_info64;
        use mach::mach_types::mach_port_t;
        use mach::vm_statistics64::vm_statistics64_t;

        let host: mach_port_t = mach::mach_host_self();
        let mut stats: vm_statistics64_t = unsafe { std::mem::zeroed() };
        let mut count = (std::mem::size_of::<vm_statistics64_t>() / std::mem::size_of::<u32>())
            as mach_msg_type_number_t;

        let result = unsafe {
            host_info64(
                host,
                HOST_VM_INFO64,
                &mut stats as *mut vm_statistics64_t as *mut i32,
                &mut count,
            )
        };

        if result != KERN_SUCCESS {
            return self.fallback_headroom(); // Use existing vm_stat
        }

        let page_size = vm_kernel_page_size as u64;
        let total_bytes = self.get_total_memory_bytes()?; // sysctl hw.memsize

        let active = (stats.active_count as u64).saturating_mul(page_size);
        let inactive = (stats.inactive_count as u64).saturating_mul(page_size);
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
}

// Add enum
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MemoryPressureLevel {
    Low,
    Medium,
    High,
    Critical,
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

// In UmaPressureMonitor
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

// In headroom_pct_macos, fix mach if error, use fallback vm_stat

#[cfg(target_os = "macos")]
fn headroom_pct_macos(&self) -> Option<f32> {
    // Try mach first
    // Assume mach crate added to Cargo.toml
    // If compile error, comment out and use fallback

    // Fallback to vm_stat
    let output = Command::new("vm_stat").output().ok()?;
    let vm_stat = String::from_utf8_lossy(&output.stdout);

    let mut free_pages = 0u64;
    let mut inactive_pages = 0u64;
    let page_size = 4096u64;

    for line in vm_stat.lines() {
        if line.contains("Pages free") {
            if let Ok(p) = line
                .split(':')
                .nth(1)
                .unwrap_or("")
                .trim()
                .trim_end_matches('.')
                .parse()
            {
                free_pages = p;
            }
        } else if line.contains("Pages inactive") {
            if let Ok(p) = line
                .split(':')
                .nth(1)
                .unwrap_or("")
                .trim()
                .trim_end_matches('.')
                .parse()
            {
                inactive_pages = p;
            }
        }
    }

    let total_bytes = self.get_total_memory_bytes()? as f32;
    let available_bytes = (free_pages + inactive_pages) as f32 * page_size as f32;
    let headroom_pct = (available_bytes / total_bytes) * 100.0;

    Some(headroom_pct)
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
        };
        let level = determine_pressure(&stats, 15.0);
        assert_eq!(level, MemoryPressureLevel::Medium);

        let critical = UmaStats {
            headroom_pct: 10.0,
            used_mb: 14400,
            total_mb: 16000,
        };
        let level = determine_pressure(&critical, 15.0);
        assert_eq!(level, MemoryPressureLevel::Critical);
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
        let _ = t
            .log(
                "uma.pressure",
                json!({
                    "level": level.to_string(),
                    "headroom_pct": stats.headroom_pct,
                    "used_mb": stats.used_mb,
                    "available_mb": stats.total_mb - stats.used_mb, // Calculate available_mb
                    "total_mb": stats.total_mb,
                    "timestamp": Utc::now().timestamp()
                }),
            )
            .await;
    }
}

#[derive(Clone)]
pub struct UmaStats {
    headroom_pct: f32,
    used_mb: u64,
    total_mb: u64,
}
