//! Multi-agent spawn system for AdapterOS
//!
//! This crate provides infrastructure for spawning 15-30 parallel AI agent processes
//! to collaboratively strategize about code modifications. Agents communicate via
//! Unix Domain Sockets and synchronize using `AgentBarrier` from `adapteros-deterministic-exec`.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    aosctl agent spawn <task>                    │
//! └─────────────────────────────┬───────────────────────────────────┘
//!                               │
//! ┌─────────────────────────────▼───────────────────────────────────┐
//! │                      AgentOrchestrator                          │
//! │  ┌────────────────┐  ┌───────────────┐  ┌────────────────────┐  │
//! │  │  TaskRouter    │  │ ResultMerger  │  │ AgentBarrier       │  │
//! │  │  (work split)  │  │ (consolidate) │  │ (from det-exec)    │  │
//! │  └────────────────┘  └───────────────┘  └────────────────────┘  │
//! │  ┌─────────────────────────────────────────────────────────────┐│
//! │  │              AgentSupervisor (lifecycle, health)            ││
//! │  └─────────────────────────────────────────────────────────────┘│
//! └─────────────────────────────┬───────────────────────────────────┘
//!                               │ UDS (Unix Domain Sockets)
//!           ┌───────────────────┼───────────────────┐
//!           ▼                   ▼                   ▼
//!     ┌──────────┐        ┌──────────┐        ┌──────────┐
//!     │ Agent 0  │        │ Agent 1  │  ...   │ Agent N  │
//!     │ (process)│        │ (process)│        │ (process)│
//!     └──────────┘        └──────────┘        └──────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use adapteros_agent_spawn::{AgentOrchestrator, AgentSpawnConfig, PlanningTask};
//!
//! let config = AgentSpawnConfig::builder()
//!     .agent_count(20)
//!     .distribution_strategy(DistributionStrategy::Semantic)
//!     .build();
//!
//! let mut orchestrator = AgentOrchestrator::new(config)?;
//!
//! let task = PlanningTask::new("Add error handling to all API handlers");
//! let plan = orchestrator.execute(task).await?;
//! ```

pub mod agent;
pub mod config;
pub mod error;
pub mod orchestrator;
pub mod protocol;
pub mod result_merger;
pub mod supervisor;
pub mod task_router;

// Re-exports
pub use agent::AgentHandle;
pub use config::{AgentSpawnConfig, DistributionStrategy};
pub use error::{AgentSpawnError, Result};
pub use orchestrator::AgentOrchestrator;
pub use protocol::{AgentRequest, AgentResponse, TaskAssignment, TaskProposal};
pub use result_merger::{ConflictResolution, ResultMerger, UnifiedPlan};
pub use supervisor::AgentSupervisor;
pub use task_router::{CodebaseContext, PlanningTask, TaskRouter};
