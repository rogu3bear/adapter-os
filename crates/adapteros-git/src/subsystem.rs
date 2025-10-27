//! Git subsystem stub
//!
//! The GitSubsystem has been temporarily stubbed out to resolve feature conflicts.
//! Full implementation will be provided in a future iteration.

use adapteros_core::Result;

/// Git subsystem manager (stubbed)
pub struct GitSubsystem;

impl GitSubsystem {
    /// Create a new Git subsystem
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl Default for GitSubsystem {
    fn default() -> Self {
        Self
    }
}

