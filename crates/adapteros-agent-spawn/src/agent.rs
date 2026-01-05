//! Agent handle for subprocess management
//!
//! Provides `AgentHandle` which wraps a spawned agent child process and
//! manages communication over Unix Domain Sockets.

use crate::config::AgentSpawnConfig;
use crate::error::{AgentSpawnError, Result};
use crate::protocol::{AgentRequest, AgentResponse, AgentState, HandshakeResponse};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Handle to a spawned agent process
///
/// Manages the lifecycle of an agent subprocess and provides communication
/// via Unix Domain Sockets.
pub struct AgentHandle {
    /// Unique agent ID (e.g., "agent-00")
    pub id: String,

    /// Process ID of the spawned agent
    pub pid: u32,

    /// Path to the agent's UDS socket
    pub socket_path: PathBuf,

    /// Path to the agent's PID file
    pub pid_file: PathBuf,

    /// Child process handle
    child: Mutex<Option<Child>>,

    /// Current agent state
    state: Arc<AtomicU8>,

    /// UDS stream for communication (established after handshake)
    stream: Mutex<Option<UnixStream>>,

    /// Configuration
    config: AgentSpawnConfig,

    /// Start time
    started_at: std::time::Instant,
}

impl AgentHandle {
    /// Spawn a new agent process
    ///
    /// This:
    /// 1. Creates the UDS socket path
    /// 2. Spawns the agent subprocess
    /// 3. Waits for the agent to connect and complete handshake
    pub async fn spawn(agent_id: String, config: &AgentSpawnConfig) -> Result<Self> {
        info!(agent_id = %agent_id, "Spawning agent");

        let socket_path = config.agent_socket_path(&agent_id);
        let pid_file = config.agent_pid_path(&agent_id);

        // Ensure directories exist
        if let Some(parent) = socket_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if let Some(parent) = pid_file.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Clean up old socket if exists
        if socket_path.exists() {
            tokio::fs::remove_file(&socket_path).await?;
        }

        // Determine agent binary
        let agent_binary = config
            .agent_binary
            .clone()
            .unwrap_or_else(|| std::env::current_exe().unwrap_or_else(|_| PathBuf::from("aosctl")));

        // Spawn the agent process
        let global_seed_hex = config.global_seed.map(hex::encode).unwrap_or_default();

        let child = Command::new(&agent_binary)
            .arg("agent")
            .arg("worker")
            .arg("--agent-id")
            .arg(&agent_id)
            .arg("--socket")
            .arg(&socket_path)
            .arg("--seed")
            .arg(&global_seed_hex)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| AgentSpawnError::spawn_failed(&agent_id, e.to_string()))?;

        let pid = child.id().unwrap_or(0);

        // Write PID file
        tokio::fs::write(&pid_file, pid.to_string()).await?;

        info!(
            agent_id = %agent_id,
            pid = pid,
            socket = %socket_path.display(),
            "Agent process spawned"
        );

        let handle = Self {
            id: agent_id.clone(),
            pid,
            socket_path,
            pid_file,
            child: Mutex::new(Some(child)),
            state: Arc::new(AtomicU8::new(AgentState::Starting as u8)),
            stream: Mutex::new(None),
            config: config.clone(),
            started_at: std::time::Instant::now(),
        };

