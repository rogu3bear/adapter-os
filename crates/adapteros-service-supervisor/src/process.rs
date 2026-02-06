//! Process management utilities for the service supervisor

use crate::error::{Result, SupervisorError};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tokio::time::Duration;
use tracing::{error, info, warn};

/// Process manager for handling service processes
pub struct ProcessManager {
    processes: Arc<RwLock<HashMap<String, Arc<ManagedProcess>>>>,
}

impl ProcessManager {
    /// Create a new process manager
    pub fn new() -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start a new process
    pub async fn start_process(
        &self,
        id: String,
        command: &str,
        args: &[String],
        working_dir: Option<&std::path::Path>,
        env: &HashMap<String, String>,
    ) -> Result<Arc<ManagedProcess>> {
        // Check if process already exists
        let mut processes = self.processes.write().await;
        if processes.contains_key(&id) {
            return Err(SupervisorError::ServiceOperation(format!(
                "Process {} already exists",
                id
            )));
        }

        let managed_process =
            Arc::new(ManagedProcess::new(id.clone(), command, args, working_dir, env).await?);
        processes.insert(id, Arc::clone(&managed_process));

        Ok(managed_process)
    }

    /// Stop a process
    pub async fn stop_process(&self, id: &str, timeout_secs: u64) -> Result<()> {
        let processes = self.processes.read().await;
        if let Some(process) = processes.get(id) {
            process.stop(Duration::from_secs(timeout_secs)).await?;
        }
        Ok(())
    }

    /// Get process status
    pub async fn get_process_status(&self, id: &str) -> Option<ProcessStatus> {
        let processes = self.processes.read().await;
        processes.get(id).map(|p| p.status())
    }

    /// List all processes
    pub async fn list_processes(&self) -> Vec<(String, ProcessStatus)> {
        let processes = self.processes.read().await;
        let mut result = Vec::new();

        for (id, process) in processes.iter() {
            result.push((id.clone(), process.status()));
        }

        result
    }

    /// Remove a process from management
    pub async fn remove_process(&self, id: &str) {
        let mut processes = self.processes.write().await;
        processes.remove(id);
    }
}

/// Managed process instance
pub struct ManagedProcess {
    id: String,
    child: RwLock<Option<Child>>,
    start_time: std::time::Instant,
}

impl ManagedProcess {
    /// Create a new managed process
    pub async fn new(
        id: String,
        command: &str,
        args: &[String],
        working_dir: Option<&std::path::Path>,
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        // Set environment variables
        for (key, value) in env {
            cmd.env(key, value);
        }

        let child = cmd.spawn().map_err(|e| {
            SupervisorError::Process(format!("Failed to spawn process {}: {}", id, e))
        })?;

        info!("Started process {} with PID {:?}", id, child.id());

        Ok(Self {
            id,
            child: RwLock::new(Some(child)),
            start_time: std::time::Instant::now(),
        })
    }

    /// Get process status
    pub fn status(&self) -> ProcessStatus {
        ProcessStatus {
            id: self.id.clone(),
            running: true, // We'll check this properly
            pid: None,     // We'll implement this
            start_time: self.start_time,
            uptime: self.start_time.elapsed(),
        }
    }

    /// Stop the process
    pub async fn stop(&self, timeout: Duration) -> Result<()> {
        let mut child_guard = self.child.write().await;
        if let Some(mut child) = child_guard.take() {
            // Try graceful shutdown first
            if let Some(pid) = child.id() {
                if let Err(e) = kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
                    error!(
                        process_id = %self.id,
                        pid = pid,
                        error = %e,
                        "Failed to send SIGTERM, attempting SIGKILL"
                    );
                    // SIGTERM failed, try SIGKILL immediately
                    if let Err(kill_err) = kill(Pid::from_raw(pid as i32), Signal::SIGKILL) {
                        error!(
                            process_id = %self.id,
                            pid = pid,
                            error = %kill_err,
                            "Failed to send SIGKILL - process may be orphaned"
                        );
                    }
                }
            }

            // Wait for process to exit
            match tokio::time::timeout(timeout, child.wait()).await {
                Ok(result) => match result {
                    Ok(status) => info!("Process {} exited with status: {}", self.id, status),
                    Err(e) => warn!("Error waiting for process {}: {}", self.id, e),
                },
                Err(_) => {
                    // Force kill if timeout
                    if let Some(pid) = child.id() {
                        if let Err(e) = kill(Pid::from_raw(pid as i32), Signal::SIGKILL) {
                            error!(
                                process_id = %self.id,
                                pid = pid,
                                error = %e,
                                "Failed to send SIGKILL after timeout - process may be orphaned"
                            );
                        } else {
                            warn!("Force killed process {} after timeout", self.id);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Send signal to process
    pub async fn signal(&self, signal: Signal) -> Result<()> {
        let child_guard = self.child.read().await;
        if let Some(child) = child_guard.as_ref() {
            if let Some(pid) = child.id() {
                kill(Pid::from_raw(pid as i32), signal).map_err(|e| {
                    SupervisorError::Process(format!("Failed to send signal to {}: {}", self.id, e))
                })?;
            }
        }
        Ok(())
    }
}

/// Process status information
#[derive(Debug, Clone)]
pub struct ProcessStatus {
    pub id: String,
    pub running: bool,
    pub pid: Option<u32>,
    pub start_time: std::time::Instant,
    pub uptime: std::time::Duration,
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}
