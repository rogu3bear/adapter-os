//! # Training Job Orchestration and Management
//!
//! Handles scheduling, executing, and monitoring adapter training jobs.
//! Integrates with worker backends (CoreML/MLX/Metal/CPU) for actual training operations.
//!
//! ## State Management Architecture (Triple State)
//!
//! Training jobs maintain state in **three separate locations**, which can
//! diverge under failure conditions. Understanding this is critical for
//! debugging and ensuring consistency.
//!
//! ```text
//! +-------------------------------------------------------------------------------+
//! |                         TRIPLE STATE MANAGEMENT                               |
//! |                                                                               |
//! |  +-------------------------------------------------------------------------+  |
//! |  |                    1. IN-MEMORY STATE (jobs HashMap)                    |  |
//! |  |                                                                         |  |
//! |  |  - Arc<RwLock<HashMap<String, TrainingJob>>>                            |  |
//! |  |  - Authoritative for progress_pct, current_epoch, status               |  |
//! |  |  - Lost on process restart                                             |  |
//! |  |  - Updated in real-time during training                                |  |
//! |  +-------------------------------------------------------------------------+  |
//! |                                   |                                           |
//! |                                   | persist (non-blocking)                    |
//! |                                   v                                           |
//! |  +-------------------------------------------------------------------------+  |
//! |  |                    2. DATABASE STATE (SQLite)                           |  |
//! |  |                                                                         |  |
//! |  |  - training_jobs table                                                  |  |
//! |  |  - Updated periodically (every epoch or status change)                  |  |
//! |  |  - **DB writes are non-fatal**: failures logged but don't stop job     |  |
//! |  |  - May lag behind in-memory state                                       |  |
//! |  +-------------------------------------------------------------------------+  |
//! |                                                                               |
//! |  +-------------------------------------------------------------------------+  |
//! |  |                    3. CANCEL TOKENS (AtomicBool)                        |  |
//! |  |                                                                         |  |
//! |  |  - Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>                        |  |
//! |  |  - Cooperative cancellation (checked at epoch boundaries)               |  |
//! |  |  - Token removed after job completes (success or failure)               |  |
//! |  |  - No persistence - cancel requests lost on restart                     |  |
//! |  +-------------------------------------------------------------------------+  |
//! +-------------------------------------------------------------------------------+
//! ```
//!
//! ## Race Condition Scenarios
//!
//! | Scenario | Symptom | Cause | Mitigation |
//! |----------|---------|-------|------------|
//! | Process crash during training | DB shows "running" but no progress | In-memory state lost | Implement startup recovery scan |
//! | DB write failure | DB shows old progress_pct | Non-fatal write logged | Retry logic, monitoring |
//! | Cancel during epoch | Job completes current epoch | Token only checked at boundaries | Document expected behavior |
//! | Concurrent status updates | Inconsistent reads | RwLock allows concurrent reads | Use single-writer pattern |
//!
//! ## Job Lifecycle
//!
//! ```text
//!   +------------+     create_job()     +--------------+
//!   |   (none)   | ------------------->|   pending    |
//!   +------------+                     +--------------+
//!                                              |
//!                                              | run_training_job()
//!                                              v
//!   +------------+     cancel_job()     +--------------+
//!   | cancelled  | <-------------------|   running    |
//!   +------------+                     +--------------+
//!                                              |
//!                          +---------------+---+---------------+
//!                          | success       |               | failure
//!                          v               v               |
//!                   +--------------+  +--------------+     |
//!                   |  completed   |  |    failed    |     |
//!                   +--------------+  +--------------+     |
//! ```
//!
//! ## Critical Functions
//!
//! - [`TrainingService::create_job`]: Creates job in pending state
//! - [`TrainingService::run_training_job`]: Spawns deterministic task, manages tokens
//! - [`TrainingService::cancel_job`]: Sets cancel token (cooperative cancellation)
//! - [`TrainingService::update_job_progress`]: Updates in-memory + DB (non-fatal write)
//!
//! ## Known Limitations
//!
//! 1. **Non-transactional**: In-memory and DB updates are not atomic
//! 2. **Cancel latency**: Up to 1 epoch delay for cancellation to take effect
//! 3. **No distributed locking**: Single-node assumption for job management
//! 4. **Recovery semantics**: Startup recovery and background cleanup handle orphaned jobs,
//!    but in-memory state is still lost on restart and DB may lag behind live progress.

mod config;
mod coreml;
mod dataset;
mod execution;
mod job;
mod metrics;
mod packaging;
mod pipeline;
mod report;
mod service;
mod versioning;

#[cfg(test)]
mod tests;

// Primary public API (matches current lib.rs exports)
pub use config::PostActions;
pub use job::{
    DataLineageMode, DatasetVersionSelection, DatasetVersionTrustSnapshot, LoraTier,
    TrainingBackendKind, TrainingBackendPolicy, TrainingConfig, TrainingJob, TrainingJobStatus,
    TrainingTemplate,
};
pub use service::{OrphanedJobRecoveryReport, TrainingService};
pub use versioning::{compute_combined_data_spec_hash, TrainingVersioningContext};

// Internal but exposed for other internal modules
// NOTE: These items are used within the training/ submodules, not from outside.
// They are not re-exported at the crate level but accessible via full paths.
