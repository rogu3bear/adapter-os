//! Canonical workspace build ID resolver for AdapterOS build scripts.
//!
//! Goals:
//! - One standard format everywhere: `{git_describe}-{YYYYMMDDHHmmss}`.
//! - One shared workspace file: `target/build_id.txt`.
//! - Concurrency-safe: writers take `target/build_id.lock` and write atomically.
//! - Bounded waiting: if another build is generating the build id, wait briefly,
//!   then emit a Cargo warning and fail fast (no infinite hangs).
//!
//! This module is intended to be `#[path = "../../build_support/aos_build_id.rs"] mod aos_build_id;`
//! from individual `build.rs` files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct BuildIdInfo {
    pub build_id: String,
}

pub fn find_workspace_root() -> Option<PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let mut path = PathBuf::from(manifest_dir);

    while let Some(parent) = path.parent() {
        let cargo_toml = parent.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(contents) = fs::read_to_string(&cargo_toml) {
                if contents.contains("[workspace]") {
                    return Some(parent.to_path_buf());
                }
            }
        }
        path = parent.to_path_buf();
    }

    None
}

pub fn workspace_target_dir() -> Option<PathBuf> {
    find_workspace_root().map(|root| root.join("target"))
}

pub fn workspace_build_id_path() -> Option<PathBuf> {
    workspace_target_dir().map(|target| target.join("build_id.txt"))
}

fn workspace_build_id_lock_path() -> Option<PathBuf> {
    workspace_target_dir().map(|target| target.join("build_id.lock"))
}

/// Canonical git identifier for build IDs.
///
/// Determinism note: `git describe --tags` is not stable if tags change. For
/// receipts/replay we need a value that is stable for a given commit, so we
/// use the short git SHA (7 chars) and optionally append `-dirty` when the
/// working tree is not clean.
pub fn get_git_describe() -> String {
    let sha = if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
    {
        if output.status.success() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            "unknown".to_string()
        }
    } else {
        "unknown".to_string()
    };

    if sha == "unknown" {
        return sha;
    }

    // `git status --porcelain` is the simplest consistent dirty check:
    // it includes staged, unstaged, and untracked changes.
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(!o.stdout.is_empty())
            } else {
                None
            }
        })
        .unwrap_or(false);

    if dirty {
        format!("{sha}-dirty")
    } else {
        sha
    }
}

/// Timestamp in compact format (UTC): `YYYYMMDDHHmmss`.
///
/// If `SOURCE_DATE_EPOCH` is set, it is used for reproducible builds.
pub fn get_timestamp_compact() -> String {
    if let Some(secs) = source_date_epoch_seconds() {
        if let Some(ts) = format_timestamp_compact_from_epoch(secs) {
            return ts;
        }
    }

    if let Ok(output) = Command::new("date").args(["-u", "+%Y%m%d%H%M%S"]).output() {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }

    "00000000000000".to_string()
}

fn source_date_epoch_seconds() -> Option<i64> {
    let epoch = std::env::var("SOURCE_DATE_EPOCH").ok()?;
    let trimmed = epoch.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<i64>().ok()
}

fn format_timestamp_compact_from_epoch(secs: i64) -> Option<String> {
    if let Ok(output) = Command::new("date")
        .args(["-u", "-r", &secs.to_string(), "+%Y%m%d%H%M%S"])
        .output()
    {
        if output.status.success() {
            let ts = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if is_compact_timestamp(&ts) {
                return Some(ts);
            }
        }
    }

    // GNU date fallback.
    if let Ok(output) = Command::new("date")
        .args(["-u", "-d", &format!("@{}", secs), "+%Y%m%d%H%M%S"])
        .output()
    {
        if output.status.success() {
            let ts = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if is_compact_timestamp(&ts) {
                return Some(ts);
            }
        }
    }

    None
}

fn is_compact_timestamp(s: &str) -> bool {
    s.len() == 14 && s.chars().all(|c| c.is_ascii_digit())
}

/// Parse `{prefix}-{YYYYMMDDHHmmss}` by splitting on the last `-`.
fn parse_build_id(value: &str) -> Option<(String, String)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (prefix, ts) = trimmed.rsplit_once('-')?;
    if prefix.is_empty() || !is_compact_timestamp(ts) {
        return None;
    }
    Some((prefix.to_string(), ts.to_string()))
}

/// Split a canonical build id into `(git_describe, timestamp_compact)`.
///
/// Returns None if `value` is not in `{prefix}-{YYYYMMDDHHmmss}` form.
#[allow(dead_code)] // used only by build scripts that need to export component env vars
pub fn split_build_id(value: &str) -> Option<(String, String)> {
    parse_build_id(value)
}

fn atomic_write_string(path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Avoid needless mtime churn when content already matches.
    if let Ok(existing) = fs::read_to_string(path) {
        if existing.trim() == content.trim() {
            return Ok(());
        }
    }

    let pid = std::process::id();
    let tmp = path.with_extension(format!("tmp.{}", pid));
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

struct LockGuard {
    lock_path: PathBuf,
}

impl LockGuard {
    fn acquire(lock_path: PathBuf) -> io::Result<Self> {
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent)?;
        }
        // create_new gives us a simple cross-process mutex.
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)?;
        // Best-effort: include holder PID for debugging.
        let _ = writeln_pid(&mut f);
        Ok(Self { lock_path })
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

fn writeln_pid(f: &mut fs::File) -> io::Result<()> {
    use std::io::Write;
    writeln!(f, "pid={}", std::process::id())
}

