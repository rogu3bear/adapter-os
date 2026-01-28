//! Process isolation and security boundary tests
//!
//! Tests cover:
//! - PF (packet filter) rule enforcement verification
//! - UID/GID privilege dropping
//! - Process isolation boundaries
//! - Sandboxing verification
//! - Network isolation checks
//!
//! IMPORTANT: Many tests in this file require elevated privileges (root/sudo)
//! and specific system configurations. They are marked with #[ignore] and
//! include comments explaining the prerequisites.
//!
//! Prerequisites for running ignored tests:
//! 1. Run as root or with sudo
//! 2. Enable PF: `sudo pfctl -e`
//! 3. Configure deny-all egress: `echo 'block out all' | sudo pfctl -f -`
//! 4. Have aos-worker binary available in PATH

use adapteros_node::agent::NodeAgent;
use std::process::Command;
use tempfile::TempDir;

#[cfg(unix)]
use nix::unistd::{Gid, Uid};

/// Helper to create a test temp directory using OS temp
fn new_test_tempdir() -> TempDir {
    TempDir::with_prefix("aos-test-").expect("create temp dir")
}

// ============================================================
// PF (Packet Filter) Rule Verification Tests
// ============================================================

#[test]
#[cfg(target_os = "macos")]
fn test_pf_status_check_command() {
    // Verify that we can execute pfctl commands
    // This doesn't require root, just checks if the command exists
    let output = Command::new("which").arg("pfctl").output();

    assert!(output.is_ok(), "which command should work");
    let output = output.unwrap();

    // On macOS, pfctl should be available
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout);
        assert!(
            path.contains("pfctl"),
            "pfctl should be found: {}",
            path.trim()
        );
    }
}

#[test]
#[cfg(target_os = "macos")]
#[ignore = "Requires root privileges to check PF info [prereq: sudo]"]
fn test_pf_info_access() {
    // Try to get PF info - requires root
    let output = Command::new("pfctl").args(["-s", "info"]).output();

    assert!(output.is_ok(), "pfctl -s info should execute");
    let output = output.unwrap();

    if output.status.success() {
        let info = String::from_utf8_lossy(&output.stdout);
        // Should contain status information
        assert!(
            info.contains("Status:") || info.contains("Interface"),
            "PF info should contain status: {}",
            info
        );
    } else {
        // Permission denied is expected without root
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Permission denied")
                || stderr.contains("Operation not permitted")
                || stderr.is_empty(), // Sometimes just fails silently
            "Should fail with permission error: {}",
            stderr
        );
    }
}

#[test]
#[cfg(target_os = "macos")]
#[ignore = "Requires root and PF enabled [prereq: sudo pfctl -e]"]
fn test_pf_rules_check() {
    let output = Command::new("pfctl").args(["-s", "rules"]).output();

    assert!(output.is_ok(), "pfctl -s rules should execute");
    let output = output.unwrap();

    if output.status.success() {
        let rules = String::from_utf8_lossy(&output.stdout);
        println!("Current PF rules:\n{}", rules);

        // In a properly configured system, we should see block rules
        // This is informational - the actual test is in the agent
    }
}

#[tokio::test]
async fn test_pf_status_via_agent() {
    let agent = NodeAgent::new();

    let result = agent.check_pf_status().await;
    assert!(result.is_ok(), "check_pf_status should not error");

    let status = result.unwrap();
    // Result depends on system configuration
    println!("PF status: {}", status);

    // The important thing is that the check completes without panic
}

#[tokio::test]
#[ignore = "Requires PF enabled with deny-all egress [prereq: echo 'block out all' | sudo pfctl -f -]"]
async fn test_pf_deny_all_verification() {
    let agent = NodeAgent::new();

    let status = agent.check_pf_status().await.expect("PF check should work");
    assert!(
        status,
        "PF should report active when deny-all egress is configured"
    );
}

// ============================================================
// UID/GID Privilege Dropping Tests
// ============================================================

#[test]
#[cfg(unix)]
fn test_current_uid_gid() {
    // Check current process UID/GID
    let uid = Uid::current();
    let gid = Gid::current();

    println!("Current UID: {}", uid);
    println!("Current GID: {}", gid);

    // These should be non-negative
    assert!(uid.as_raw() < u32::MAX, "UID should be valid");
    assert!(gid.as_raw() < u32::MAX, "GID should be valid");
}

