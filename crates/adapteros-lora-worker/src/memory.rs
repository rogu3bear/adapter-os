//! Memory monitoring and eviction

use adapteros_core::Result;

/// Memory monitor for enforcing headroom policy
pub struct MemoryMonitor {
    min_headroom_pct: u8,
}

impl MemoryMonitor {
    pub fn new(min_headroom_pct: u8) -> Self {
        Self { min_headroom_pct }
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

    #[cfg(target_os = "macos")]
    fn headroom_pct_macos(&self) -> Option<f32> {
        // Use sysctl to get memory info (simpler than mach APIs)
        use std::process::Command;

        let output = Command::new("sysctl")
            .args(&["-n", "hw.memsize"])
            .output()
            .ok()?;

        let total_bytes = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u64>()
            .ok()?;

        let output = Command::new("vm_stat").output().ok()?;

        let vm_stat = String::from_utf8_lossy(&output.stdout);

        // Parse free and inactive pages
        let mut free_pages = 0u64;
        let mut inactive_pages = 0u64;
        let page_size = 4096u64; // Standard page size on macOS

        for line in vm_stat.lines() {
            if line.contains("Pages free") {
                free_pages = line
                    .split(':')
                    .nth(1)?
                    .trim()
                    .trim_end_matches('.')
                    .parse()
                    .ok()?;
            } else if line.contains("Pages inactive") {
                inactive_pages = line
                    .split(':')
                    .nth(1)?
                    .trim()
                    .trim_end_matches('.')
                    .parse()
                    .ok()?;
            }
        }

        let available_bytes = (free_pages + inactive_pages) * page_size;
        let free_pct = (available_bytes as f32 / total_bytes as f32) * 100.0;

        Some(free_pct)
    }

    #[cfg(target_os = "linux")]
    fn headroom_pct_linux(&self) -> Option<f32> {
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
