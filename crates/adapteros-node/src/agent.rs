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
use nix::unistd::{close, Gid, Uid};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(unix)]
use std::os::unix::net::UnixStream;

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
        model_cache_max_mb: Option<u64>,
        config_toml_path: Option<&str>,
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

            // Set environment variables for worker configuration
            cmd.env("TENANT_ID", tenant_id);
            cmd.env("PLAN_ID", plan_id);
            cmd.env("UDS_PATH", &uds_path);

            // Propagate model cache budget to worker (required for model loading)
            if let Some(cache_mb) = model_cache_max_mb {
                cmd.env("AOS_MODEL_CACHE_MAX_MB", cache_mb.to_string());
                info!(
                    model_cache_max_mb = cache_mb,
                    "Propagating model cache budget to worker"
                );
            }

            // Propagate config TOML path if specified
            if let Some(config_path) = config_toml_path {
                cmd.env("AOS_CONFIG_TOML", config_path);
                info!(
                    config_toml_path = config_path,
                    "Propagating config TOML path to worker"
                );
            }

            // Set uid/gid for multi-tenant process isolation
            // This requires the parent process to have CAP_SETUID/CAP_SETGID or run as root
            let target_uid = uid;
            let target_gid = gid;

            // Use pre_exec to set gid before uid (order matters for privilege drop)
            // SAFETY: pre_exec runs after fork but before exec in a single-threaded context
            let (warning_read, warning_write) =
                UnixStream::pair().context("Failed to create pre-exec warning pipe")?;
            let mut can_read_warnings = true;

            if let Err(err) = warning_read.set_nonblocking(true) {
                warn!(
                    error = %err,
                    tenant_id = tenant_id,
                    plan_id = plan_id,
                    "Failed to set warning pipe to non-blocking"
                );
                can_read_warnings = false;
            }

            let warning_read_fd = warning_read.as_raw_fd();
            let warning_write_fd = warning_write.as_raw_fd();

            // SAFETY: pre_exec is called between fork() and exec() in the child process.
            // The closure captures file descriptors and UID/GID values by value (not references).
            // All operations inside (close, setgroups, setgid, setuid) are async-signal-safe
            // per POSIX requirements for this context. The file descriptors are valid at
            // capture time and remain valid in the child's address space.
            unsafe {
                cmd.pre_exec(move || {
                    let _ = close(warning_read_fd);

                    // Set supplementary groups to empty (drop all supplementary group privileges)
                    #[cfg(target_os = "linux")]
                    {
                        if nix::unistd::setgroups(&[]).is_err() {
                            let _ = nix::unistd::write(
                                warning_write_fd,
                                b"Warning: Failed to clear supplementary groups\n",
                            );
                        }
                    }

                    // Set GID first (must be done before dropping root via setuid)
                    if let Err(e) = nix::unistd::setgid(Gid::from_raw(target_gid)) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            format!("setgid({}) failed: {}", target_gid, e),
                        ));
                    }

                    // Set UID last (this drops root privileges)
                    if let Err(e) = nix::unistd::setuid(Uid::from_raw(target_uid)) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            format!("setuid({}) failed: {}", target_uid, e),
                        ));
                    }

                    let _ = close(warning_write_fd);
                    Ok(())
                });
            }

            // Spawn the worker process with isolation applied
            let child = match cmd.spawn() {
                Ok(child) => {
                    drop(warning_write);
                    if can_read_warnings {
                        drain_pre_exec_warnings(warning_read, tenant_id, plan_id);
                    } else {
                        drop(warning_read);
                    }
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
                        drop(warning_write);
                        if can_read_warnings {
                            drain_pre_exec_warnings(warning_read, tenant_id, plan_id);
                        } else {
                            drop(warning_read);
                        }
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
                    drop(warning_write);
                    if can_read_warnings {
                        drain_pre_exec_warnings(warning_read, tenant_id, plan_id);
                    } else {
                        drop(warning_read);
                    }
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

#[cfg(unix)]
fn drain_pre_exec_warnings(mut warning_read: UnixStream, tenant_id: &str, plan_id: &str) {
    use std::io::Read;

    let mut buffer = [0u8; 512];
    let mut output = Vec::new();

    for _ in 0..3 {
        loop {
            match warning_read.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => output.extend_from_slice(&buffer[..n]),
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::WouldBlock {
                        break;
                    }
                    warn!(
                        error = %err,
                        tenant_id = tenant_id,
                        plan_id = plan_id,
                        "Failed to read pre-exec warning pipe"
                    );
                    break;
                }
            }
        }

        if !output.is_empty() {
            break;
        }

        std::thread::sleep(Duration::from_millis(2));
    }

    if !output.is_empty() {
        let message = String::from_utf8_lossy(&output);
        let trimmed = message.trim();
        if !trimmed.is_empty() {
            warn!(
                tenant_id = tenant_id,
                plan_id = plan_id,
                warning = %trimmed,
                "Worker pre-exec warning"
            );
        }
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