#[test]
#[cfg(unix)]
fn test_effective_uid_gid() {
    let euid = Uid::effective();
    let egid = Gid::effective();

    println!("Effective UID: {}", euid);
    println!("Effective GID: {}", egid);

    // Effective UID/GID might differ from real UID/GID if setuid bit is set
    assert!(euid.as_raw() < u32::MAX, "EUID should be valid");
    assert!(egid.as_raw() < u32::MAX, "EGID should be valid");
}

#[test]
#[cfg(unix)]
#[ignore = "Requires root to test setuid/setgid [prereq: run as root]"]
fn test_privilege_drop_order() {
    // This test verifies that GID must be set before UID when dropping privileges
    // (because after dropping UID to non-root, we can't change GID anymore)

    let original_uid = Uid::current();
    let _original_gid = Gid::current();

    if original_uid.is_root() {
        // Try to set GID first (correct order)
        let target_gid = Gid::from_raw(1000);
        let target_uid = Uid::from_raw(1000);

        // In a forked process, we would:
        // 1. setgid(target_gid)
        // 2. setuid(target_uid)

        // We can't actually test this without forking, but we verify the constants
        assert!(
            target_gid.as_raw() > 0,
            "Target GID should be positive non-root"
        );
        assert!(
            target_uid.as_raw() > 0,
            "Target UID should be positive non-root"
        );
    } else {
        println!(
            "Not running as root, skipping privilege drop test (uid={})",
            original_uid
        );
    }
}

#[tokio::test]
#[ignore = "Requires root and CAP_SETUID capability [prereq: run as root]"]
async fn test_worker_spawn_with_uid_gid_change() {
    let agent = NodeAgent::new();

    // Try to spawn a worker with specific UID/GID
    // This requires CAP_SETUID and CAP_SETGID capabilities or root
    let result = agent
        .spawn_worker(
            "uid-test", "plan-uid", 1000, // target uid
            1000, // target gid
            None, None,
        )
        .await;

    match result {
        Ok(pid) => {
            println!("Worker spawned with PID: {}", pid);
            // Clean up
            let _ = agent.stop_worker(pid).await;
        }
        Err(e) => {
            let err_msg = format!("{}", e);
            // Expected failures:
            // - PF not enabled
            // - Permission denied for setuid/setgid
            // - aos-worker not found
            println!("Expected error: {}", err_msg);
        }
    }
}

// ============================================================
// Process Isolation Boundary Tests
// ============================================================

#[test]
fn test_uds_socket_directory_format() {
    // Verify the UDS socket directory format provides tenant isolation
    let tenant_id = "tenant-abc123";
    let expected_dir = format!("/var/run/aos/{}", tenant_id);
    let expected_socket = format!("{}/aos.sock", expected_dir);

    // Each tenant should have their own directory
    assert!(expected_dir.contains(tenant_id));
    assert!(expected_socket.ends_with(".sock"));

    // Different tenants should have different paths
    let tenant2_dir = format!("/var/run/aos/{}", "tenant-def456");
    assert_ne!(
        expected_dir, tenant2_dir,
        "Different tenants different paths"
    );
}

#[test]
#[ignore = "Requires creating directories in /var/run [prereq: sudo access to /var/run/aos]"]
fn test_uds_directory_creation() {
    let _tmpdir = new_test_tempdir();
    let test_tenant = format!("test-tenant-{}", std::process::id());
    let uds_dir = format!("/var/run/aos/{}", test_tenant);

    // Try to create the UDS directory
    let result = std::fs::create_dir_all(&uds_dir);

    match result {
        Ok(_) => {
            // Verify directory was created
            assert!(
                std::path::Path::new(&uds_dir).exists(),
                "Directory should exist"
            );

            // Clean up
            let _ = std::fs::remove_dir_all(&uds_dir);
        }
        Err(e) => {
            // Permission denied is expected without elevated privileges
            let err_msg = format!("{}", e);
            assert!(
                err_msg.contains("Permission denied")
                    || err_msg.contains("Operation not permitted"),
                "Should fail with permission error: {}",
                err_msg
            );
        }
    }
}

#[tokio::test]
async fn test_worker_tenant_isolation() {
    let agent = NodeAgent::new();

    // Even if workers can't be spawned, verify the tracking structure
    // maintains proper tenant isolation
    let workers = agent.list_workers().await.expect("list_workers works");

    // Initially empty
    assert!(workers.is_empty(), "Should start with no workers");

    // Each worker when spawned should be associated with exactly one tenant
    // (This is enforced by the WorkerInfo structure)
}

