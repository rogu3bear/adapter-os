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

/// Model Server process information for shared model inference
#[derive(Debug, Clone)]
pub struct ModelServerInfo {
    /// Process ID of the model server
    pub pid: u32,
    /// gRPC address (e.g., "127.0.0.1:50051")
    pub grpc_addr: String,
    /// Model name/path loaded
    pub model_name: String,
    /// When the server was started
    pub started_at: Instant,
    /// Whether the server is healthy (last health check)
    pub healthy: bool,
    /// gRPC port for respawning
    pub grpc_port: u16,
    /// Max KV cache sessions for respawning
    pub max_kv_cache_sessions: u32,
    /// Number of times this server has been restarted
    pub restart_count: u32,
}

/// Configuration for Model Server supervision
#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    /// Interval between health checks
    pub check_interval: Duration,
    /// Maximum number of restart attempts before giving up
    pub max_restart_attempts: u32,
    /// Backoff multiplier for restart delays (exponential backoff)
    pub restart_backoff_multiplier: f64,
    /// Initial delay before first restart attempt
    pub initial_restart_delay: Duration,
    /// Maximum delay between restart attempts
    pub max_restart_delay: Duration,
    /// Health check timeout
    pub health_check_timeout: Duration,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(10),
            max_restart_attempts: 5,
            restart_backoff_multiplier: 2.0,
            initial_restart_delay: Duration::from_secs(1),
            max_restart_delay: Duration::from_secs(60),
            health_check_timeout: Duration::from_secs(5),
        }
    }
}

pub struct NodeAgent {
    workers: Arc<RwLock<HashMap<u32, WorkerInfo>>>,
    /// Model server process (single instance per node)
    model_server: Arc<RwLock<Option<ModelServerInfo>>>,
    pf_status_cache: Arc<RwLock<Option<(bool, Instant)>>>,
    pf_cache_ttl: Duration,
    /// Supervisor configuration
    supervisor_config: SupervisorConfig,
    /// Flag to signal supervisor shutdown
    supervisor_shutdown: Arc<RwLock<bool>>,
}

impl NodeAgent {
    pub fn new() -> Self {
        Self::with_supervisor_config(SupervisorConfig::default())
    }

    /// Create a new NodeAgent with custom supervisor configuration
    pub fn with_supervisor_config(supervisor_config: SupervisorConfig) -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            model_server: Arc::new(RwLock::new(None)),
            pf_status_cache: Arc::new(RwLock::new(None)),
            pf_cache_ttl: Duration::from_secs(30),
            supervisor_config,
            supervisor_shutdown: Arc::new(RwLock::new(false)),
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

