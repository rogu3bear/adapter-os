//! aos-secd status command

use adapteros_secd::{is_process_running, read_pid};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Display aos-secd daemon status
pub async fn run(
    pid_file: &Path,
    heartbeat_file: &Path,
    socket_path: &Path,
    db_path: Option<&Path>,
) -> Result<()> {
    println!("aos-secd Status");
    println!("═══════════════");
    println!();

    // Check PID
    match read_pid(pid_file) {
        Ok(pid) => {
            let running = is_process_running(pid);
            if running {
                println!("✓ Status:        running (pid {})", pid);

                // Get uptime if we can read the PID file creation time
                if let Ok(metadata) = std::fs::metadata(pid_file) {
                    if let Ok(created) = metadata.created().or_else(|_| metadata.modified()) {
                        if let Ok(elapsed) = created.elapsed() {
                            let uptime = format_duration(elapsed);
                            println!("  Uptime:        {}", uptime);
                        }
                    }
                }
            } else {
                println!("✗ Status:        not running (stale pid {})", pid);
            }
        }
        Err(_) => {
            println!("✗ Status:        not running");
        }
    }

    // Check socket
    if socket_path.exists() {
        println!("✓ Socket:        {}", socket_path.display());
    } else {
        println!("✗ Socket:        {} (not found)", socket_path.display());
    }

    // Check heartbeat
    match std::fs::read_to_string(heartbeat_file) {
        Ok(content) => {
            if let Ok(last_heartbeat_ms) = content.trim().parse::<u128>() {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("System time before UNIX epoch")
                    .as_millis();

                let elapsed_ms = now_ms.saturating_sub(last_heartbeat_ms);
                let elapsed_secs = (elapsed_ms / 1000) as u64;

                if elapsed_secs < 30 {
                    println!("✓ Heartbeat:     {} ago", format_seconds(elapsed_secs));
                } else {
                    println!(
                        "⚠ Heartbeat:     {} ago (stale)",
                        format_seconds(elapsed_secs)
                    );
                }
            } else {
                println!("✗ Heartbeat:     invalid format");
            }
        }
        Err(_) => {
            println!("✗ Heartbeat:     not available");
        }
    }

    // Check database and query stats
    if let Some(db_path) = db_path {
        // Calculate current time once for all age calculations
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before UNIX epoch");
        let now_secs = now.as_secs() as i64;

        match adapteros_db::Db::connect(
            db_path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
        )
        .await
        {
            Ok(db) => {
                // Get operation count
                match db.count_enclave_operations().await {
                    Ok(count) => {
                        println!("✓ Operations:    {} logged", count);
                    }
                    Err(e) => {
                        println!("⚠ Operations:    error querying ({})", e);
                    }
                }

                // Get key ages
                match db.list_all_keys().await {
                    Ok(keys) => {
                        if keys.is_empty() {
                            println!("  Keys:          no keys tracked");
                        } else {
                            let max_age = keys
                                .iter()
                                .map(|k| (now_secs - k.created_at) / 86400)
                                .max()
                                .unwrap_or(0);

                            if max_age > 90 {
                                println!("⚠ Keys:          {} tracked (oldest {} days - exceeds 90-day threshold)", keys.len(), max_age);
                            } else {
                                println!(
                                    "✓ Keys:          {} tracked (oldest {} days)",
                                    keys.len(),
                                    max_age
                                );
                            }
                        }
                    }
                    Err(e) => {
                        println!("⚠ Keys:          error querying ({})", e);
                    }
                }

                // Check for warnings
                match db.list_old_keys(90).await {
                    Ok(old_keys) => {
                        if old_keys.is_empty() {
                            println!("✓ Warnings:      none");
                        } else {
                            println!(
                                "⚠ Warnings:      {} key(s) exceed 90-day threshold",
                                old_keys.len()
                            );
                            for key in old_keys {
                                let age_days = (now_secs - key.created_at) / 86400;
                                println!(
                                    "                 - {} ({} days old)",
                                    key.key_label, age_days
                                );
                            }
                        }
                    }
                    Err(e) => {
                        println!("⚠ Warnings:      error checking ({})", e);
                    }
                }
            }
            Err(e) => {
                println!("✗ Database:      not available ({})", e);
            }
        }
    } else {
        println!("  Database:      not configured");
    }

    println!();
    Ok(())
}

fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

fn format_seconds(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}