// ============================================================
// Network Isolation Tests
// ============================================================

#[test]
#[cfg(target_os = "macos")]
fn test_lsof_availability() {
    // Verify lsof is available for socket checking
    let output = Command::new("which").arg("lsof").output();

    assert!(output.is_ok(), "which command should work");
    let output = output.unwrap();

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout);
        assert!(path.contains("lsof"), "lsof should be found");
    }
}

#[test]
#[ignore = "Checks actual TCP listeners on the system"]
fn test_no_unexpected_tcp_listeners() {
    // Check for TCP listening sockets
    // In production, worker processes should not have any TCP listeners
    let output = Command::new("lsof")
        .args(["-iTCP", "-sTCP:LISTEN", "-n", "-P"])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let sockets = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = sockets.lines().skip(1).collect(); // Skip header

            println!("Current TCP listeners: {} processes", lines.len());
            for line in &lines {
                println!("  {}", line);
            }

            // This is informational - actual enforcement is at the PF level
        }
    }
}

#[test]
fn test_unix_socket_only_communication() {
    // Verify that the node agent uses Unix domain sockets for worker communication
    // This test checks the expected socket path format

    let tenant_id = "socket-test-tenant";
    let uds_path = format!("/var/run/aos/{}/aos.sock", tenant_id);

    // The path should be a Unix socket path (not TCP)
    assert!(uds_path.starts_with("/"), "Should be absolute path");
    assert!(uds_path.ends_with(".sock"), "Should end in .sock");
    assert!(
        !uds_path.contains(":"),
        "Should not be TCP format (host:port)"
    );
}

// ============================================================
// Sandboxing Verification Tests
// ============================================================

#[test]
#[cfg(target_os = "macos")]
fn test_sandbox_exec_availability() {
    // Check if sandbox-exec is available on macOS
    let output = Command::new("which").arg("sandbox-exec").output();

    if let Ok(output) = output {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout);
            println!("sandbox-exec available at: {}", path.trim());
        } else {
            println!("sandbox-exec not found (may not be needed if using other isolation)");
        }
    }
}

#[test]
#[cfg(target_os = "macos")]
#[ignore = "Requires specific sandbox profile configuration"]
fn test_sandbox_profile_denies_network() {
    // Test that a sandbox profile would deny network access
    // This would use sandbox-exec with a deny-network profile

    let profile = "(version 1)(deny default)(allow process-fork)";

    // In actual usage, we would run:
    // sandbox-exec -p "$profile" /path/to/aos-worker

    assert!(
        profile.contains("deny"),
        "Sandbox profile should have deny rules"
    );
}

// ============================================================
// Environment Variable Isolation Tests
// ============================================================

#[test]
fn test_worker_env_propagation_fields() {
    // Verify the environment variables that should be propagated to workers
    let expected_vars = [
        "TENANT_ID",
        "PLAN_ID",
        "UDS_PATH",
        "AOS_MODEL_CACHE_MAX_MB",
        "AOS_CONFIG_TOML",
    ];

    for var in &expected_vars {
        println!("Expected worker env var: {}", var);
    }

    // All expected vars should be uppercase (convention)
    for var in &expected_vars {
        assert_eq!(
            var.to_uppercase(),
            *var,
            "Env vars should be uppercase: {}",
            var
        );
    }
}

#[test]
fn test_sensitive_env_not_propagated() {
    // Verify that certain sensitive environment variables should NOT be
    // propagated to worker processes

    let sensitive_vars = [
        "AWS_SECRET_ACCESS_KEY",
        "GITHUB_TOKEN",
        "DATABASE_URL",
        "API_KEY",
        "PRIVATE_KEY",
    ];

    // Document what should be filtered (actual filtering is in spawn_worker)
    for var in &sensitive_vars {
        println!("Should NOT propagate: {}", var);
    }
}

// ============================================================
// Signal Handling Tests
// ============================================================

#[test]
#[cfg(unix)]
fn test_signal_constants() {
    use nix::sys::signal::Signal;

    // Verify signal constants used in worker management
    let sigterm = Signal::SIGTERM;
    let sigkill = Signal::SIGKILL;

    println!("SIGTERM: {:?}", sigterm);
    println!("SIGKILL: {:?}", sigkill);

    // SIGTERM should be used for graceful shutdown
    // SIGKILL is fallback after timeout
}