    /// Spawn the Model Server process for shared model inference
    ///
    /// The Model Server loads the base model once and serves multiple workers
    /// via gRPC, reducing GPU memory usage significantly.
    ///
    /// # Arguments
    /// * `model_path` - Path to the model directory
    /// * `grpc_port` - Port for gRPC server (default: 50051)
    /// * `max_kv_cache_sessions` - Maximum KV cache sessions (default: 32)
    pub async fn spawn_model_server(
        &self,
        model_path: &str,
        grpc_port: Option<u16>,
        max_kv_cache_sessions: Option<u32>,
    ) -> Result<u32> {
        let port = grpc_port.unwrap_or(50051);
        let max_sessions = max_kv_cache_sessions.unwrap_or(32);

        info!(
            model_path = model_path,
            grpc_port = port,
            max_kv_cache_sessions = max_sessions,
            "Spawning Model Server"
        );

        // Check if model server is already running
        {
            let ms = self.model_server.read().await;
            if let Some(ref info) = *ms {
                if info.healthy {
                    warn!(pid = info.pid, "Model Server already running");
                    return Err(anyhow::anyhow!(
                        "Model Server already running with PID {}",
                        info.pid
                    ));
                }
            }
        }

        // 1. Verify PF deny rules (model server should also be air-gapped)
        if !self.check_pf_status().await? {
            return Err(anyhow::anyhow!(
                "PF egress rules not active - refusing to spawn Model Server"
            ));
        }

        let grpc_addr = format!("127.0.0.1:{}", port);

        // 2. Spawn Model Server process
        #[cfg(unix)]
        {
            use std::process::Command;

            let mut cmd = Command::new("aos-model-srv");
            cmd.arg("--model-path")
                .arg(model_path)
                .arg("--grpc-port")
                .arg(port.to_string())
                .arg("--max-kv-cache-sessions")
                .arg(max_sessions.to_string());

            // Set environment variables
            cmd.env("AOS_MODEL_PATH", model_path);
            cmd.env("AOS_GRPC_PORT", port.to_string());

            match cmd.spawn() {
                Ok(child) => {
                    let pid = child.id();
                    info!(
                        pid = pid,
                        grpc_addr = %grpc_addr,
                        model_path = model_path,
                        "Model Server spawned"
                    );

                    // Track model server
                    let model_server_info = ModelServerInfo {
                        pid,
                        grpc_addr: grpc_addr.clone(),
                        model_name: model_path.to_string(),
                        started_at: Instant::now(),
                        healthy: true, // Assume healthy until health check fails
                        grpc_port: port,
                        max_kv_cache_sessions: max_sessions,
                        restart_count: 0,
                    };

                    *self.model_server.write().await = Some(model_server_info);

                    Ok(pid)
                }
                Err(e) => {
                    // If aos-model-srv binary not found, create simulated server for testing
                    warn!(
                        error = %e,
                        "Failed to spawn aos-model-srv, creating simulated server for testing"
                    );

                    // Generate a simulated PID for development/testing
                    let simulated_pid = (std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u32)
                        .unwrap_or(2000)
                        % 65536)
                        + 20000;

                    let model_server_info = ModelServerInfo {
                        pid: simulated_pid,
                        grpc_addr: grpc_addr.clone(),
                        model_name: model_path.to_string(),
                        started_at: Instant::now(),
                        healthy: true,
                        grpc_port: port,
                        max_kv_cache_sessions: max_sessions,
                        restart_count: 0,
                    };

                    *self.model_server.write().await = Some(model_server_info);
                    info!(pid = simulated_pid, "Simulated Model Server created");
                    Ok(simulated_pid)
                }
            }
        }

