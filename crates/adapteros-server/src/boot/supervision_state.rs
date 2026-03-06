//! Supervision state tracking for crash-vs-rebuild restart discrimination.
//!
//! The `backend-supervision.state` file records restart metadata in JSON
//! format. On each boot the server compares the current binary's mtime
//! against the stored value: if they differ the restart is classified as
//! a rebuild (not a crash) and `crash_restart_count` is reset.
//!
//! Backward compatible: if the file is in the legacy `key=value` format
//! it is parsed and migrated to JSON on the next write.

use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{info, warn};

/// JSON-serializable supervision state written to `var/run/backend-supervision.state`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SupervisionState {
    /// Increments only for actual crash restarts (resets on rebuild).
    #[serde(default)]
    pub crash_restart_count: u32,

    /// Increments for every restart regardless of cause.
    #[serde(default)]
    pub total_restart_count: u32,

    /// Cause of the most recent restart (e.g. "launchd_kickstart", "crash", "rebuild_detected").
    #[serde(default)]
    pub last_restart_cause: String,

    /// ISO-8601 timestamp of the most recent restart event.
    #[serde(default)]
    pub last_restart_ts: String,

    /// ISO-8601 timestamp of the most recent boot.
    #[serde(default)]
    pub last_boot_ts: String,

    /// ISO-8601 mtime of the server binary at the previous boot.
    #[serde(default)]
    pub binary_mtime: String,
}

impl SupervisionState {
    /// Load supervision state from `path`.
    ///
    /// Tries JSON first; if that fails, falls back to the legacy `key=value`
    /// format for backward compatibility.  Returns `Default` if the file is
    /// missing or unreadable.
    pub fn load(path: &Path) -> Self {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };

        // Try JSON first.
        if let Ok(state) = serde_json::from_str::<Self>(&content) {
            return state;
        }

        // Fallback: legacy key=value format.
        Self::parse_legacy_format(&content)
    }

    /// Parse the legacy `key=value` format into a `SupervisionState`.
    ///
    /// Maps `restart_count` to `total_restart_count` (crash count is unknown
    /// under legacy and defaults to 0).
    fn parse_legacy_format(content: &str) -> Self {
        let mut state = Self::default();
        for line in content.lines() {
            let line = line.trim();
            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "restart_count" => {
                        state.total_restart_count = value.parse().unwrap_or(0);
                    }
                    "last_restart_cause" => {
                        state.last_restart_cause = value.to_string();
                    }
                    "last_restart_ts" => {
                        state.last_restart_ts = value.to_string();
                    }
                    _ => {}
                }
            }
        }
        state
    }

    /// Atomically write state as pretty-printed JSON.
    ///
    /// Writes to a `.tmp` sibling and renames for crash safety.
    pub fn write_atomic(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Returns `true` if the stored binary mtime differs from `current_binary_mtime`,
    /// indicating a rebuild has occurred since the last boot.
    pub fn is_rebuild(&self, current_binary_mtime: &str) -> bool {
        !self.binary_mtime.is_empty() && self.binary_mtime != current_binary_mtime
    }
}

/// Get the modification time of the current server binary as an ISO-8601 string.
///
/// Returns `None` if the binary path or its metadata cannot be read.
pub fn get_server_binary_mtime() -> Option<String> {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "Failed to determine current binary path");
            return None;
        }
    };

    let meta = match std::fs::metadata(&exe) {
        Ok(m) => m,
        Err(e) => {
            warn!(path = %exe.display(), error = %e, "Failed to read binary metadata");
            return None;
        }
    };

    let mtime = match meta.modified() {
        Ok(t) => t,
        Err(e) => {
            warn!(error = %e, "Failed to read binary mtime");
            return None;
        }
    };

    let dt: chrono::DateTime<chrono::Utc> = mtime.into();
    Some(dt.to_rfc3339())
}

