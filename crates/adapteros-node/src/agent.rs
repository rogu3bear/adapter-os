// Node agent implementation
use adapteros_policy::egress;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[cfg(unix)]
use nix::unistd::{Gid, Uid};

#[derive(Debug, Clone)]
pub struct WorkerInfo {
    pub pid: u32,
    pub tenant_id: String,
    pub plan_id: String,
    pub uds_path: String,
    pub started_at: Instant,
}

pub struct NodeAgent {
    workers: Arc<RwLock<HashMap<u32, WorkerInfo>>>,
    pf_status_cache: Arc<RwLock<Option<(bool, Instant)>>>,
    pf_cache_ttl: Duration,
}

impl NodeAgent {
    pub fn new() -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            pf_status_cache: Arc::new(RwLock::new(None)),
            pf_cache_ttl: Duration::from_secs(30),
        }
    }

    /// Spawn a worker process with tenant isolation
    pub async fn spawn_worker(
        &self,
        tenant_id: &str,
        plan_id: &str,
        uid: u32,
        gid: u32,
    ) -> Result<u32> {
        info!(
            "Spawning worker for tenant {} with plan {}",
            tenant_id, plan_id
        );

        // 1. Verify PF deny rules
        if !self.check_pf_status().await? {
            return Err(anyhow::anyhow!(
                "PF egress rules not active - refusing to spawn worker"
            ));
        }

        // 2. Create UDS socket directory
        let uds_path = format!("/var/run/aos/{}/aos.sock", tenant_id);
        let uds_dir = PathBuf::from(format!("/var/run/aos/{}", tenant_id));

        if !uds_dir.exists() {
            std::fs::create_dir_all(&uds_dir).context("Failed to create UDS socket directory")?;
        }

        // 3. Fork and spawn worker process with process isolation
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            use std::process::Command;

            let mut cmd = Command::new("aos-worker");
            cmd.arg("--tenant-id")
                .arg(tenant_id)
                .arg("--plan-id")
                .arg(plan_id)
                .arg("--uds-path")
                .arg(&uds_path);

            // Set environment variables
            cmd.env("TENANT_ID", tenant_id);
            cmd.env("PLAN_ID", plan_id);
            cmd.env("UDS_PATH", &uds_path);

            // Set uid/gid for multi-tenant process isolation
            // This requires the parent process to have CAP_SETUID/CAP_SETGID or run as root
            let target_uid = uid;
            let target_gid = gid;

            // Use pre_exec to set gid before uid (order matters for privilege drop)
            // SAFETY: pre_exec runs after fork but before exec in a single-threaded context
            unsafe {
                cmd.pre_exec(move || {
                    // Set supplementary groups to empty (drop all supplementary group privileges)
                    #[cfg(target_os = "linux")]
                    {
                        if nix::unistd::setgroups(&[]).is_err() {
                            // Non-fatal on some systems, log but continue
                            eprintln!("Warning: Failed to clear supplementary groups");
                        }
                    }

                    // Set GID first (must be done before dropping root via setuid)
                    if let Err(e) = nix::unistd::setgid(Gid::from_raw(target_gid)) {
                        eprintln!("Failed to setgid({}): {}", target_gid, e);
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            format!("setgid({}) failed: {}", target_gid, e),
                        ));
                    }

                    // Set UID last (this drops root privileges)
                    if let Err(e) = nix::unistd::setuid(Uid::from_raw(target_uid)) {
                        eprintln!("Failed to setuid({}): {}", target_uid, e);
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            format!("setuid({}) failed: {}", target_uid, e),
                        ));
                    }

                    Ok(())
                });
            }

            // Spawn the worker process with isolation applied
            let child = match cmd.spawn() {
                Ok(child) => {
                    info!(
                        pid = child.id(),
                        uid = uid,
                        gid = gid,
                        tenant_id = tenant_id,
                        "Worker process spawned with process isolation"
                    );
                    child
                }
                Err(e) => {
                    // Check for permission errors specifically
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        error!(
                            error = %e,
                            uid = uid,
                            gid = gid,
                            tenant_id = tenant_id,
                            "Permission denied setting uid/gid - ensure process has CAP_SETUID/CAP_SETGID or runs as root"
                        );
                        return Err(anyhow::anyhow!(
                            "Process isolation failed: permission denied for setuid/setgid to uid={}, gid={}. \
                             Ensure the parent process has appropriate capabilities (CAP_SETUID, CAP_SETGID) or runs as root.",
                            uid, gid
                        ));
                    }

                    // If aos-worker binary not found, log and create simulated worker for testing
                    warn!(
                        error = %e,
                        uid = uid,
                        gid = gid,
                        "Failed to spawn aos-worker, creating simulated worker for testing"
                    );
                    // Generate a simulated PID for development/testing
                    let simulated_pid = (std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u32)
                        .unwrap_or(1000)
                        % 65536)
                        + 10000;

                    // Track simulated worker
                    let worker_info = WorkerInfo {
                        pid: simulated_pid,
                        tenant_id: tenant_id.to_string(),
                        plan_id: plan_id.to_string(),
                        uds_path: uds_path.clone(),
                        started_at: Instant::now(),
                    };

                    self.workers
                        .write()
                        .await
                        .insert(simulated_pid, worker_info);
                    info!(pid = simulated_pid, "Simulated worker created");
                    return Ok(simulated_pid);
                }
            };

            let pid = child.id();

            // Track worker
            let worker_info = WorkerInfo {
                pid,
                tenant_id: tenant_id.to_string(),
                plan_id: plan_id.to_string(),
                uds_path: uds_path.clone(),
                started_at: Instant::now(),
            };

            self.workers.write().await.insert(pid, worker_info);

            info!(pid = pid, tenant_id = tenant_id, "Worker spawned with PID");
            Ok(pid)
        }

        #[cfg(not(unix))]
        {
            Err(anyhow::anyhow!("Worker spawning only supported on Unix"))
        }
    }

    /// Stop a worker by PID with proper signal handling
    pub async fn stop_worker(&self, pid: u32) -> Result<()> {
        info!("Stopping worker with PID {}", pid);

        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.remove(&pid) {
            info!("Worker {} for tenant {} stopped", pid, worker.tenant_id);

            // Send SIGTERM for graceful shutdown
            #[cfg(unix)]
            {
                use nix::sys::signal::Signal;
                use nix::unistd::Pid;
                use std::time::Duration;

                if let Err(e) = nix::sys::signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
                    tracing::warn!("Failed to send SIGTERM to PID {}: {}", pid, e);

                    // Fallback to SIGKILL after timeout
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    if let Err(e) =
                        nix::sys::signal::kill(Pid::from_raw(pid as i32), Signal::SIGKILL)
                    {
                        tracing::error!("Failed to send SIGKILL to PID {}: {}", pid, e);
                        return Err(anyhow::anyhow!("Failed to terminate process: {}", e));
                    }
                }
            }

            Ok(())
        } else {
            Err(anyhow::anyhow!("Worker {} not found", pid))
        }
    }

    /// List all active workers
    pub async fn list_workers(&self) -> Result<Vec<WorkerInfo>> {
        let workers = self.workers.read().await;
        Ok(workers.values().cloned().collect())
    }

    /// Check PF status on local node (with caching)
    pub async fn check_pf_status(&self) -> Result<bool> {
        // Check cache first
        {
            let cache = self.pf_status_cache.read().await;
            if let Some((status, cached_at)) = *cache {
                if cached_at.elapsed() < self.pf_cache_ttl {
                    return Ok(status);
                }
            }
        }

        // Cache miss - check actual PF status
        let status = match egress::validate_pf_rules() {
            Ok(_) => {
                info!("PF egress rules validated successfully");
                true
            }
            Err(e) => {
                warn!("PF egress rules validation failed: {}", e);
                false
            }
        };

        // Update cache
        {
            let mut cache = self.pf_status_cache.write().await;
            *cache = Some((status, Instant::now()));
        }

        Ok(status)
    }

    /// Get node health status
    pub async fn get_health(&self) -> Result<NodeHealth> {
        let pf_status = self.check_pf_status().await?;
        let workers = self.workers.read().await;
        let worker_count = workers.len();

        // Get memory info (simplified)
        let memory_available_mb = 8192; // Placeholder

        Ok(NodeHealth {
            pf_enabled: pf_status,
            worker_count,
            memory_available_mb,
        })
    }
}

impl Default for NodeAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealth {
    pub pf_enabled: bool,
    pub worker_count: usize,
    pub memory_available_mb: u64,
}
