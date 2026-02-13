//! Server availability tracking
//!
//! Provides a shared reactive signal indicating whether the backend is reachable.
//! When consecutive API failures exceed a threshold, the server is declared
//! unreachable. A background health probe runs with exponential backoff to
//! detect recovery.
//!
//! Polling hooks (`use_polling`, `use_conditional_polling`) check this context
//! and skip ticks when the server is down, preventing request storms against
//! an unavailable backend.

use crate::api::ApiClient;
use crate::boot_log;
use leptos::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// Consecutive failures before declaring server unreachable
const UNREACHABLE_THRESHOLD: u32 = 2;

/// Health probe base interval (ms)
const PROBE_BASE_MS: u32 = 2_000;

/// Health probe max interval (ms)
const PROBE_MAX_MS: u32 = 30_000;

/// Maximum probe attempts before giving up (~30 minutes at max backoff)
const PROBE_MAX_ATTEMPTS: u32 = 60;

/// Shared server availability state
#[derive(Clone)]
pub struct ServerStatus {
    /// Whether the server is currently reachable
    pub reachable: RwSignal<bool>,
    /// Consecutive failure count
    failures: Arc<AtomicU32>,
    /// Whether a recovery probe is already running
    probing: Arc<AtomicBool>,
}

impl ServerStatus {
    pub(crate) fn new() -> Self {
        Self {
            reachable: RwSignal::new(true),
            failures: Arc::new(AtomicU32::new(0)),
            probing: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Report a successful server contact - resets failure count
    pub fn report_success(&self) {
        self.failures.store(0, Ordering::Relaxed);
        if !self.reachable.get_untracked() {
            boot_log("server-status", "server recovered");
            self.reachable.set(true);
        }
    }

    /// Report a server contact failure
    pub fn report_failure(&self) {
        let count = self.failures.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= UNREACHABLE_THRESHOLD && self.reachable.get_untracked() {
            boot_log(
                "server-status",
                &format!("server unreachable after {} consecutive failures", count),
            );
            self.reachable.set(false);
            self.start_probe();
        }
    }

    /// Start a background health probe to detect server recovery
    fn start_probe(&self) {
        // Only one probe at a time
        if self.probing.swap(true, Ordering::Relaxed) {
            return;
        }

        boot_log("server-status", "starting recovery probe");

        let status = self.clone();
        let client = Arc::new(ApiClient::new());
        gloo_timers::callback::Timeout::new(0, move || {
            wasm_bindgen_futures::spawn_local(async move {
                let mut delay_ms = PROBE_BASE_MS;
                let mut attempts: u32 = 0;
                loop {
                    gloo_timers::future::TimeoutFuture::new(delay_ms).await;

                    // Another path may have recovered (e.g., auth retry succeeded)
                    if status.reachable.get_untracked() {
                        status.probing.store(false, Ordering::Relaxed);
                        break;
                    }

                    // Skip probe when tab is hidden
                    let tab_hidden = web_sys::window()
                        .and_then(|w| w.document())
                        .map(|d| d.hidden())
                        .unwrap_or(false);
                    if tab_hidden {
                        continue;
                    }

                    attempts += 1;
                    if attempts > PROBE_MAX_ATTEMPTS {
                        boot_log(
                            "server-status",
                            "health probe exhausted, waiting for manual retry",
                        );
                        status.probing.store(false, Ordering::Relaxed);
                        break;
                    }

                    match client.health().await {
                        Ok(_) => {
                            boot_log("server-status", "health probe succeeded, server recovered");
                            status.probing.store(false, Ordering::Relaxed);
                            status.report_success();
                            break;
                        }
                        Err(_) => {
                            boot_log(
                                "server-status",
                                &format!("health probe failed, next attempt in {}ms", delay_ms),
                            );
                            delay_ms = (delay_ms * 2).min(PROBE_MAX_MS);
                        }
                    }
                }
            });
        })
        .forget();
    }
}

/// Provide server status context at the app level
pub fn provide_server_status() {
    provide_context(ServerStatus::new());
}

/// Access server status context
pub fn use_server_status() -> ServerStatus {
    expect_context::<ServerStatus>()
}

/// Try to access server status without panicking
pub fn try_use_server_status() -> Option<ServerStatus> {
    use_context::<ServerStatus>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use leptos::prelude::Owner;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_report_success_resets_failures() {
        let owner = Owner::new();
        owner.with(|| {
            let status = ServerStatus::new();

            // Accumulate some failures (below threshold)
            status.failures.store(1, Ordering::Relaxed);
            status.report_success();

            assert_eq!(status.failures.load(Ordering::Relaxed), 0);
            assert!(status.reachable.get_untracked());
        });
    }

    #[test]
    fn test_report_failure_threshold() {
        let owner = Owner::new();
        owner.with(|| {
            let status = ServerStatus::new();
            // Prevent probe from actually spawning WASM tasks
            status.probing.store(true, Ordering::Relaxed);

            // First failure: still reachable
            status.failures.store(0, Ordering::Relaxed);
            let count = status.failures.fetch_add(1, Ordering::Relaxed) + 1;
            assert_eq!(count, 1);
            assert!(count < UNREACHABLE_THRESHOLD);

            // At threshold: should flip unreachable
            status
                .failures
                .store(UNREACHABLE_THRESHOLD - 1, Ordering::Relaxed);
            status.report_failure();
            assert!(!status.reachable.get_untracked());
        });
    }

    #[test]
    fn test_report_failure_idempotent() {
        let owner = Owner::new();
        owner.with(|| {
            let status = ServerStatus::new();
            // Pre-set probing to true so start_probe is a no-op guard
            status.probing.store(true, Ordering::Relaxed);

            // Push past threshold
            status
                .failures
                .store(UNREACHABLE_THRESHOLD - 1, Ordering::Relaxed);
            status.report_failure();
            assert!(!status.reachable.get_untracked());

            // Further failures should not panic or change state unexpectedly
            status.report_failure();
            status.report_failure();
            assert!(!status.reachable.get_untracked());
        });
    }

    #[test]
    fn test_success_after_unreachable() {
        let owner = Owner::new();
        owner.with(|| {
            let status = ServerStatus::new();
            // Simulate unreachable state
            status.reachable.set(false);
            status.failures.store(5, Ordering::Relaxed);

            status.report_success();

            assert!(status.reachable.get_untracked());
            assert_eq!(status.failures.load(Ordering::Relaxed), 0);
        });
    }
}