        Ok(handle)
    }

    /// Wait for agent to be ready (socket available and handshake complete)
    pub async fn wait_ready(&self, timeout: Duration) -> Result<()> {
        let start = std::time::Instant::now();

        // Wait for socket to exist
        while start.elapsed() < timeout {
            if self.socket_path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        if !self.socket_path.exists() {
            return Err(AgentSpawnError::timeout(
                format!("waiting for agent {} socket", self.id),
                timeout.as_secs(),
            ));
        }

        // Connect to agent
        let remaining = timeout.saturating_sub(start.elapsed());
        let stream = tokio::time::timeout(remaining, UnixStream::connect(&self.socket_path))
            .await
            .map_err(|_| AgentSpawnError::timeout("connecting to agent", timeout.as_secs()))?
            .map_err(|e| AgentSpawnError::socket_error(self.socket_path.clone(), e.to_string()))?;

        // Store stream
        *self.stream.lock().await = Some(stream);

        // Complete handshake
        let handshake_response = self.complete_handshake().await?;

        if !handshake_response.is_success() {
            return Err(AgentSpawnError::protocol_error(format!(
                "Handshake failed: {:?}",
                handshake_response.error
            )));
        }

        self.set_state(AgentState::Ready);
        info!(agent_id = %self.id, "Agent ready");

        Ok(())
    }

    /// Complete the handshake with the agent
    async fn complete_handshake(&self) -> Result<HandshakeResponse> {
        // Note: In a real implementation, the agent would send a HandshakeRequest
        // and we'd respond with HandshakeResponse. For now, we simulate a simple
        // ping-pong handshake.
        let response = HandshakeResponse::accepted(format!("session-{}", self.id))
            .with_worker_socket(self.config.worker_socket.to_string_lossy().to_string());

        if let Some(seed) = self.config.global_seed {
            return Ok(response.with_global_seed(seed));
        }

        Ok(response)
    }

    /// Send a request to the agent
    pub async fn send(&self, request: AgentRequest) -> Result<()> {
        let mut stream_guard = self.stream.lock().await;
        let stream = stream_guard
            .as_mut()
            .ok_or_else(|| AgentSpawnError::communication_failed(&self.id, "No connection"))?;

        let json = serde_json::to_string(&request)?;
        let message = format!("{}\n", json);

        stream
            .write_all(message.as_bytes())
            .await
            .map_err(|e| AgentSpawnError::communication_failed(&self.id, e.to_string()))?;

        stream
            .flush()
            .await
            .map_err(|e| AgentSpawnError::communication_failed(&self.id, e.to_string()))?;

        debug!(agent_id = %self.id, request_type = ?std::mem::discriminant(&request), "Sent request to agent");

        Ok(())
    }

    /// Receive a response from the agent with timeout
    pub async fn recv(&self, timeout: Duration) -> Result<AgentResponse> {
        let mut stream_guard = self.stream.lock().await;
        let stream = stream_guard
            .as_mut()
            .ok_or_else(|| AgentSpawnError::communication_failed(&self.id, "No connection"))?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();

        let result = tokio::time::timeout(timeout, reader.read_line(&mut line)).await;

        match result {
            Ok(Ok(0)) => Err(AgentSpawnError::communication_failed(
                &self.id,
                "Connection closed",
            )),
            Ok(Ok(_)) => {
                let response: AgentResponse = serde_json::from_str(line.trim())?;
                debug!(agent_id = %self.id, response_type = ?std::mem::discriminant(&response), "Received response from agent");
                Ok(response)
            }
            Ok(Err(e)) => Err(AgentSpawnError::communication_failed(
                &self.id,
                e.to_string(),
            )),
            Err(_) => Err(AgentSpawnError::timeout(
                format!("receiving from agent {}", self.id),
                timeout.as_secs(),
            )),
        }
    }

    /// Send a request and wait for response
    pub async fn request(&self, request: AgentRequest, timeout: Duration) -> Result<AgentResponse> {
        self.send(request).await?;
        self.recv(timeout).await
    }

    /// Check if the agent process is still running
    pub async fn is_alive(&self) -> bool {
        let mut child_guard = self.child.lock().await;
        if let Some(ref mut child) = *child_guard {
            match child.try_wait() {
                Ok(None) => true,     // Still running
                Ok(Some(_)) => false, // Exited
                Err(_) => false,      // Error checking
            }
        } else {
            false
        }
    }

    /// Get the current agent state
    pub fn state(&self) -> AgentState {
        match self.state.load(Ordering::Acquire) {
            0 => AgentState::Starting,
            1 => AgentState::Ready,
            2 => AgentState::Working,
            3 => AgentState::WaitingAtBarrier,
            4 => AgentState::Completed,
            5 => AgentState::Failed,
            6 => AgentState::ShuttingDown,
            _ => AgentState::Failed,
        }
    }

    /// Set the agent state
    pub fn set_state(&self, state: AgentState) {
        self.state.store(state as u8, Ordering::Release);
    }

    /// Get uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Gracefully shutdown the agent
    pub async fn shutdown(&self, drain_timeout: Duration) -> Result<()> {
        info!(agent_id = %self.id, "Shutting down agent");
        self.set_state(AgentState::ShuttingDown);

        // Send shutdown request
        let shutdown_request = AgentRequest::Shutdown {
            drain_timeout_ms: drain_timeout.as_millis() as u64,
        };

        if let Err(e) = self.send(shutdown_request).await {
            warn!(agent_id = %self.id, error = %e, "Failed to send shutdown request");
        }

        // Wait for graceful exit
        let mut child_guard = self.child.lock().await;
        if let Some(ref mut child) = *child_guard {
            match tokio::time::timeout(drain_timeout, child.wait()).await {
                Ok(Ok(status)) => {
                    info!(agent_id = %self.id, status = ?status, "Agent exited gracefully");
                }
                Ok(Err(e)) => {
                    warn!(agent_id = %self.id, error = %e, "Error waiting for agent");
                }
                Err(_) => {
                    warn!(agent_id = %self.id, "Agent did not exit in time, killing");
                    let _ = child.kill().await;
                }
            }
        }

        // Cleanup
        self.cleanup().await;

        Ok(())
    }

    /// Force kill the agent
    pub async fn kill(&self) -> Result<()> {
        warn!(agent_id = %self.id, "Force killing agent");
        self.set_state(AgentState::Failed);

        let mut child_guard = self.child.lock().await;
        if let Some(ref mut child) = *child_guard {
            let _ = child.kill().await;
        }

        self.cleanup().await;
        Ok(())
    }

    /// Cleanup agent resources (socket, PID file)
    async fn cleanup(&self) {
        // Remove socket file
        if self.socket_path.exists() {
            if let Err(e) = tokio::fs::remove_file(&self.socket_path).await {
                warn!(path = %self.socket_path.display(), error = %e, "Failed to remove socket file");
            }
        }

        // Remove PID file
        if self.pid_file.exists() {
            if let Err(e) = tokio::fs::remove_file(&self.pid_file).await {
                warn!(path = %self.pid_file.display(), error = %e, "Failed to remove PID file");
            }
        }

        // Close stream
        *self.stream.lock().await = None;
    }
}

impl Drop for AgentHandle {
    fn drop(&mut self) {
        // Note: Async cleanup happens in shutdown/kill methods.
        // This is a sync fallback that will be called if handle is dropped.
        debug!(agent_id = %self.id, "AgentHandle dropped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_state_conversion() {
        let state = Arc::new(AtomicU8::new(AgentState::Ready as u8));
        assert_eq!(state.load(Ordering::Acquire), 1);

        state.store(AgentState::Working as u8, Ordering::Release);
        assert_eq!(state.load(Ordering::Acquire), 2);
    }
}
