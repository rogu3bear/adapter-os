//! Typed SSE lifecycle event payloads
//!
//! Strongly typed event payloads emitted at key lifecycle points in the system.
//! Each enum maps to a specific `SseStreamType` and uses `#[serde(tag = "event")]`
//! so clients can pattern-match on the `event` field in the JSON.

use serde::{Deserialize, Serialize};

/// Adapter lifecycle events emitted on [`SseStreamType::AdapterState`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AdapterLifecycleEvent {
    /// Adapter tier promoted (e.g. persistent -> warm -> ephemeral)
    Promoted {
        adapter_id: String,
        from_state: String,
        to_state: String,
    },
    /// Adapter loaded into memory
    Loaded {
        adapter_id: String,
        load_time_ms: u64,
    },
    /// Adapter load failed
    LoadFailed { adapter_id: String, error: String },
    /// Adapter unloaded / evicted from memory
    Evicted { adapter_id: String, reason: String },
}

/// Training lifecycle events emitted on [`SseStreamType::Training`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TrainingLifecycleEvent {
    /// Training job started
    JobStarted {
        job_id: String,
        adapter_id: String,
        config_summary: String,
    },
    /// An epoch completed
    EpochCompleted {
        job_id: String,
        epoch: u32,
        total_epochs: u32,
        loss: f64,
        learning_rate: f64,
    },
    /// Checkpoint saved to disk
    CheckpointSaved {
        job_id: String,
        epoch: u32,
        path: String,
    },
    /// Training job completed successfully
    JobCompleted {
        job_id: String,
        adapter_id: String,
        final_loss: f64,
        duration_secs: u64,
    },
    /// Training job failed
    JobFailed {
        job_id: String,
        error: String,
        last_epoch: u32,
    },
}

/// System health transition events emitted on [`SseStreamType::Alerts`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SystemHealthEvent {
    /// Worker lifecycle state changed
    WorkerStateChanged {
        worker_id: String,
        previous: String,
        current: String,
        reason: String,
    },
    /// Drain phase started
    DrainStarted {
        worker_id: String,
        previous_status: String,
    },
    /// Adapter was evicted from memory pressure or explicit unload.
    AdapterEvicted {
        adapter_id: String,
        adapter_name: String,
        reason: String,
        #[serde(default)]
        freed_mb: u32,
    },
}

/// Internal memory eviction signal used by server components before SSE fan-out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEvictionEvent {
    pub tenant_id: String,
    pub adapter_id: String,
    pub adapter_name: String,
    pub reason: String,
    #[serde(default)]
    pub freed_mb: u32,
}

/// Adapter version lifecycle events emitted on [`SseStreamType::AdapterState`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AdapterVersionEvent {
    /// A version was promoted to active
    VersionPromoted {
        version_id: String,
        repo_id: String,
        branch: String,
    },
    /// A branch was rolled back to a previous version
    VersionRolledBack {
        repo_id: String,
        branch: String,
        target_version_id: String,
    },
    /// An automatic rollback was applied after dataset trust regression.
    AutoRollbackApplied {
        repo_id: String,
        branch: String,
        target_version_id: String,
        dataset_version_id: String,
        timeline_event_id: String,
        reason: String,
    },
}
