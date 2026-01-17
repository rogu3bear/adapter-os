#![allow(clippy::collapsible_if)]

use adapteros_boot::{ensure_runtime_dir, EXIT_CONFIG_ERROR};
use adapteros_config::ConfigLoader;
use adapteros_server::boot::{bind_error_exit_code, precheck_tcp_port, BindError};
use std::net::TcpListener;

#[test]
fn missing_manifest_returns_error() {
    let loader = ConfigLoader::new();
    let err = loader
        .load(vec![], Some("configs/does-not-exist.toml".to_string()))
        .expect_err("missing manifest should return an error");
    assert!(err.to_string().contains("not found"));
}

#[test]
fn invalid_manifest_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let manifest_path = dir.path().join("invalid.toml");
    std::fs::write(&manifest_path, "not = [=toml").unwrap();

    let loader = ConfigLoader::new();
    let err = loader
        .load(vec![], Some(manifest_path.to_string_lossy().to_string()))
        .expect_err("invalid manifest should return an error");
    assert!(err.to_string().contains("Invalid TOML"));
}

#[test]
fn invalid_env_value_uses_default() {
    let original = std::env::var("AOS_SERVER_PORT").ok();
    std::env::set_var("AOS_SERVER_PORT", "not-a-port");

    let loader = ConfigLoader::new();
    let config = loader
        .load(vec![], None)
        .expect("env fallback should succeed");

    assert_eq!(config.get("server.port"), Some(&"8080".to_string()));

    if let Some(val) = original {
        std::env::set_var("AOS_SERVER_PORT", val);
    } else {
        std::env::remove_var("AOS_SERVER_PORT");
    }
}

#[cfg(unix)]
#[test]
fn read_only_dir_switches_to_fallback() {
    use std::os::unix::fs::PermissionsExt;

    let original_var_dir = std::env::var("AOS_VAR_DIR").ok();
    let preferred = tempfile::tempdir().unwrap();
    let fallback = tempfile::tempdir().unwrap();

    let preferred_path = preferred.path().join("ro");
    std::fs::create_dir_all(&preferred_path).unwrap();
    std::fs::set_permissions(&preferred_path, std::fs::Permissions::from_mode(0o555)).unwrap();

    let runtime = ensure_runtime_dir(preferred_path.as_path(), Some(fallback.path()))
        .expect("fallback runtime dir should be used");

    // Restore permissions so tempdir cleanup succeeds.
    std::fs::set_permissions(&preferred_path, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(runtime.used_fallback);
    assert_eq!(runtime.path, fallback.path());

    if let Some(val) = original_var_dir {
        std::env::set_var("AOS_VAR_DIR", val);
    } else {
        std::env::remove_var("AOS_VAR_DIR");
    }
}

#[cfg(unix)]
#[test]
fn disk_full_switches_to_fallback() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::Path;

    let original_var_dir = std::env::var("AOS_VAR_DIR").ok();
    let preferred = tempfile::tempdir().unwrap();
    let fallback = tempfile::tempdir().unwrap();

    let preferred_path = preferred.path().join("full");
    std::fs::create_dir_all(&preferred_path).unwrap();

    // Force a write failure: prefer /dev/full if present, otherwise revoke write perms.
    let probe_path = preferred_path.join(".aos-permcheck");
    let mut used_dev_full = false;
    if Path::new("/dev/full").exists() {
        if symlink("/dev/full", &probe_path).is_ok() {
            used_dev_full = true;
        }
    }
    if !used_dev_full {
        std::fs::set_permissions(&preferred_path, std::fs::Permissions::from_mode(0o555)).unwrap();
    }

    let runtime = ensure_runtime_dir(preferred_path.as_path(), Some(fallback.path()))
        .expect("fallback runtime dir should be used when writes fail");

    // Clean up probes/permissions for tempdir teardown.
    if used_dev_full {
        let _ = std::fs::remove_file(&probe_path);
    } else {
        std::fs::set_permissions(&preferred_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    assert!(runtime.used_fallback);
    assert_eq!(runtime.path, fallback.path());

    if let Some(val) = original_var_dir {
        std::env::set_var("AOS_VAR_DIR", val);
    } else {
        std::env::remove_var("AOS_VAR_DIR");
    }
}

#[test]
fn port_conflict_maps_to_exit_code() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(sock) => sock,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                eprintln!("skipping port_conflict_maps_to_exit_code: {e}");
                return;
            }
            panic!("failed to bind temporary socket: {e}");
        }
    };
    let addr = listener.local_addr().unwrap();

    let err = precheck_tcp_port(addr).expect_err("port should be reported as in use");
    match err {
        BindError::PortInUse { port, .. } => assert_eq!(port, addr.port()),
        other => panic!("unexpected error: {other:?}"),
    }

    assert_eq!(bind_error_exit_code(&err), EXIT_CONFIG_ERROR);
}
