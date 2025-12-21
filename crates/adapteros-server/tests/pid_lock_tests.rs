//! Tests for PID file lock functionality.

use adapteros_server::pid_lock::PidFileLock;
use tempfile::tempdir;

#[test]
fn test_lock_acquisition_succeeds_when_no_existing_lock() {
    let dir = tempdir().unwrap();
    let pid_path = dir.path().join("test.pid");

    let lock = PidFileLock::acquire(Some(pid_path.clone()));
    assert!(lock.is_ok(), "Lock acquisition should succeed");

    // Verify PID file was created with current process ID
    let contents = std::fs::read_to_string(&pid_path).unwrap();
    assert_eq!(
        contents,
        std::process::id().to_string(),
        "PID file should contain current process ID"
    );
}

#[test]
fn test_lock_acquisition_fails_when_process_exists() {
    let dir = tempdir().unwrap();
    let pid_path = dir.path().join("test.pid");

    // Write current process's PID to simulate existing lock
    std::fs::write(&pid_path, std::process::id().to_string()).unwrap();

    let result = PidFileLock::acquire(Some(pid_path));
    assert!(
        result.is_err(),
        "Lock acquisition should fail when process exists"
    );

    let err = result.err().expect("Should be an error");
    assert!(
        err.to_string().contains("Another aos-cp process is running"),
        "Error should mention existing process"
    );
}

#[test]
fn test_lock_file_cleaned_up_on_drop() {
    let dir = tempdir().unwrap();
    let pid_path = dir.path().join("test.pid");

    {
        let lock = PidFileLock::acquire(Some(pid_path.clone()));
        assert!(lock.is_ok(), "Lock acquisition should succeed");
        assert!(pid_path.exists(), "PID file should exist while lock held");
    }

    // After drop, file should be removed
    assert!(
        !pid_path.exists(),
        "PID file should be cleaned up after drop"
    );
}

#[test]
fn test_process_exists_returns_false_for_invalid_pid() {
    let dir = tempdir().unwrap();
    let pid_path = dir.path().join("test.pid");

    // Write an invalid PID string
    std::fs::write(&pid_path, "not_a_number").unwrap();

    // Acquisition should succeed because invalid PID is treated as non-existent
    let result = PidFileLock::acquire(Some(pid_path));
    assert!(
        result.is_ok(),
        "Lock acquisition should succeed for invalid PID string"
    );
}
