//! Periodic worker/model + workspace-active-state reconciliation.
//!
//! Runs in the background to keep worker model state and workspace active model
//! projections consistent after startup.

use crate::handlers::workspaces::reconcile_active_models;
use crate::state::AppState;
use futures_util::FutureExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info, warn};

const TASK_NAME: &str = "Worker state reconciler";
const RECONCILE_INTERVAL_SECS: u64 = 60;

/// Spawn periodic worker/workspace reconciliation at a fixed cadence.
pub fn spawn_worker_reconciler(state: Arc<AppState>) {
    let tracker = state.background_task_tracker();
    tracker.record_spawned(TASK_NAME, false);

    tokio::spawn(async move {
        let panic_tracker = Arc::clone(&tracker);
        if let Err(panic) = std::panic::AssertUnwindSafe(async move {
            let mut ticker = interval(Duration::from_secs(RECONCILE_INTERVAL_SECS));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // Consume the immediate first tick so cadence starts 60s after startup.
            ticker.tick().await;

            loop {
                ticker.tick().await;
                tracker.heartbeat(TASK_NAME);

                let mut had_failure = false;

                match state.db.reconcile_worker_model_states_at_startup().await {
                    Ok(count) => {
                        if count > 0 {
                            info!(
                                reconciled = count,
                                "Periodic worker model reconciliation applied updates"
                            );
                        }
                    }
                    Err(e) => {
                        had_failure = true;
                        tracker.record_failed(TASK_NAME, &e.to_string(), false);
                        warn!(
                            error = %e,
                            "Periodic worker model reconciliation failed"
                        );
                    }
                }

                reconcile_active_models(state.as_ref()).await;

                if !had_failure {
                    tracker.record_spawned(TASK_NAME, false);
                }
            }
        })
        .catch_unwind()
        .await
        {
            let panic_message = format!("background task panicked: {:?}", panic);
            panic_tracker.record_failed(TASK_NAME, &panic_message, false);
            error!(task = "worker_state_reconciliation", "{}", panic_message);
        }
    });
}
