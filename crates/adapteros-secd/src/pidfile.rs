//! PID file management for aos-secd daemon

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Write current process PID to file
pub fn write_pid(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();

    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Check if PID file exists and process is still running
    if path.exists() {
        if let Ok(old_pid_str) = fs::read_to_string(path) {
            if let Ok(old_pid) = old_pid_str.trim().parse::<u32>() {
                // Check if process exists (Unix-specific)
                #[cfg(unix)]
                {
                    use nix::sys::signal::{kill, Signal};
                    use nix::unistd::Pid;

                    if kill(Pid::from_raw(old_pid as i32), Signal::SIGCONT).is_ok() {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            format!("aos-secd already running with PID {}", old_pid),
                        ));
                    }
                }

                tracing::warn!("Removing stale PID file (process {} not running)", old_pid);
                fs::remove_file(path)?;
            }
        }
    }

    // Write current PID
    let pid = std::process::id();
    let mut file = fs::File::create(path)?;
    write!(file, "{}", pid)?;
    file.sync_all()?;

    tracing::info!("Wrote PID {} to {}", pid, path.display());
    Ok(())
}

/// Remove PID file
pub fn remove_pid(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();

    if path.exists() {
        fs::remove_file(path)?;
        tracing::info!("Removed PID file: {}", path.display());
    }

    Ok(())
}

/// Read PID from file
pub fn read_pid(path: impl AsRef<Path>) -> io::Result<u32> {
    let content = fs::read_to_string(path)?;
    content
        .trim()
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Check if process with given PID is running
#[cfg(unix)]
pub fn is_process_running(pid: u32) -> bool {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    kill(Pid::from_raw(pid as i32), Signal::SIGCONT).is_ok()
}

#[cfg(not(unix))]
pub fn is_process_running(_pid: u32) -> bool {
    // Non-Unix platforms: assume running
    true
}
