//! Background task spawner for AdapterOS control plane.
//!
//! This module provides a unified interface for spawning and managing background
//! tasks during the boot sequence. It consolidates the common pattern of:
//! 1. Spawning a deterministic task
//! 2. Registering it with the shutdown coordinator
//! 3. Logging success/failure
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_server::boot::BackgroundTaskSpawner;
//!
//! let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator);
//!
//! // Spawn a task with automatic registration
//! spawner.spawn("Status writer", async move {
//!     // task logic
//! }).await;
//!
//! // Get the coordinator back when done
//! let coordinator = spawner.into_coordinator();
//! ```

use crate::shutdown::ShutdownCoordinator;
use adapteros_deterministic_exec::spawn_deterministic;
use std::future::Future;
use tracing::{error, info, warn};

/// Result type for spawn operations.
pub type SpawnResult = Result<(), SpawnError>;

/// Error type for spawn failures.
#[derive(Debug, Clone)]
pub struct SpawnError {
    pub task_name: String,
    pub message: String,
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to spawn {}: {}", self.task_name, self.message)
    }
}

impl std::error::Error for SpawnError {}

/// Manages background task spawning during boot.
///
/// Provides a unified interface for spawning deterministic background tasks
/// and registering them with the shutdown coordinator.
pub struct BackgroundTaskSpawner {
    /// The shutdown coordinator that receives task handles
    shutdown_coordinator: ShutdownCoordinator,
    /// Names of successfully spawned tasks (for diagnostics)
    spawned_tasks: Vec<String>,
    /// Names of tasks that failed to spawn
    failed_tasks: Vec<String>,
}

impl BackgroundTaskSpawner {
    /// Create a new background task spawner.
    ///
    /// # Arguments
    /// * `shutdown_coordinator` - The coordinator that will manage task shutdown
    pub fn new(shutdown_coordinator: ShutdownCoordinator) -> Self {
        Self {
            shutdown_coordinator,
            spawned_tasks: Vec::new(),
            failed_tasks: Vec::new(),
        }
    }

    /// Spawn a background task with automatic registration.
    ///
    /// This is the core spawning method. It:
    /// 1. Spawns the task using `spawn_deterministic`
    /// 2. Registers the handle with the shutdown coordinator
    /// 3. Logs success or failure
    /// 4. Tracks the task name for diagnostics
    ///
    /// # Arguments
    /// * `name` - Human-readable task name for logging
    /// * `future` - The async task to spawn
    ///
    /// # Returns
    /// `Ok(())` if the task was spawned successfully, `Err` otherwise.
    pub fn spawn<F>(&mut self, name: &str, future: F) -> SpawnResult
    where
        F: Future<Output = ()> + Send + 'static,
    {
        match spawn_deterministic(name.to_string(), future) {
            Ok(handle) => {
                self.shutdown_coordinator.register_task(handle);
                self.spawned_tasks.push(name.to_string());
                info!(task = %name, "Background task started");
                Ok(())
            }
            Err(e) => {
                let err = SpawnError {
                    task_name: name.to_string(),
                    message: e.to_string(),
                };
                self.failed_tasks.push(name.to_string());
                error!(task = %name, error = %e, "Failed to spawn background task");
                Err(err)
            }
        }
    }

    /// Spawn a background task, logging a warning on failure instead of error.
    ///
    /// Use this for non-critical tasks where failure is acceptable.
    ///
    /// # Arguments
    /// * `name` - Human-readable task name for logging
    /// * `future` - The async task to spawn
    /// * `fallback_msg` - Message to log explaining what happens without this task
    pub fn spawn_optional<F>(&mut self, name: &str, future: F, fallback_msg: &str) -> SpawnResult
    where
        F: Future<Output = ()> + Send + 'static,
    {
        match spawn_deterministic(name.to_string(), future) {
            Ok(handle) => {
                self.shutdown_coordinator.register_task(handle);
                self.spawned_tasks.push(name.to_string());
                info!(task = %name, "Background task started");
                Ok(())
            }
            Err(e) => {
                let err = SpawnError {
                    task_name: name.to_string(),
                    message: e.to_string(),
                };
                self.failed_tasks.push(name.to_string());
                warn!(task = %name, error = %e, fallback = %fallback_msg, "Failed to spawn optional background task");
                Err(err)
            }
        }
    }

