//! Job alert watcher and notification system

use adapteros_db::{Db, Job};
use adapteros_deterministic_exec::{spawn_deterministic, DeterministicJoinHandle};
use adapteros_server::config::AlertingConfig;
use anyhow::Result;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::time::{interval, Duration};
use tracing::{error, info};
use uuid::Uuid;

/// Alert watcher that monitors for failed jobs and writes alerts
pub struct AlertWatcher {
    db: Db,
    config: AlertingConfig,
    current_file: Option<File>,
    current_file_path: Option<PathBuf>,
    current_count: usize,
    current_size: u64,
}

impl AlertWatcher {
    /// Create a new alert watcher
    pub fn new(db: Db, config: AlertingConfig) -> Self {
        Self {
            db,
            config,
            current_file: None,
            current_file_path: None,
            current_count: 0,
            current_size: 0,
        }
    }

    /// Start the alert watcher loop
    pub async fn start(mut self) -> Result<()> {
        info!("Starting alert watcher");

        // Ensure alert directory exists
        std::fs::create_dir_all(&self.config.alert_dir)?;

        let mut ticker = interval(Duration::from_secs(5));

        loop {
            ticker.tick().await;

            if let Err(e) = self.check_failed_jobs().await {
                error!("Alert watcher error: {}", e);
            }
        }
    }

    /// Check for new failed jobs
    async fn check_failed_jobs(&mut self) -> Result<()> {
        // Query jobs that failed and haven't been alerted yet
        let failed_jobs = sqlx::query_as::<_, Job>(
            "SELECT id, kind, tenant_id, user_id, payload_json, status, result_json, 
                    logs_path, created_at, started_at, finished_at 
             FROM jobs 
             WHERE status = 'failed' 
             AND id NOT IN (SELECT subject_id FROM alerts WHERE kind = 'job_failed' AND subject_id IS NOT NULL)"
        )
        .fetch_all(self.db.pool())
        .await?;

        if !failed_jobs.is_empty() {
            info!("Found {} failed jobs to alert", failed_jobs.len());
        }

        for job in failed_jobs {
            let message = format!(
                "Job {} ({}) failed. Tenant: {:?}, Result: {}",
                job.id,
                job.kind,
                job.tenant_id,
                job.result_json.as_deref().unwrap_or("no result")
            );

            if let Err(e) = self.emit_alert("job_failed", Some(&job.id), &message).await {
                error!("Failed to emit alert for job {}: {}", job.id, e);
            }
        }

        Ok(())
    }

    /// Emit an alert (write to database and log file)
    async fn emit_alert(
        &mut self,
        kind: &str,
        subject_id: Option<&str>,
        message: &str,
    ) -> Result<()> {
        let alert_id = Uuid::now_v7().to_string();

        // Write to database
        sqlx::query(
            "INSERT INTO alerts (id, severity, kind, subject_id, message, acknowledged) 
             VALUES (?, 'high', ?, ?, ?, 0)",
        )
        .bind(&alert_id)
        .bind(kind)
        .bind(subject_id)
        .bind(message)
        .execute(self.db.pool())
        .await?;

        // Write to log file
        self.write_to_log(&alert_id, kind, message).await?;

        info!("Alert emitted: {} - {}", kind, message);

        Ok(())
    }

    /// Write alert to rotating log file
    async fn write_to_log(&mut self, alert_id: &str, kind: &str, message: &str) -> Result<()> {
        // Check if we need to rotate
        if self.should_rotate() {
            self.rotate_log_file()?;
        }

        // Open file if not already open
        if self.current_file.is_none() {
            self.open_new_log_file()?;
        }

        // Write alert line
        if let Some(file) = &mut self.current_file {
            let timestamp = chrono::Utc::now().to_rfc3339();
            let line = format!("{}\t{}\t{}\t{}\n", timestamp, alert_id, kind, message);

            file.write_all(line.as_bytes())?;
            file.flush()?;

            self.current_count += 1;
            self.current_size += line.len() as u64;
        }

        Ok(())
    }

    /// Check if log file should be rotated
    fn should_rotate(&self) -> bool {
        if self.current_file.is_none() {
            return false;
        }

        self.current_count >= self.config.max_alerts_per_file
            || self.current_size >= self.config.rotate_size_mb * 1024 * 1024
    }

    /// Rotate to a new log file
    fn rotate_log_file(&mut self) -> Result<()> {
        if let Some(file) = self.current_file.take() {
            drop(file);
        }

        self.current_file = None;
        self.current_file_path = None;
        self.current_count = 0;
        self.current_size = 0;

        self.open_new_log_file()?;

        Ok(())
    }

    /// Open a new log file with timestamp
    fn open_new_log_file(&mut self) -> Result<()> {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("alerts_{}.log", timestamp);
        let filepath = Path::new(&self.config.alert_dir).join(&filename);

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&filepath)?;

        info!("Opened new alert log file: {}", filepath.display());

        self.current_file = Some(file);
        self.current_file_path = Some(filepath);
        self.current_count = 0;
        self.current_size = 0;

        Ok(())
    }
}

/// Helper function to spawn alert watcher as a background task
pub fn spawn_alert_watcher(
    db: Db,
    config: AlertingConfig,
) -> Result<DeterministicJoinHandle, adapteros_core::AosError> {
    spawn_deterministic("Alert watcher".to_string(), async move {
        let watcher = AlertWatcher::new(db, config);
        if let Err(e) = watcher.start().await {
            error!("Alert watcher crashed: {}", e);
        }
    })
    .map_err(|e| {
        adapteros_core::AosError::Internal(format!("Failed to spawn alert watcher: {}", e))
    })
}
