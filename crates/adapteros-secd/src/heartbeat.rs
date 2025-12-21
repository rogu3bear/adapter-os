//! Heartbeat file management for aos-secd daemon

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::time;

/// Heartbeat manager
pub struct Heartbeat {
    path: PathBuf,
    start_time: Instant,
}

impl Heartbeat {
    /// Create a new heartbeat manager
    pub fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let start_time = Instant::now();

        Ok(Self { path, start_time })
    }

    /// Write current timestamp to heartbeat file
    pub fn update(&self) -> io::Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_millis();

        let mut file = fs::File::create(&self.path)?;
        write!(file, "{}", timestamp)?;
        file.sync_all()?;

        Ok(())
    }

    /// Get uptime since daemon start
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Spawn a background task that updates heartbeat every interval
    pub async fn spawn_updater(self: std::sync::Arc<Self>, interval: Duration) {
        let mut interval_timer = time::interval(interval);

        loop {
            interval_timer.tick().await;

            if let Err(e) = self.update() {
                tracing::error!("Failed to update heartbeat: {}", e);
            } else {
                tracing::debug!("Heartbeat updated");
            }
        }
    }

    /// Remove heartbeat file
    pub fn remove(&self) -> io::Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
            tracing::info!("Removed heartbeat file: {}", self.path.display());
        }
        Ok(())
    }
}

/// Read heartbeat timestamp from file
pub fn read_heartbeat(path: impl AsRef<Path>) -> io::Result<u128> {
    let content = fs::read_to_string(path)?;
    content
        .trim()
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Get time since last heartbeat in seconds
pub fn time_since_heartbeat(path: impl AsRef<Path>) -> io::Result<u64> {
    let last_heartbeat_ms = read_heartbeat(path)?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_millis();

    let elapsed_ms = now_ms.saturating_sub(last_heartbeat_ms);
    Ok((elapsed_ms / 1000) as u64)
}
