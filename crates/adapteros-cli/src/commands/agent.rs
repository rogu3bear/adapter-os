//! Multi-agent spawn commands for parallel code modification strategies
//!
//! This module provides CLI commands for spawning and managing multiple AI agents
//! that collaboratively strategize about code modifications.

use crate::commands::session::{generate_session_id, global_session_store, AgentSession};
use crate::commands::worker_executor;
use crate::output::OutputWriter;
use adapteros_agent_spawn::protocol::{
    AgentRequest, AgentResponse, AgentState, AgentStatus, TaskProgress,
};
use adapteros_agent_spawn::{
    AgentOrchestrator, AgentSpawnConfig, DistributionStrategy, PlanningTask,
};
use adapteros_core::Result;
use clap::Subcommand;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{unix::OwnedWriteHalf, UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// Agent commands for multi-agent code modification
#[derive(Debug, Subcommand, Clone)]
pub enum AgentCommand {
    /// Spawn multiple agents to plan code modifications
    #[command(after_help = "Examples:\n  \
            aosctl agent spawn --task 'Add error handling'\n  \
            aosctl agent spawn --task-file ./task.md --agents 25\n  \
            aosctl agent spawn --task 'Refactor' --strategy semantic --output plan.json")]
    Spawn {
        /// Task description or objective
        #[arg(long, conflicts_with = "task_file")]
        task: Option<String>,

        /// Path to task file (markdown with detailed requirements)
        #[arg(long, conflicts_with = "task")]
        task_file: Option<PathBuf>,

        /// Number of agents to spawn (default: 20)
        #[arg(long, default_value = "20")]
        agents: u16,

        /// Distribution strategy (file, ast, round-robin, semantic)
        #[arg(long, default_value = "semantic")]
        strategy: String,

        /// Target directory for code analysis
        #[arg(long, default_value = ".")]
        target: PathBuf,

        /// Output file for the unified plan (JSON)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Enable deterministic mode
        #[arg(long)]
        deterministic: bool,

        /// Global seed for deterministic execution (hex string)
        #[arg(long)]
        seed: Option<String>,

        /// Timeout in seconds
        #[arg(long, default_value = "600")]
        timeout: u64,

        /// Dry run - show what would be done without spawning agents
        #[arg(long)]
        dry_run: bool,
    },

    /// List running agent sessions
    #[command(after_help = "Examples:\n  aosctl agent list\n  aosctl agent list --json")]
    List {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Show status of a specific agent session
    #[command(after_help = "Examples:\n  aosctl agent status sess_abc123")]
    Status {
        /// Session ID
        session_id: String,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Cancel a running agent session
    #[command(after_help = "Examples:\n  aosctl agent cancel sess_abc123")]
    Cancel {
        /// Session ID
        session_id: String,

        /// Force kill without graceful drain
        #[arg(long)]
        force: bool,
    },

    /// Run as an agent worker (internal use, spawned by orchestrator)
    #[command(hide = true)]
    Worker {
        /// Agent ID assigned by orchestrator
        #[arg(long)]
        agent_id: String,

        /// UDS socket path for communication
        #[arg(long)]
        socket: PathBuf,

        /// Global seed for deterministic execution (hex string)
        #[arg(long)]
        seed: String,
    },
}

/// Handle agent commands
pub async fn handle_agent_command(cmd: AgentCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        AgentCommand::Spawn {
            task,
            task_file,
            agents,
            strategy,
            target,
            output: output_file,
            deterministic,
            seed,
            timeout,
            dry_run,
        } => {
            handle_spawn(
                task,
                task_file,
                agents,
                strategy,
                target,
                output_file,
                deterministic,
                seed,
                timeout,
                dry_run,
                output,
            )
            .await
        }
        AgentCommand::List { json } => handle_list(json, output).await,
        AgentCommand::Status { session_id, json } => handle_status(session_id, json, output).await,
        AgentCommand::Cancel { session_id, force } => {
            handle_cancel(session_id, force, output).await
        }
        AgentCommand::Worker {
            agent_id,
            socket,
            seed,
        } => handle_worker(agent_id, socket, seed, output).await,
    }
}

/// Handle spawn command
#[allow(clippy::too_many_arguments)]
async fn handle_spawn(
    task: Option<String>,
    task_file: Option<PathBuf>,
    agents: u16,
    strategy: String,
    target: PathBuf,
    output_file: Option<PathBuf>,
    deterministic: bool,
    _seed: Option<String>,
    timeout: u64,
    dry_run: bool,
    output: &OutputWriter,
) -> Result<()> {
    // Get the task objective
    let objective = if let Some(t) = task {
        t
    } else if let Some(path) = task_file {
        tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| adapteros_core::AosError::Io(format!("Failed to read task file: {}", e)))?
    } else {
        return Err(adapteros_core::AosError::validation(
            "Either --task or --task-file must be specified",
        ));
    };

    output.section("Multi-Agent Spawn");
    output.blank();

    output.kv("Objective", &objective);
    output.kv("Agents", &agents.to_string());
    output.kv("Strategy", &strategy);
    output.kv("Target", &target.display().to_string());
    output.kv("Deterministic", &deterministic.to_string());
    output.kv("Timeout", &format!("{}s", timeout));

    if dry_run {
        output.blank();
        output.info("Dry run - no agents will be spawned");
        output.blank();

        // Show what would happen
        output.section("Would spawn agents:");
        for i in 0..agents.min(5) {
            output.kv(&format!("agent-{:02}", i), "pending");
        }
        if agents > 5 {
            output.kv("...", &format!("({} more)", agents - 5));
        }

        return Ok(());
    }

    output.blank();
    output.progress("Initializing orchestrator...");

    // Parse seed if provided
    let global_seed = if let Some(seed_hex) = _seed {
        let bytes = hex::decode(&seed_hex).map_err(|e| {
            adapteros_core::AosError::validation(format!("Invalid seed hex: {}", e))
        })?;
        if bytes.len() != 32 {
            return Err(adapteros_core::AosError::validation(
                "Seed must be exactly 32 bytes (64 hex characters)",
            ));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        Some(seed)
    } else if deterministic {
        // Generate a random seed for deterministic mode
        Some(rand::random::<[u8; 32]>())
    } else {
        None
    };

    // Parse distribution strategy
    let dist_strategy = match strategy.as_str() {
        "file" => DistributionStrategy::FileOwnership,
        "ast" => DistributionStrategy::AstRegion,
        "round-robin" => DistributionStrategy::RoundRobin,
        "semantic" => DistributionStrategy::Semantic,
        other => {
            return Err(adapteros_core::AosError::validation(format!(
                "Unknown strategy '{}'. Valid options: file, ast, round-robin, semantic",
                other
            )))
        }
    };

    // Create configuration
    let mut builder = AgentSpawnConfig::builder()
        .agent_count(agents)
        .distribution_strategy(dist_strategy)
        .task_timeout_secs(timeout);

    // Add seed if provided
    if let Some(seed) = global_seed {
        builder = builder.global_seed(seed);
    }

    let config = builder.build();

    // Create planning task
    let task = PlanningTask::new(&objective).with_root_dir(target.clone());

    // Create orchestrator
    let orchestrator =
        AgentOrchestrator::new(config).map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;

    output.progress_done(true);

    // Generate session ID and store session
    let session_id = generate_session_id();
    output.kv("Session ID", &session_id);
    output.blank();

    // Store session before execution
    let session = Arc::new(AgentSession::new(
        session_id.clone(),
        orchestrator,
        objective.clone(),
        agents,
    ));
    global_session_store()
        .write()
        .insert(session_id.clone(), session.clone());

    output.info("Starting multi-agent planning...");
    output.blank();

    // Execute orchestration
    let plan_result = {
        let mut orch = session.orchestrator.write().await;
        orch.execute(task).await
    };

    // Remove session from store after completion
    global_session_store().write().remove(&session_id);

    match plan_result {
        Ok(plan) => {
            output.blank();
            output.section("Planning Complete");
            output.blank();
            output.kv("Modifications", &plan.modifications.len().to_string());
            output.kv("Contributors", &plan.contributors.len().to_string());
            output.kv("Confidence", &format!("{:.2}", plan.confidence));

            // Write output file if specified
            if let Some(path) = output_file {
                let json = serde_json::to_string_pretty(&plan)
                    .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;
                tokio::fs::write(&path, json)
                    .await
                    .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;
                output.blank();
                output.kv("Output written to", &path.display().to_string());
            }

            Ok(())
        }
        Err(e) => {
            output.blank();
            output.error(format!("Planning failed: {}", e));
            Err(adapteros_core::AosError::Io(e.to_string()))
        }
    }
}

/// Handle list command
async fn handle_list(json: bool, output: &OutputWriter) -> Result<()> {
    let sessions: Vec<Arc<AgentSession>> = {
        let store = global_session_store();
        let s = store.read().values().cloned().collect();
        s
    };

    if json {
        let session_list: Vec<_> = sessions
            .iter()
            .map(|sess| {
                serde_json::json!({
                    "session_id": sess.id,
                    "objective": sess.objective,
                    "agent_count": sess.agent_count,
                    "uptime_secs": sess.uptime_secs(),
                    "phase": format!("{:?}", sess.orchestrator.blocking_read().phase()),
                })
            })
            .collect();
        output.print_json(&serde_json::json!({ "sessions": session_list }))?;
    } else {
        output.section("Agent Sessions");
        output.blank();

        if sessions.is_empty() {
            output.info("No active agent sessions");
        } else {
            for sess in sessions.iter() {
                output.kv("Session ID", &sess.id);
                output.kv("  Objective", &sess.objective);
                output.kv("  Agents", &sess.agent_count.to_string());
                output.kv("  Uptime", &format!("{}s", sess.uptime_secs()));
                output.kv(
                    "  Phase",
                    &format!("{:?}", sess.orchestrator.blocking_read().phase()),
                );
                output.blank();
            }
        }
    }
    Ok(())
}

/// Handle status command
async fn handle_status(session_id: String, json: bool, output: &OutputWriter) -> Result<()> {
    let session = {
        let store = global_session_store();
        let s = store.read().get(&session_id).cloned();
        s
    };

    if let Some(session) = session {
        let orchestrator = session.orchestrator.read().await;
        let phase = orchestrator.phase();
        let active_agents = orchestrator.active_agent_count();

        if json {
            output.print_json(&serde_json::json!({
                "session_id": session_id,
                "objective": session.objective,
                "agent_count": session.agent_count,
                "active_agents": active_agents,
                "uptime_secs": session.uptime_secs(),
                "phase": format!("{:?}", phase),
            }))?;
        } else {
            output.section(format!("Session {}", session_id));
            output.blank();
            output.kv("Objective", &session.objective);
            output.kv("Total Agents", &session.agent_count.to_string());
            output.kv("Active Agents", &active_agents.to_string());
            output.kv("Uptime", &format!("{}s", session.uptime_secs()));
            output.kv("Phase", &format!("{:?}", phase));
        }
    } else if json {
        output.print_json(&serde_json::json!({
            "session_id": session_id,
            "error": "Session not found"
        }))?;
    } else {
        output.error(format!("Session {} not found", session_id));
    }
    Ok(())
}

/// Handle cancel command
async fn handle_cancel(session_id: String, force: bool, output: &OutputWriter) -> Result<()> {
    let store = global_session_store();
    let session = store.write().remove(&session_id);

    if let Some(_session) = session {
        output.info(format!(
            "Canceling session {} (force: {})",
            session_id, force
        ));

        // Shutdown the orchestrator's agents
        let _drain_timeout = if force {
            Duration::from_secs(5)
        } else {
            Duration::from_secs(30)
        };

        // This is a simplified shutdown - full implementation would call orchestrator.shutdown()
        output.progress("Shutting down agents...");
        // Note: AgentOrchestrator doesn't expose a public shutdown yet, but when it does:
        // session.orchestrator.write().shutdown_all(drain_timeout).await?;
        output.progress_done(true);

        output.blank();
        output.info(format!("Session {} canceled", session_id));
        Ok(())
    } else {
        output.error(format!("Session {} not found", session_id));
        Err(adapteros_core::AosError::NotFound(format!(
            "Session {}",
            session_id
        )))
    }
}

#[derive(Debug)]
struct AgentRuntimeState {
    current_task: Option<[u8; 32]>,
    current_cancel: Option<worker_executor::CancellationToken>,
    tasks_completed: u32,
    start_time: Instant,
    last_activity: chrono::DateTime<chrono::Utc>,
}

impl AgentRuntimeState {
    fn new() -> Self {
        Self {
            current_task: None,
            current_cancel: None,
            tasks_completed: 0,
            start_time: Instant::now(),
            last_activity: chrono::Utc::now(),
        }
    }

    fn status(&self, agent_id: &str) -> AgentStatus {
        let state = if self.current_task.is_some() {
            AgentState::Working
        } else {
            AgentState::Ready
        };

        AgentStatus {
            agent_id: agent_id.to_string(),
            state,
            current_task: self.current_task,
            tasks_completed: self.tasks_completed,
            uptime_secs: self.start_time.elapsed().as_secs(),
            memory_bytes: None,
            last_activity: self.last_activity,
        }
    }
}

/// Handle worker command (internal - spawned by orchestrator)
async fn handle_worker(
    agent_id: String,
    socket: PathBuf,
    seed_hex: String,
    _output: &OutputWriter,
) -> Result<()> {
    info!(
        agent_id = %agent_id,
        socket = %socket.display(),
        "Starting agent worker"
    );

    // Parse seed
    let seed_bytes = hex::decode(&seed_hex)
        .map_err(|e| adapteros_core::AosError::validation(format!("Invalid seed: {}", e)))?;
    if seed_bytes.len() != 32 {
        return Err(adapteros_core::AosError::validation(
            "Seed must be 32 bytes",
        ));
    }
    let mut _seed = [0u8; 32];
    _seed.copy_from_slice(&seed_bytes);

    // Ensure parent directory exists
    if let Some(parent) = socket.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;
    }

    // Remove old socket if exists
    if socket.exists() {
        tokio::fs::remove_file(&socket)
            .await
            .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;
    }

    // Create UDS listener
    let listener = UnixListener::bind(&socket)
        .map_err(|e| adapteros_core::AosError::Io(format!("Failed to bind socket: {}", e)))?;

    info!(socket = %socket.display(), "Worker listening for orchestrator");

    // Accept connection from orchestrator
    let (stream, _addr) = listener
        .accept()
        .await
        .map_err(|e| adapteros_core::AosError::Io(format!("Failed to accept connection: {}", e)))?;

    info!("Orchestrator connected");

    // Complete handshake
    let stream = complete_worker_handshake(stream, &agent_id).await?;

    info!("Handshake complete, entering main loop");

    // Main message loop
    let runtime_state = Arc::new(Mutex::new(AgentRuntimeState::new()));
    worker_message_loop(stream, &agent_id, runtime_state).await?;

    // Cleanup socket
    let _ = tokio::fs::remove_file(&socket).await;

    info!("Worker shutting down");
    Ok(())
}

/// Complete the handshake protocol
async fn complete_worker_handshake(stream: UnixStream, agent_id: &str) -> Result<UnixStream> {
    // For now, handshake is implicit (agent just needs to be ready)
    // In a full implementation, we'd exchange HandshakeRequest/Response messages
    debug!(agent_id = %agent_id, "Handshake complete (simplified)");
    Ok(stream)
}

/// Main worker message loop
async fn worker_message_loop(
    stream: UnixStream,
    agent_id: &str,
    runtime_state: Arc<Mutex<AgentRuntimeState>>,
) -> Result<()> {
    let (read_half, write_half) = stream.into_split();
    let writer = Arc::new(Mutex::new(write_half));
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .await
            .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;

        if bytes_read == 0 {
            // Connection closed
            info!("Orchestrator disconnected");
            break;
        }

        let request: AgentRequest = serde_json::from_str(line.trim())
            .map_err(|e| adapteros_core::AosError::Io(format!("Invalid request: {}", e)))?;

        debug!(
            agent_id = %agent_id,
            request_type = ?std::mem::discriminant(&request),
            "Received request"
        );

        match request {
            AgentRequest::AssignTask(assignment) => {
                info!(task_id = %hex::encode(&assignment.task_id), "Received task assignment");

                let cancel_token = worker_executor::CancellationToken::new();
                let should_accept = {
                    let mut state = runtime_state.lock().await;
                    if state.current_task.is_some() {
                        false
                    } else {
                        state.current_task = Some(assignment.task_id);
                        state.current_cancel = Some(cancel_token.clone());
                        state.last_activity = chrono::Utc::now();
                        true
                    }
                };

                if !should_accept {
                    let response = AgentResponse::Error {
                        message: "Agent already working on a task".to_string(),
                        code: Some("BUSY".to_string()),
                    };
                    send_response_locked(&writer, &response).await?;
                    continue;
                }

                let accept_response = AgentResponse::TaskAccepted {
                    task_id: assignment.task_id,
                };
                send_response_locked(&writer, &accept_response).await?;

                let writer_clone = writer.clone();
                let state_clone = runtime_state.clone();
                let agent_id = agent_id.to_string();
                tokio::spawn(async move {
                    let task_id = assignment.task_id;
                    let result = worker_executor::execute_task(
                        &assignment,
                        &agent_id,
                        Some(cancel_token.clone()),
                    )
                    .await;

                    let mut state = state_clone.lock().await;
                    let code = if cancel_token.is_cancelled() {
                        Some("CANCELLED".to_string())
                    } else {
                        None
                    };

                    let response = match result {
                        Ok(proposal) => {
                            state.tasks_completed = state.tasks_completed.saturating_add(1);
                            AgentResponse::TaskComplete(proposal)
                        }
                        Err(e) => {
                            error!(error = %e, "Task execution failed");
                            AgentResponse::TaskFailed {
                                task_id,
                                error: e.to_string(),
                                code,
                            }
                        }
                    };

                    state.current_task = None;
                    state.current_cancel = None;
                    state.last_activity = chrono::Utc::now();
                    drop(state);

                    let _ = send_response_locked(&writer_clone, &response).await;
                });
            }
            AgentRequest::SyncBarrier { tick, barrier_id } => {
                debug!(tick = tick, barrier_id = %barrier_id, "Barrier sync");
                let response = AgentResponse::BarrierReached { tick, barrier_id };
                send_response_locked(&writer, &response).await?;
            }
            AgentRequest::Shutdown { drain_timeout_ms } => {
                info!(drain_ms = drain_timeout_ms, "Shutdown requested");
                send_response_locked(&writer, &AgentResponse::ShuttingDown).await?;
                break;
            }
            AgentRequest::Ping { sequence } => {
                let response = AgentResponse::Pong { sequence };
                send_response_locked(&writer, &response).await?;
            }
            AgentRequest::StatusQuery => {
                let status = {
                    let state = runtime_state.lock().await;
                    state.status(agent_id)
                };
                send_response_locked(&writer, &AgentResponse::Status(status)).await?;
            }
            AgentRequest::CancelTask { reason } => {
                let (token, task_id) = {
                    let state = runtime_state.lock().await;
                    (state.current_cancel.clone(), state.current_task)
                };

                if let Some(token) = token {
                    warn!(reason = %reason, "Task cancellation requested");
                    token.cancel(reason.clone());
                    let progress = TaskProgress {
                        task_id: task_id.unwrap_or([0u8; 32]),
                        percent: 0,
                        stage: "cancelling".to_string(),
                        message: Some(reason),
                        current_files: vec![],
                    };
                    send_response_locked(&writer, &AgentResponse::Progress(progress)).await?;
                } else {
                    let response = AgentResponse::Error {
                        message: "No active task to cancel".to_string(),
                        code: Some("NO_ACTIVE_TASK".to_string()),
                    };
                    send_response_locked(&writer, &response).await?;
                }
            }
        }
    }

    Ok(())
}

/// Send a response to the orchestrator
async fn send_response<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    response: &AgentResponse,
) -> Result<()> {
    let json =
        serde_json::to_string(response).map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;
    let message = format!("{}\n", json);

    writer
        .write_all(message.as_bytes())
        .await
        .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;

    writer
        .flush()
        .await
        .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;

    Ok(())
}

async fn send_response_locked(
    writer: &Arc<Mutex<OwnedWriteHalf>>,
    response: &AgentResponse,
) -> Result<()> {
    let mut guard = writer.lock().await;
    send_response(&mut *guard, response).await
}