/// Update supervision state on boot.
///
/// Loads the existing state, compares binary mtime to detect rebuilds,
/// resets `crash_restart_count` on rebuild, records the current boot
/// timestamp, and writes atomically.  All operations are best-effort.
pub fn update_supervision_state_on_boot(var_dir: &Path) {
    let state_path = var_dir.join("run/backend-supervision.state");
    let mut state = SupervisionState::load(&state_path);

    let current_mtime = get_server_binary_mtime().unwrap_or_default();

    if state.is_rebuild(&current_mtime) {
        info!(
            old_mtime = %state.binary_mtime,
            new_mtime = %current_mtime,
            "Rebuild detected, resetting crash counter"
        );
        state.crash_restart_count = 0;
        state.last_restart_cause = "rebuild_detected".to_string();
    }

    let now: chrono::DateTime<chrono::Utc> = chrono::Utc::now();
    state.last_boot_ts = now.to_rfc3339();
    state.binary_mtime = current_mtime;

    if let Err(e) = state.write_atomic(&state_path) {
        warn!(path = %state_path.display(), error = %e, "Failed to write supervision state");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_supervision_state_json_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("state.json");

        let state = SupervisionState {
            crash_restart_count: 3,
            total_restart_count: 15,
            last_restart_cause: "launchd_kickstart".to_string(),
            last_restart_ts: "2025-06-01T12:00:00Z".to_string(),
            last_boot_ts: "2025-06-01T12:05:00Z".to_string(),
            binary_mtime: "2025-06-01T10:00:00Z".to_string(),
        };

        state.write_atomic(&path).unwrap();
        let loaded = SupervisionState::load(&path);

        assert_eq!(loaded.crash_restart_count, 3);
        assert_eq!(loaded.total_restart_count, 15);
        assert_eq!(loaded.last_restart_cause, "launchd_kickstart");
        assert_eq!(loaded.last_restart_ts, "2025-06-01T12:00:00Z");
        assert_eq!(loaded.last_boot_ts, "2025-06-01T12:05:00Z");
        assert_eq!(loaded.binary_mtime, "2025-06-01T10:00:00Z");
    }

    #[test]
    fn test_legacy_format_migration() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("state.legacy");

        let legacy = "restart_count=42\nlast_restart_cause=launchd_kickstart_missing_backend\nlast_restart_ts=2025-05-01T08:00:00+10:00\n";
        std::fs::write(&path, legacy).unwrap();

        let loaded = SupervisionState::load(&path);

        assert_eq!(loaded.total_restart_count, 42);
        assert_eq!(loaded.crash_restart_count, 0); // unknown under legacy
        assert_eq!(
            loaded.last_restart_cause,
            "launchd_kickstart_missing_backend"
        );
        assert_eq!(loaded.last_restart_ts, "2025-05-01T08:00:00+10:00");
    }

    #[test]
    fn test_rebuild_detection() {
        let state = SupervisionState {
            binary_mtime: "2025-01-01T00:00:00Z".to_string(),
            ..Default::default()
        };

        assert!(state.is_rebuild("2025-01-02T00:00:00Z"));
        assert!(!state.is_rebuild("2025-01-01T00:00:00Z"));
    }

    #[test]
    fn test_crash_restart_counted() {
        let state = SupervisionState {
            crash_restart_count: 5,
            binary_mtime: "2025-01-01T00:00:00Z".to_string(),
            ..Default::default()
        };

        // Non-rebuild: crash count should be preserved (caller doesn't modify it here).
        assert!(!state.is_rebuild("2025-01-01T00:00:00Z"));
        assert_eq!(state.crash_restart_count, 5);
    }

    #[test]
    fn test_rebuild_resets_crash_count() {
        let tmp = TempDir::new().unwrap();
        let run = tmp.path().join("run");
        std::fs::create_dir_all(&run).unwrap();

        let state_path = run.join("backend-supervision.state");

        // Write state with high crash count and a known binary mtime.
        let state = SupervisionState {
            crash_restart_count: 10,
            total_restart_count: 50,
            binary_mtime: "2025-01-01T00:00:00Z".to_string(),
            ..Default::default()
        };
        state.write_atomic(&state_path).unwrap();

        // update_supervision_state_on_boot will use the REAL binary mtime
        // which will differ from "2025-01-01T00:00:00Z", triggering rebuild detection.
        update_supervision_state_on_boot(tmp.path());

        let loaded = SupervisionState::load(&state_path);
        assert_eq!(
            loaded.crash_restart_count, 0,
            "Rebuild should reset crash count"
        );
        assert_eq!(loaded.last_restart_cause, "rebuild_detected");
    }

    #[test]
    fn test_atomic_write_creates_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("new-state.json");

        assert!(!path.exists());

        let state = SupervisionState {
            crash_restart_count: 1,
            total_restart_count: 1,
            last_restart_cause: "test".to_string(),
            ..Default::default()
        };
        state.write_atomic(&path).unwrap();

        assert!(path.exists());

        // Verify it's valid JSON.
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["crash_restart_count"], 1);
    }
}