fn read_build_id_file(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let trimmed = content.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn acquire_lock_with_wait(lock_path: &Path, max_wait: Duration) -> Result<LockGuard, String> {
    let deadline = Instant::now() + max_wait;
    loop {
        match LockGuard::acquire(lock_path.to_path_buf()) {
            Ok(guard) => return Ok(guard),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if Instant::now() >= deadline {
                    return Err(format!(
                        "Workspace build id is currently being generated (lock present): {}",
                        lock_path.display()
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(e) => {
                return Err(format!(
                    "Failed to acquire build_id.lock at {}: {}",
                    lock_path.display(),
                    e
                ));
            }
        }
    }
}

fn wait_for_workspace_build_id(
    build_id_path: &Path,
    lock_path: &Path,
    current_git: &str,
    expected_ts: Option<&str>,
    max_wait: Duration,
) -> Result<String, String> {
    let deadline = Instant::now() + max_wait;
    while Instant::now() < deadline {
        if let Some(s) = read_build_id_file(build_id_path) {
            if let Some((prefix, ts)) = parse_build_id(&s) {
                if prefix == current_git && expected_ts.is_none_or(|e| e == ts) {
                    return Ok(s);
                }
            }
        }
        if !lock_path.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    Err(format!(
        "Workspace build id is currently being generated (lock present): {}",
        lock_path.display()
    ))
}

/// Resolve the canonical workspace build id, generating it if needed.
///
/// Semantics:
/// - If env `AOS_BUILD_ID` is set: validate format, persist to `target/build_id.txt`, return it.
/// - Else if `target/build_id.txt` exists and matches current git descriptor (and `SOURCE_DATE_EPOCH` if set): return it.
/// - Else generate `{git_describe}-{timestamp}` under lock, persist atomically, return it.
///
/// Concurrency:
/// - If another process holds the lock, we wait briefly for the file to appear (bounded),
///   otherwise we return an error to allow the caller to `cargo:warning` and fail fast.
pub fn resolve_workspace_build_id() -> Result<BuildIdInfo, String> {
    let build_id_path = workspace_build_id_path()
        .ok_or_else(|| "Could not find workspace target dir for build_id.txt".to_string())?;
    let lock_path = workspace_build_id_lock_path()
        .ok_or_else(|| "Could not find workspace target dir for build_id.lock".to_string())?;

    let current_git = get_git_describe();
    let expected_ts_from_epoch =
        source_date_epoch_seconds().and_then(format_timestamp_compact_from_epoch);

    // Explicit override always wins, but must be standard.
    if let Ok(raw) = std::env::var("AOS_BUILD_ID") {
        let override_id = raw.trim().to_string();
        if override_id.is_empty() {
            return Err("AOS_BUILD_ID is set but empty".to_string());
        }
        let _ = parse_build_id(&override_id).ok_or_else(|| {
            format!(
                "AOS_BUILD_ID override is not in canonical format {{git}}-{{YYYYMMDDHHmmss}}: {}",
                override_id
            )
        })?;

        // Persist so other build scripts (and scripts/build-ui.sh) get the same ID.
        // If another build is in the middle of generating, wait briefly (bounded).
        if let Some(existing) = read_build_id_file(&build_id_path) {
            if existing.trim() == override_id.trim() {
                return Ok(BuildIdInfo {
                    build_id: override_id,
                });
            }
        }

        let _guard = acquire_lock_with_wait(&lock_path, Duration::from_secs(10))?;
        atomic_write_string(&build_id_path, &override_id).map_err(|e| {
            format!(
                "Failed to write build_id.txt at {}: {}",
                build_id_path.display(),
                e
            )
        })?;
        return Ok(BuildIdInfo {
            build_id: override_id,
        });
    }

    // Fast path: existing build_id.txt matches our current git descriptor (+ epoch if set).
    if let Some(s) = read_build_id_file(&build_id_path) {
        if let Some((prefix, ts)) = parse_build_id(&s) {
            let ts_ok = expected_ts_from_epoch.as_deref().is_none_or(|e| e == ts);
            if prefix == current_git && ts_ok {
                return Ok(BuildIdInfo { build_id: s });
            }
        }
    }

    // If another build is currently generating the build id, wait briefly.
    if lock_path.exists() {
        let s = wait_for_workspace_build_id(
            &build_id_path,
            &lock_path,
            &current_git,
            expected_ts_from_epoch.as_deref(),
            Duration::from_secs(10),
        )?;
        let _ = parse_build_id(&s).ok_or_else(|| {
            format!(
                "Workspace build_id.txt became available but is invalid: {}",
                s
            )
        })?;
        return Ok(BuildIdInfo { build_id: s });
    }

    // Generate under lock.
    let guard = match LockGuard::acquire(lock_path.clone()) {
        Ok(guard) => guard,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            // Race: someone else grabbed the lock after our existence check.
            let s = wait_for_workspace_build_id(
                &build_id_path,
                &lock_path,
                &current_git,
                expected_ts_from_epoch.as_deref(),
                Duration::from_secs(10),
            )?;
            let _ = parse_build_id(&s).ok_or_else(|| {
                format!(
                    "Workspace build_id.txt became available but is invalid: {}",
                    s
                )
            })?;
            return Ok(BuildIdInfo { build_id: s });
        }
        Err(e) => {
            return Err(format!(
                "Failed to acquire build_id.lock at {}: {}",
                lock_path.display(),
                e
            ));
        }
    };

    let ts = expected_ts_from_epoch.unwrap_or_else(get_timestamp_compact);
    let build_id = format!("{}-{}", current_git, ts);

    atomic_write_string(&build_id_path, &build_id).map_err(|e| {
        format!(
            "Failed to write build_id.txt at {}: {}",
            build_id_path.display(),
            e
        )
    })?;
    drop(guard);

    Ok(BuildIdInfo { build_id })
}
