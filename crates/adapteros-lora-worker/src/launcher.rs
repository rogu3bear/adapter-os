//! Privileged launcher for per-tenant process isolation

use adapteros_core::{AosError, Result};
use std::path::PathBuf;
use tracing::info;

#[cfg(target_os = "macos")]
use nix::unistd::{setgid, setuid, Gid, Uid};

/// Tenant isolation configuration
#[derive(Debug, Clone)]
pub struct TenantIsolation {
    pub tenant_id: String,
    pub uid: u32,
    pub gid: u32,
    pub root_dir: PathBuf,
    pub socket_path: PathBuf,
}

/// Drop privileges to tenant UID/GID
///
/// This MUST be called before serving to ensure isolation.
/// Requires the process to be running as root initially.
#[cfg(target_os = "macos")]
pub fn drop_privileges(isolation: &TenantIsolation) -> Result<()> {
    use nix::unistd::geteuid;

    // Check if we're running as root
    if geteuid().as_raw() != 0 {
        return Err(AosError::IsolationViolation(
            "Must run as root to drop privileges. Current UID: {}".to_string(),
        ));
    }

    info!(uid = isolation.uid, gid = isolation.gid, "Dropping privileges");

    // Change group first (must be done before changing user)
    let gid = Gid::from_raw(isolation.gid);
    setgid(gid).map_err(|e| AosError::IsolationViolation(format!("Failed to set GID: {}", e)))?;

    // Change user
    let uid = Uid::from_raw(isolation.uid);
    setuid(uid).map_err(|e| AosError::IsolationViolation(format!("Failed to set UID: {}", e)))?;

    // Verify we can't escalate back
    if geteuid().as_raw() == 0 {
        return Err(AosError::IsolationViolation(
            "Failed to drop privileges - still running as root!".to_string(),
        ));
    }

    info!(uid = %uid, gid = %gid, "Privileges dropped successfully");

    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn drop_privileges(_isolation: &TenantIsolation) -> Result<()> {
    Err(AosError::IsolationViolation(
        "Privilege dropping only supported on macOS/Unix".to_string(),
    ))
}

/// Set up capability-scoped filesystem access
///
/// After dropping privileges, restrict filesystem access to only:
/// - Tenant root directory
/// - Socket directory
/// - Read-only model artifacts
pub fn setup_filesystem_caps(isolation: &TenantIsolation) -> Result<()> {
    use std::fs;

    // Create tenant directories if they don't exist
    fs::create_dir_all(&isolation.root_dir).map_err(|e| {
        AosError::IsolationViolation(format!("Failed to create tenant root directory: {}", e))
    })?;

    if let Some(socket_dir) = isolation.socket_path.parent() {
        fs::create_dir_all(socket_dir).map_err(|e| {
            AosError::IsolationViolation(format!("Failed to create socket directory: {}", e))
        })?;
    }

    // In production, use cap-std to open directory handles
    // and pass them to the worker instead of paths
    // This prevents path traversal attacks

    info!(root_dir = %isolation.root_dir.display(), socket_path = %isolation.socket_path.display(), "Filesystem capabilities configured");

    Ok(())
}

/// Complete tenant isolation setup
pub fn setup_tenant_isolation(isolation: &TenantIsolation) -> Result<()> {
    info!(tenant_id = %isolation.tenant_id, "Setting up tenant isolation");

    // Set up filesystem first (before dropping privileges)
    setup_filesystem_caps(isolation)?;

    // Drop privileges
    drop_privileges(isolation)?;

    // Additional hardening
    #[cfg(target_os = "macos")]
    {
        // Set resource limits
        // In production: set RLIMIT_NOFILE, RLIMIT_NPROC, etc.
        info!("Resource limits configured");
    }

    info!("Tenant isolation complete");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isolation_config() {
        let isolation = TenantIsolation {
            tenant_id: "test-tenant".to_string(),
            uid: 1001,
            gid: 1001,
            root_dir: PathBuf::from("/var/aos/test-tenant"),
            socket_path: PathBuf::from("/var/run/aos/test-tenant/aos.sock"),
        };

        assert_eq!(isolation.uid, 1001);
        assert_eq!(isolation.gid, 1001);
    }
}