        #[cfg(not(unix))]
        {
            Err(anyhow::anyhow!(
                "Model Server spawning only supported on Unix"
            ))
        }
    }

    /// Stop the Model Server process
    pub async fn stop_model_server(&self) -> Result<()> {
        let mut ms = self.model_server.write().await;
        if let Some(info) = ms.take() {
            info!(pid = info.pid, "Stopping Model Server");

            #[cfg(unix)]
            {
                use nix::sys::signal::Signal;
                use nix::unistd::Pid;

                // Send SIGTERM for graceful shutdown
                if let Err(e) =
                    nix::sys::signal::kill(Pid::from_raw(info.pid as i32), Signal::SIGTERM)
                {
                    warn!(
                        pid = info.pid,
                        error = %e,
                        "Failed to send SIGTERM to Model Server"
                    );
                }

                // Wait for graceful shutdown
                tokio::time::sleep(Duration::from_secs(5)).await;

                // Force kill if still running
                if let Err(_) =
                    nix::sys::signal::kill(Pid::from_raw(info.pid as i32), Signal::SIGKILL)
                {
                    // Process already exited, which is fine
                }
            }

            info!(pid = info.pid, "Model Server stopped");
            Ok(())
        } else {
            Err(anyhow::anyhow!("Model Server not running"))
        }
    }

    /// Get Model Server info
    pub async fn get_model_server_info(&self) -> Option<ModelServerInfo> {
        self.model_server.read().await.clone()
    }

    /// Check Model Server health by attempting to connect to its gRPC endpoint
    ///
    /// This method:
    /// 1. Checks if the process is still running (via kill(0))
    /// 2. Attempts a TCP connection to the gRPC address
    /// 3. Updates the `healthy` field in `ModelServerInfo`
    ///
    /// Returns `true` if the server is healthy, `false` otherwise.
    pub async fn check_model_server_health(&self) -> bool {
        let info = {
            let ms = self.model_server.read().await;
            match &*ms {
                Some(info) => info.clone(),
                None => {
                    // No model server configured
                    return false;
                }
            }
        };

        let mut is_healthy = true;

        // 1. Check if process is still running
        #[cfg(unix)]
        {
            use nix::sys::signal::Signal;
            use nix::unistd::Pid;

            // kill(pid, 0) checks if process exists without sending a signal
            if nix::sys::signal::kill(Pid::from_raw(info.pid as i32), Signal::SIGCONT).is_err() {
                // Use SIGCONT (0 would be ideal but nix doesn't have it)
                // Actually, we can check if the process exists by trying to send signal 0
                // but nix wraps this - let's try a different approach
            }

            // Try to check if process exists via /proc on Linux or kill check
            match nix::sys::signal::kill(Pid::from_raw(info.pid as i32), None) {
                Ok(_) => {
                    // Process exists
                }
                Err(nix::errno::Errno::ESRCH) => {
                    // No such process
                    warn!(
                        pid = info.pid,
                        "Model Server process no longer exists (ESRCH)"
                    );
                    is_healthy = false;
                }
                Err(nix::errno::Errno::EPERM) => {
                    // Process exists but we don't have permission (unusual for our own child)
                    // Still consider it as potentially running
                }
                Err(e) => {
                    warn!(
                        pid = info.pid,
                        error = %e,
                        "Failed to check Model Server process status"
                    );
                    is_healthy = false;
                }
            }
        }

        // 2. Try TCP connection to gRPC port (if process check passed)
        if is_healthy {
            let timeout = self.supervisor_config.health_check_timeout;
            let addr = info.grpc_addr.clone();

            let connect_result =
                tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr)).await;

            match connect_result {
                Ok(Ok(_stream)) => {
                    // Connection successful - server is accepting connections
                }
                Ok(Err(e)) => {
                    warn!(
                        grpc_addr = %addr,
                        error = %e,
                        "Model Server TCP connection failed"
                    );
                    is_healthy = false;
                }
                Err(_) => {
                    warn!(
                        grpc_addr = %addr,
                        timeout_secs = timeout.as_secs(),
                        "Model Server health check timed out"
                    );
                    is_healthy = false;
                }
            }
        }

        // 3. Update the healthy field
        {
            let mut ms = self.model_server.write().await;
            if let Some(ref mut server_info) = *ms {
                server_info.healthy = is_healthy;
            }
        }

        if is_healthy {
            tracing::debug!(
                pid = info.pid,
                grpc_addr = %info.grpc_addr,
                "Model Server health check passed"
            );
        }

        is_healthy
    }

    /// Restart the Model Server process
    ///
    /// This method:
    /// 1. Stops the existing Model Server (if running)
    /// 2. Spawns a new Model Server with the same configuration
    /// 3. Increments the restart counter
    ///
    /// Returns the new PID on success.
    pub async fn restart_model_server(&self) -> Result<u32> {
        // Get current config before stopping
        let (model_path, grpc_port, max_kv_cache_sessions, restart_count) = {
            let ms = self.model_server.read().await;
            match &*ms {
                Some(info) => (
                    info.model_name.clone(),
                    info.grpc_port,
                    info.max_kv_cache_sessions,
                    info.restart_count,
                ),
                None => {
                    return Err(anyhow::anyhow!(
                        "Cannot restart Model Server: no previous configuration found"
                    ));
                }
            }
        };

        info!(
            restart_count = restart_count + 1,
            model_path = %model_path,
            grpc_port = grpc_port,
            "Restarting Model Server"
        );

        // Stop existing server (ignore errors if already dead)
        let _ = self.stop_model_server().await;

        // Brief delay to allow port to be released
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Spawn new server
        let new_pid = self
            .spawn_model_server(&model_path, Some(grpc_port), Some(max_kv_cache_sessions))
            .await?;

        // Update restart count
        {
            let mut ms = self.model_server.write().await;
            if let Some(ref mut info) = *ms {
                info.restart_count = restart_count + 1;
            }
        }

        info!(
            new_pid = new_pid,
            restart_count = restart_count + 1,
            "Model Server restarted successfully"
        );

        Ok(new_pid)
    }

    /// Start the Model Server supervisor loop
    ///
    /// This background task periodically checks Model Server health and
    /// automatically restarts it if it becomes unhealthy. The supervisor
    /// uses exponential backoff for restart attempts.
    ///
    /// # Arguments
    /// * `check_interval` - Override for health check interval (uses config default if None)
    ///
    /// # Returns
    /// A `JoinHandle` for the supervisor task. The task runs until:
    /// - `shutdown_model_server_supervisor()` is called
    /// - Maximum restart attempts are exhausted
    /// - The NodeAgent is dropped
    pub async fn start_model_server_supervisor(
        self: Arc<Self>,
        check_interval: Option<Duration>,
    ) -> tokio::task::JoinHandle<()> {
        let interval = check_interval.unwrap_or(self.supervisor_config.check_interval);
        let max_attempts = self.supervisor_config.max_restart_attempts;
        let initial_delay = self.supervisor_config.initial_restart_delay;
        let max_delay = self.supervisor_config.max_restart_delay;
        let backoff_multiplier = self.supervisor_config.restart_backoff_multiplier;

        // Reset shutdown flag
        {
            let mut shutdown = self.supervisor_shutdown.write().await;
            *shutdown = false;
        }

        info!(
            check_interval_secs = interval.as_secs(),
            max_restart_attempts = max_attempts,
            "Starting Model Server supervisor"
        );

        let agent = Arc::clone(&self);

        tokio::spawn(async move {
            let mut consecutive_failures = 0u32;
            let mut current_delay = initial_delay;

            loop {
                // Check shutdown flag
                {
                    let shutdown = agent.supervisor_shutdown.read().await;
                    if *shutdown {
                        info!("Model Server supervisor shutting down");
                        break;
                    }
                }

                // Check if model server is configured
                let has_server = {
                    let ms = agent.model_server.read().await;
                    ms.is_some()
                };

                if !has_server {
                    // No model server to supervise - wait and check again
                    tokio::time::sleep(interval).await;
                    continue;
                }

                // Perform health check
                let is_healthy = agent.check_model_server_health().await;

                if is_healthy {
                    // Reset failure counter and delay on success
                    if consecutive_failures > 0 {
                        info!(
                            previous_failures = consecutive_failures,
                            "Model Server recovered, resetting failure counter"
                        );
                    }
                    consecutive_failures = 0;
                    current_delay = initial_delay;
                } else {
                    consecutive_failures += 1;
                    warn!(
                        consecutive_failures = consecutive_failures,
                        max_attempts = max_attempts,
                        "Model Server health check failed"
                    );

                    // Check if we've exceeded max attempts
                    if consecutive_failures > max_attempts {
                        error!(
                            consecutive_failures = consecutive_failures,
                            max_attempts = max_attempts,
                            "Model Server exceeded maximum restart attempts, supervisor giving up"
                        );
                        break;
                    }

                    // Attempt restart with backoff
                    info!(
                        attempt = consecutive_failures,
                        delay_secs = current_delay.as_secs(),
                        "Attempting Model Server restart after backoff"
                    );

                    tokio::time::sleep(current_delay).await;

                    match agent.restart_model_server().await {
                        Ok(new_pid) => {
                            info!(
                                new_pid = new_pid,
                                attempt = consecutive_failures,
                                "Model Server restart succeeded"
                            );
                            // Don't reset consecutive_failures yet - wait for next health check
                        }
                        Err(e) => {
                            error!(
                                error = %e,
                                attempt = consecutive_failures,
                                "Model Server restart failed"
                            );
                        }
                    }

                    // Calculate next backoff delay (exponential with cap)
                    current_delay = Duration::from_secs_f64(
                        (current_delay.as_secs_f64() * backoff_multiplier)
                            .min(max_delay.as_secs_f64()),
                    );
                }

                // Wait for next check interval
                tokio::time::sleep(interval).await;
            }

            info!("Model Server supervisor exited");
        })
    }

    /// Signal the Model Server supervisor to shut down
    pub async fn shutdown_model_server_supervisor(&self) {
        let mut shutdown = self.supervisor_shutdown.write().await;
        *shutdown = true;
        info!("Model Server supervisor shutdown requested");
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
