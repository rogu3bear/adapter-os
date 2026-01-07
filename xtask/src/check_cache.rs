//! Check Cargo cache staleness to prevent disk bloat
//!
//! Warns when incremental compilation cache is older than threshold (default 48h)
//! or exceeds size threshold (default 50GB).

use anyhow::Result;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

const DEFAULT_MAX_AGE_HOURS: u64 = 48;
const DEFAULT_MAX_SIZE_GB: u64 = 50;
const WARN_SIZE_GB: u64 = 20;

pub fn run() -> Result<()> {
    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
    let cache_dir = Path::new(&target_dir).join("debug/incremental");

    if !cache_dir.exists() {
        return Ok(());
    }

    let max_age_hours: u64 = std::env::var("AOS_CACHE_MAX_AGE_HOURS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_AGE_HOURS);

    let mut warnings = Vec::new();

    // Check cache age
    if let Some(oldest_time) = find_oldest_file_time(&cache_dir) {
        let age = SystemTime::now()
            .duration_since(oldest_time)
            .unwrap_or(Duration::ZERO);
        let age_hours = age.as_secs() / 3600;

        if age_hours > max_age_hours {
            warnings.push(format!(
                "Cargo incremental cache is {}h old (threshold: {}h)",
                age_hours, max_age_hours
            ));
        }
    }

    // Check cache size
    if let Ok(size_bytes) = dir_size(&cache_dir) {
        let size_gb = size_bytes / (1024 * 1024 * 1024);

        if size_gb > DEFAULT_MAX_SIZE_GB {
            warnings.push(format!(
                "Incremental cache is {}GB - run 'cargo clean' to free space",
                size_gb
            ));
        } else if size_gb > WARN_SIZE_GB {
            warnings.push(format!("Incremental cache is {}GB", size_gb));
        }
    }

    // Print warnings
    if !warnings.is_empty() {
        eprintln!();
        eprintln!("\x1b[33m⚠️  CACHE WARNING:\x1b[0m");
        for warning in &warnings {
            eprintln!("   {}", warning);
        }
        eprintln!("   Run: rm -rf target/debug/incremental target/release/incremental");
        eprintln!();
    }

    Ok(())
}

fn find_oldest_file_time(dir: &Path) -> Option<SystemTime> {
    let mut oldest: Option<SystemTime> = None;

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(metadata) = fs::metadata(&path) {
                    if let Ok(modified) = metadata.modified() {
                        oldest = Some(match oldest {
                            Some(current) if modified < current => modified,
                            Some(current) => current,
                            None => modified,
                        });
                    }
                }
            } else if path.is_dir() {
                if let Some(sub_oldest) = find_oldest_file_time(&path) {
                    oldest = Some(match oldest {
                        Some(current) if sub_oldest < current => sub_oldest,
                        Some(current) => current,
                        None => sub_oldest,
                    });
                }
            }
        }
    }

    oldest
}

fn dir_size(dir: &Path) -> Result<u64> {
    let mut size = 0;

    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                size += fs::metadata(&path)?.len();
            } else if path.is_dir() {
                size += dir_size(&path)?;
            }
        }
    }

    Ok(size)
}