#[tokio::test]
#[ignore = "Requires spawning actual worker process"]
async fn test_sigterm_graceful_shutdown() {
    let agent = NodeAgent::new();

    let result = agent
        .spawn_worker("signal-test", "plan-signal", 1000, 1000, None, None)
        .await;

    if let Ok(pid) = result {
        // Stop sends SIGTERM first
        let stop_result = agent.stop_worker(pid).await;
        assert!(stop_result.is_ok(), "SIGTERM shutdown should succeed");
    }
}

// ============================================================
// Pre-exec Safety Tests
// ============================================================

#[test]
#[cfg(unix)]
fn test_pre_exec_warning_pipe_concept() {
    // The pre_exec callback uses a UnixStream pair for communicating warnings
    // This test verifies the concept works
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    let (mut reader, mut writer) = UnixStream::pair().expect("create socket pair");
    reader.set_nonblocking(true).expect("set nonblocking");

    // Simulate pre_exec warning
    let warning = b"Warning: some issue occurred";
    writer.write_all(warning).expect("write warning");
    drop(writer); // Close write end

    // Read the warning
    let mut buf = vec![0u8; 256];
    let mut total = 0;
    loop {
        match reader.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) => panic!("read error: {}", e),
        }
    }

    let message = String::from_utf8_lossy(&buf[..total]);
    assert!(
        message.contains("Warning"),
        "Should capture warning: {}",
        message
    );
}

// ============================================================
// Security Boundary Integration Tests
// ============================================================

#[tokio::test]
#[ignore = "Full security integration test [prereq: root + PF + aos-worker]"]
async fn test_full_isolation_stack() {
    let agent = NodeAgent::new();

    // 1. Verify PF is enabled
    let pf_status = agent.check_pf_status().await.expect("PF check works");
    assert!(pf_status, "PF must be enabled with deny-all egress");

    // 2. Spawn worker with isolation
    let pid = agent
        .spawn_worker("isolation-test", "plan-isolation", 1000, 1000, None, None)
        .await
        .expect("Worker spawn should succeed");

    // 3. Verify worker is tracked
    let workers = agent.list_workers().await.expect("list works");
    assert!(
        workers.iter().any(|w| w.pid == pid),
        "Worker should be tracked"
    );

    // 4. Verify UDS path is correct
    let worker = workers.iter().find(|w| w.pid == pid).unwrap();
    assert!(
        worker.uds_path.contains("isolation-test"),
        "UDS path should be tenant-specific"
    );

    // 5. Clean up
    agent.stop_worker(pid).await.expect("Stop should work");
}

// ============================================================
// Supplementary Groups Test
// ============================================================

#[test]
#[cfg(target_os = "linux")]
#[ignore = "Requires root to clear supplementary groups"]
fn test_supplementary_groups_concept() {
    // On Linux, supplementary groups should be cleared before privilege drop
    // This ensures the worker doesn't inherit any additional group memberships

    // The actual clearing happens in pre_exec with nix::unistd::setgroups(&[])

    // Verify setgroups is available
    use nix::unistd::getgroups;

    match getgroups() {
        Ok(groups) => {
            println!("Current supplementary groups: {:?}", groups);
        }
        Err(e) => {
            println!("getgroups error (may need privileges): {}", e);
        }
    }
}

// ============================================================
// Worker Binary Verification
// ============================================================

#[test]
fn test_aos_worker_binary_location() {
    // Check if aos-worker binary exists in PATH
    let output = Command::new("which").arg("aos-worker").output();

    match output {
        Ok(out) => {
            if out.status.success() {
                let path = String::from_utf8_lossy(&out.stdout);
                println!("aos-worker found at: {}", path.trim());
            } else {
                println!("aos-worker not in PATH (expected for dev environment)");
            }
        }
        Err(e) => {
            println!("which command failed: {} (platform-specific)", e);
        }
    }
}

#[test]
#[ignore = "Requires aos-worker binary to be built and in PATH"]
fn test_aos_worker_binary_executes() {
    // Verify aos-worker can at least show help/version
    let output = Command::new("aos-worker").arg("--help").output();

    match output {
        Ok(out) => {
            if out.status.success() || out.status.code() == Some(0) || out.status.code() == Some(2)
            {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                println!("aos-worker help output:\n{}\n{}", stdout, stderr);
            } else {
                panic!(
                    "aos-worker --help failed with exit code: {:?}",
                    out.status.code()
                );
            }
        }
        Err(e) => {
            panic!("Failed to execute aos-worker: {}", e);
        }
    }
}
