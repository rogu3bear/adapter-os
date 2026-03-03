use adapteros_api_types::ModelLoadStatus;
use adapteros_core::{Result, WorkerStatus};
use adapteros_db::ProtectedDb;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::warn;

static LIFECYCLE_DUAL_READ_MISMATCHES: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub enum ModelWorkerLifecycleEvent {
    ModelLoadRequested {
        tenant_id: String,
        model_id: String,
        worker_id: Option<String>,
        model_hash_b3: Option<String>,
        reason: String,
    },
    ModelUnloadRequested {
        tenant_id: String,
        model_id: String,
        worker_id: Option<String>,
        reason: String,
    },
    ModelSwitchResult {
        tenant_id: String,
        worker_id: Option<String>,
        from_model_id: Option<String>,
        to_model_id: Option<String>,
        to_model_hash_b3: Option<String>,
        success: bool,
        error: Option<String>,
        memory_usage_mb: Option<i32>,
        reason: String,
    },
    WorkerStatusChanged {
        tenant_id: String,
        worker_id: String,
        status: String,
        reason: String,
    },
    WorkerModelTelemetry {
        tenant_id: String,
        worker_id: String,
        active_model_id: Option<String>,
        active_model_hash_b3: Option<String>,
        desired_model_id: Option<String>,
        status: Option<String>,
        generation: Option<i64>,
        last_error: Option<String>,
        memory_usage_mb: Option<i32>,
    },
}

#[derive(Clone)]
pub struct ModelWorkerLifecycleReducer {
    db: ProtectedDb,
    unified_enabled: bool,
}

impl ModelWorkerLifecycleReducer {
    pub fn from_env(db: ProtectedDb) -> Self {
        Self {
            db,
            unified_enabled: unified_worker_model_lifecycle_enabled(),
        }
    }

    pub fn dual_read_mismatch_count() -> u64 {
        LIFECYCLE_DUAL_READ_MISMATCHES.load(Ordering::Relaxed)
    }

    pub async fn reduce(&self, event: ModelWorkerLifecycleEvent) -> Result<()> {
        match event {
            ModelWorkerLifecycleEvent::ModelLoadRequested {
                tenant_id,
                model_id,
                worker_id,
                model_hash_b3,
                reason,
            } => {
                if let Some(worker_id) = worker_id.as_deref() {
                    self.try_transition_worker_model_state(
                        worker_id,
                        "loading",
                        &reason,
                        Some(&model_id),
                        model_hash_b3.as_deref(),
                        Some(&model_id),
                        None,
                        None,
                    )
                    .await?;
                }

                self.db
                    .update_base_model_status(&tenant_id, &model_id, "loading", None, None)
                    .await?;
                self.recompute_projection_with_mismatch_probe(&tenant_id, &model_id)
                    .await?;
            }
            ModelWorkerLifecycleEvent::ModelUnloadRequested {
                tenant_id,
                model_id,
                worker_id,
                reason,
            } => {
                if let Some(worker_id) = worker_id.as_deref() {
                    self.try_transition_worker_model_state(
                        worker_id,
                        "unloading",
                        &reason,
                        Some(&model_id),
                        None,
                        None,
                        None,
                        None,
                    )
                    .await?;
                }

                self.db
                    .update_base_model_status(&tenant_id, &model_id, "unloading", None, None)
                    .await?;
                self.recompute_projection_with_mismatch_probe(&tenant_id, &model_id)
                    .await?;
            }
            ModelWorkerLifecycleEvent::ModelSwitchResult {
                tenant_id,
                worker_id,
                from_model_id,
                to_model_id,
                to_model_hash_b3,
                success,
                error,
                memory_usage_mb,
                reason,
            } => {
                if success {
                    let terminal_status = if to_model_id.is_some() {
                        "ready"
                    } else {
                        "no-model"
                    };
                    if let Some(worker_id) = worker_id.as_deref() {
                        self.try_transition_worker_model_state(
                            worker_id,
                            terminal_status,
                            &reason,
                            to_model_id.as_deref(),
                            to_model_hash_b3.as_deref(),
                            to_model_id.as_deref(),
                            None,
                            memory_usage_mb,
                        )
                        .await?;
                    }
                } else {
                    let keep_serving_status = if from_model_id.is_some() {
                        "ready"
                    } else {
                        "error"
                    };
                    if let Some(worker_id) = worker_id.as_deref() {
                        self.try_transition_worker_model_state(
                            worker_id,
                            keep_serving_status,
                            &reason,
                            from_model_id.as_deref(),
                            None,
                            to_model_id.as_deref(),
                            error.as_deref(),
                            memory_usage_mb,
                        )
                        .await?;
                    }

                    if let Some(target_model_id) = to_model_id.as_deref() {
                        self.db
                            .update_base_model_status(
                                &tenant_id,
                                target_model_id,
                                "error",
                                error.as_deref(),
                                memory_usage_mb,
                            )
                            .await?;
                    }
                }

                if let Some(model_id) = from_model_id.as_deref() {
                    self.recompute_projection_with_mismatch_probe(&tenant_id, model_id)
                        .await?;
                }
                if let Some(model_id) = to_model_id.as_deref() {
                    self.recompute_projection_with_mismatch_probe(&tenant_id, model_id)
                        .await?;
                }
            }
            ModelWorkerLifecycleEvent::WorkerStatusChanged {
                tenant_id,
                worker_id,
                status,
                reason,
            } => {
                let worker_status = match WorkerStatus::from_str(&status) {
                    Ok(status) => status,
                    Err(e) => {
                        if self.unified_enabled {
                            return Err(e);
                        }
                        warn!(
                            worker_id = %worker_id,
                            status = %status,
                            error = %e,
                            "Ignoring unparseable worker status for lifecycle reducer (legacy mode)"
                        );
                        return Ok(());
                    }
                };

                if !worker_status.is_terminal() {
                    return Ok(());
                }

                let existing = self.db.get_worker_model_state(&worker_id).await?;
                let Some(existing) = existing else {
                    return Ok(());
                };

                let terminal_to_status = if worker_status == WorkerStatus::Error {
                    "error"
                } else {
                    "no-model"
                };

                let terminal_active_model_id = if worker_status == WorkerStatus::Error {
                    existing.active_model_id.as_deref()
                } else {
                    None
                };

                self.try_transition_worker_model_state(
                    &worker_id,
                    terminal_to_status,
                    &reason,
                    terminal_active_model_id,
                    existing.active_model_hash_b3.as_deref(),
                    existing.desired_model_id.as_deref(),
                    if worker_status == WorkerStatus::Error {
                        Some(reason.as_str())
                    } else {
                        None
                    },
                    existing.memory_usage_mb,
                )
                .await?;

                if let Some(active_model_id) = existing.active_model_id.as_deref() {
                    self.recompute_projection_with_mismatch_probe(&tenant_id, active_model_id)
                        .await?;
                }
                if let Some(desired_model_id) = existing.desired_model_id.as_deref() {
                    self.recompute_projection_with_mismatch_probe(&tenant_id, desired_model_id)
                        .await?;
                }
            }
            ModelWorkerLifecycleEvent::WorkerModelTelemetry {
                tenant_id,
                worker_id,
                active_model_id,
                active_model_hash_b3,
                desired_model_id,
                status,
                generation,
                last_error,
                memory_usage_mb,
            } => {
                let status = status
                    .as_deref()
                    .map(canonical_model_status)
                    .map(|s| s.to_string())
                    .or_else(|| active_model_id.as_ref().map(|_| "ready".to_string()))
                    .unwrap_or_else(|| "no-model".to_string());

                if let Some(generation) = generation {
                    self.db
                        .upsert_worker_model_state(
                            &worker_id,
                            &tenant_id,
                            active_model_id.as_deref(),
                            active_model_hash_b3.as_deref(),
                            desired_model_id.as_deref(),
                            &status,
                            generation,
                            last_error.as_deref(),
                            memory_usage_mb,
                        )
                        .await?;
                } else {
                    self.try_transition_worker_model_state(
                        &worker_id,
                        &status,
                        "worker telemetry update",
                        active_model_id.as_deref(),
                        active_model_hash_b3.as_deref(),
                        desired_model_id.as_deref(),
                        last_error.as_deref(),
                        memory_usage_mb,
                    )
                    .await?;
                }

                if let Some(model_id) = active_model_id.as_deref() {
                    self.recompute_projection_with_mismatch_probe(&tenant_id, model_id)
                        .await?;
                }
                if let Some(model_id) = desired_model_id.as_deref() {
                    self.recompute_projection_with_mismatch_probe(&tenant_id, model_id)
                        .await?;
                }
            }
        }

        Ok(())
    }

