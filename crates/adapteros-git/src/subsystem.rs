//! Git subsystem stub
//!
//! This module provides a minimal, no-op implementation of the Git subsystem
//! sufficient to satisfy the server’s expectations. It can be expanded later
//! to support repository watching, commit tracking, and event broadcasting.

use adapteros_core::Result;
use adapteros_db::Db;
use serde::{Deserialize, Serialize};

/// Configuration for the Git subsystem.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitConfig {
    /// Enable or disable the Git subsystem.
    #[serde(default)]
    pub enabled: bool,
}

/// Git subsystem manager (stubbed)
#[derive(Debug, Default)]
pub struct GitSubsystem {
    _enabled: bool,
}

impl GitSubsystem {
    /// Construct a new Git subsystem from config and a database handle.
    /// This is an async no-op to match server expectations.
    pub async fn new(cfg: GitConfig, _db: Db) -> Result<Self> {
        Ok(Self { _enabled: cfg.enabled })
    }

    /// Start background tasks. Currently a no-op.
    pub async fn start(&mut self) -> Result<()> {
        Ok(())
    }
}