    /// Spawn a task with detailed success logging.
    ///
    /// Use this when you want to include additional context in the success log.
    ///
    /// # Arguments
    /// * `name` - Human-readable task name
    /// * `future` - The async task to spawn
    /// * `success_details` - Additional details to log on success
    pub fn spawn_with_details<F>(
        &mut self,
        name: &str,
        future: F,
        success_details: &str,
    ) -> SpawnResult
    where
        F: Future<Output = ()> + Send + 'static,
    {
        match spawn_deterministic(name.to_string(), future) {
            Ok(handle) => {
                self.shutdown_coordinator.register_task(handle);
                self.spawned_tasks.push(name.to_string());
                info!(task = %name, details = %success_details, "Background task started");
                Ok(())
            }
            Err(e) => {
                let err = SpawnError {
                    task_name: name.to_string(),
                    message: e.to_string(),
                };
                self.failed_tasks.push(name.to_string());
                error!(task = %name, error = %e, "Failed to spawn background task");
                Err(err)
            }
        }
    }

    /// Get a mutable reference to the shutdown coordinator.
    ///
    /// Use this when you need to register handles from external spawning
    /// mechanisms (e.g., policy watcher, federation daemon).
    pub fn coordinator_mut(&mut self) -> &mut ShutdownCoordinator {
        &mut self.shutdown_coordinator
    }

    /// Get a reference to the shutdown coordinator.
    pub fn coordinator(&self) -> &ShutdownCoordinator {
        &self.shutdown_coordinator
    }

    /// Get the list of successfully spawned task names.
    pub fn spawned_tasks(&self) -> &[String] {
        &self.spawned_tasks
    }

    /// Get the list of failed task names.
    pub fn failed_tasks(&self) -> &[String] {
        &self.failed_tasks
    }

    /// Check if any tasks failed to spawn.
    pub fn has_failures(&self) -> bool {
        !self.failed_tasks.is_empty()
    }

    /// Get the total number of successfully spawned tasks.
    pub fn spawned_count(&self) -> usize {
        self.spawned_tasks.len()
    }

    /// Log a summary of all spawned tasks.
    pub fn log_summary(&self) {
        info!(
            target: "boot",
            spawned = self.spawned_tasks.len(),
            failed = self.failed_tasks.len(),
            "Background task spawning complete"
        );

        if !self.failed_tasks.is_empty() {
            warn!(
                target: "boot",
                tasks = ?self.failed_tasks,
                "Some background tasks failed to spawn"
            );
        }
    }

    /// Consume the spawner and return the shutdown coordinator.
    ///
    /// Call this when all background tasks have been spawned.
    pub fn into_coordinator(self) -> ShutdownCoordinator {
        self.shutdown_coordinator
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shutdown::ShutdownCoordinator;

    #[tokio::test]
    async fn test_spawner_tracks_tasks() {
        let coordinator = ShutdownCoordinator::new();
        let mut spawner = BackgroundTaskSpawner::new(coordinator);

        // Note: spawn_deterministic requires the executor to be initialized,
        // which won't work in unit tests. This test just verifies the struct works.
        assert_eq!(spawner.spawned_count(), 0);
        assert!(!spawner.has_failures());
        assert!(spawner.spawned_tasks().is_empty());
        assert!(spawner.failed_tasks().is_empty());
    }

    #[test]
    fn test_spawn_error_display() {
        let err = SpawnError {
            task_name: "Test task".to_string(),
            message: "executor not initialized".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Failed to spawn Test task: executor not initialized"
        );
    }
}