    async fn try_transition_worker_model_state(
        &self,
        worker_id: &str,
        to_status: &str,
        reason: &str,
        active_model_id: Option<&str>,
        active_model_hash_b3: Option<&str>,
        desired_model_id: Option<&str>,
        last_error: Option<&str>,
        memory_usage_mb: Option<i32>,
    ) -> Result<()> {
        match self
            .db
            .transition_worker_model_state(
                worker_id,
                canonical_model_status(to_status),
                reason,
                Some("control-plane-reducer"),
                active_model_id,
                active_model_hash_b3,
                desired_model_id,
                last_error,
                memory_usage_mb,
            )
            .await
        {
            Ok(()) => Ok(()),
            Err(e) if !self.unified_enabled => {
                warn!(
                    worker_id = %worker_id,
                    status = %to_status,
                    error = %e,
                    "Reducer worker-model transition failed in compatibility mode; continuing"
                );
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    async fn recompute_projection_with_mismatch_probe(
        &self,
        tenant_id: &str,
        model_id: &str,
    ) -> Result<()> {
        let legacy_before = self
            .db
            .get_base_model_status_for_model(tenant_id, model_id)
            .await?
            .map(|s| canonical_model_status(&s.status).to_string());

        let projected = self
            .db
            .recompute_base_model_status_projection(tenant_id, model_id)
            .await?;

        if let Some(before) = legacy_before {
            if before != projected {
                let mismatch_count =
                    LIFECYCLE_DUAL_READ_MISMATCHES.fetch_add(1, Ordering::Relaxed) + 1;
                if mismatch_count == 1 || mismatch_count.is_multiple_of(100) {
                    warn!(
                        tenant_id = %tenant_id,
                        model_id = %model_id,
                        legacy = %before,
                        projected = %projected,
                        mismatch_count,
                        "Detected legacy/projection model lifecycle mismatch"
                    );
                }
            }
        }

        Ok(())
    }
}

#[inline]
pub fn canonical_model_status(status: &str) -> &'static str {
    match ModelLoadStatus::parse_status(status) {
        ModelLoadStatus::Checking => "loading",
        other => other.as_str(),
    }
}

fn unified_worker_model_lifecycle_enabled() -> bool {
    std::env::var("AOS_UNIFIED_WORKER_MODEL_LIFECYCLE")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(true)
}

pub fn unified_worker_model_lifecycle_flag_name() -> &'static str {
    "AOS_UNIFIED_WORKER_MODEL_LIFECYCLE"
}
