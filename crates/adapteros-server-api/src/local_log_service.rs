use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::path::{Component, Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

const DEFAULT_TAIL_LINES: usize = 200;
const MAX_TAIL_LINES: usize = 2_000;
const SOCKET_FILE_NAME: &str = "action-logs.sock";
const DEFAULT_ACTION_LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;
const DEFAULT_ACTION_LOG_KEEP_COUNT: usize = 6;

#[derive(Debug, Clone)]
pub struct LocalLogServiceConfig {
    pub socket_path: PathBuf,
    pub logs_root: PathBuf,
}

impl Default for LocalLogServiceConfig {
    fn default() -> Self {
        let var_dir = adapteros_core::resolve_var_dir();
        Self {
            socket_path: var_dir.join("run").join(SOCKET_FILE_NAME),
            logs_root: var_dir.join("logs"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct TailRequest {
    path: String,
    lines: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TailResponse {
    ok: bool,
    lines: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ActionLogEntry {
    timestamp: String,
    actor: String,
    action: String,
    outcome: String,
    message: String,
}

pub async fn run_local_log_service(
    config: LocalLogServiceConfig,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> Result<()> {
    std::fs::create_dir_all(&config.logs_root).with_context(|| {
        format!(
            "failed to create local log service root {}",
            config.logs_root.display()
        )
    })?;
    if let Err(e) = set_private_dir_permissions(&config.logs_root) {
        warn!(
            error = %e,
            path = %config.logs_root.display(),
            "failed to tighten local log service root permissions"
        );
    }

    if let Some(parent) = config.socket_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create local log service socket dir {}",
                parent.display()
            )
        })?;
        if let Err(e) = set_private_dir_permissions(parent) {
            warn!(
                error = %e,
                path = %parent.display(),
                "failed to tighten local log service socket dir permissions"
            );
        }
    }

    if config.socket_path.exists() {
        std::fs::remove_file(&config.socket_path).with_context(|| {
            format!(
                "failed to remove stale local log service socket {}",
                config.socket_path.display()
            )
        })?;
    }

    let listener = UnixListener::bind(&config.socket_path).with_context(|| {
        format!(
            "failed to bind local log service socket {}",
            config.socket_path.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        if let Err(e) = std::fs::set_permissions(&config.socket_path, permissions) {
            warn!(
                error = %e,
                path = %config.socket_path.display(),
                "failed to tighten socket permissions for local log service"
            );
        }
    }

    info!(
        socket = %config.socket_path.display(),
        logs_root = %config.logs_root.display(),
        "Local action log service started"
    );

    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                info!("Local action log service received shutdown signal");
                break;
            }
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _addr)) => {
                        if let Err(e) = handle_connection(stream, &config.logs_root).await {
                            debug!(error = %e, "local action log service request failed");
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "local action log service accept failed");
                    }
                }
            }
        }
    }

    if config.socket_path.exists() {
        std::fs::remove_file(&config.socket_path).ok();
    }
    Ok(())
}

async fn handle_connection(stream: UnixStream, logs_root: &Path) -> Result<()> {
    let mut reader = BufReader::new(stream);
    let mut raw = String::new();
    let bytes_read = reader
        .read_line(&mut raw)
        .await
        .context("failed to read local action log request")?;
    if bytes_read == 0 {
        return Ok(());
    }

    let response = match (|| -> Result<TailResponse> {
        let request =
            serde_json::from_str::<TailRequest>(raw.trim()).context("invalid request JSON")?;
        let resolved = resolve_requested_log_path(logs_root, &request.path)?;
        let lines = request
            .lines
            .unwrap_or(DEFAULT_TAIL_LINES)
            .clamp(1, MAX_TAIL_LINES);
        let lines = tail_lines(&resolved, lines)?;
        Ok(TailResponse {
            ok: true,
            lines,
            error: None,
        })
    })() {
        Ok(response) => response,
        Err(e) => TailResponse {
            ok: false,
            lines: Vec::new(),
            error: Some(e.to_string()),
        },
    };

    let mut stream = reader.into_inner();
    let payload = serde_json::to_string(&response).context("failed to serialize response")?;
    stream
        .write_all(payload.as_bytes())
        .await
        .context("failed to write response payload")?;
    stream
        .write_all(b"\n")
        .await
        .context("failed to write response newline")?;
    stream
        .shutdown()
        .await
        .context("failed to close local action log response stream")?;
    Ok(())
}

fn resolve_requested_log_path(logs_root: &Path, requested: &str) -> Result<PathBuf> {
    if requested.trim().is_empty() {
        return Err(anyhow!("path must be provided"));
    }

    let relative = Path::new(requested.trim());
    if relative.is_absolute() {
        return Err(anyhow!("absolute paths are not allowed"));
    }

    for component in relative.components() {
        match component {
            Component::Normal(_) => {}
            _ => return Err(anyhow!("path must not contain traversal or prefixes")),
        }
    }

    let joined = logs_root.join(relative);
    let canonical_root = std::fs::canonicalize(logs_root)
        .with_context(|| format!("failed to resolve logs root {}", logs_root.display()))?;
    let canonical_path = std::fs::canonicalize(&joined)
        .with_context(|| format!("failed to resolve requested log path {}", joined.display()))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(anyhow!(
            "resolved path is outside logs root: {}",
            canonical_path.display()
        ));
    }
    Ok(canonical_path)
}

fn tail_lines(path: &Path, lines: usize) -> Result<Vec<String>> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("failed to open log file {}", path.display()))?;
    let reader = std::io::BufReader::new(file);
    let mut ring = VecDeque::with_capacity(lines);
    for line_result in reader.lines() {
        let line = line_result.with_context(|| format!("failed to read {}", path.display()))?;
        if ring.len() == lines {
            ring.pop_front();
        }
        ring.push_back(line);
    }
    Ok(ring.into_iter().collect())
}

pub fn job_log_path(job_id: &str) -> PathBuf {
    action_logs_root()
        .join("jobs")
        .join(format!("{}.log", sanitize_segment(job_id)))
}

pub fn training_log_path(job_id: &str) -> PathBuf {
    action_logs_root()
        .join("training")
        .join(format!("{}.log", sanitize_segment(job_id)))
}

pub fn service_log_path(service_id: &str) -> PathBuf {
    action_logs_root()
        .join("services")
        .join(format!("{}.log", sanitize_segment(service_id)))
}

pub async fn attach_job_logs_path(db: &adapteros_db::Db, job_id: &str) -> Result<PathBuf> {
    let path = job_log_path(job_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create action logs directory {}",
                parent.display()
            )
        })?;
        if let Err(e) = set_private_dir_permissions(parent) {
            warn!(
                error = %e,
                path = %parent.display(),
                "failed to tighten action logs directory permissions"
            );
        }
    }
    db.update_job_logs_path(job_id, Some(&path.to_string_lossy()))
        .await
        .with_context(|| format!("failed to persist logs_path for job {}", job_id))?;
    Ok(path)
}

pub fn append_job_action(
    job_id: &str,
    actor: &str,
    action: &str,
    outcome: &str,
    message: &str,
) -> Result<PathBuf> {
    append_action(job_log_path(job_id), actor, action, outcome, message)
}

pub fn append_training_action(
    job_id: &str,
    actor: &str,
    action: &str,
    outcome: &str,
    message: &str,
) -> Result<PathBuf> {
    append_action(training_log_path(job_id), actor, action, outcome, message)
}

pub fn append_service_action(
    service_id: &str,
    actor: &str,
    action: &str,
    outcome: &str,
    message: &str,
) -> Result<PathBuf> {
    append_action(
        service_log_path(service_id),
        actor,
        action,
        outcome,
        message,
    )
}

fn append_action(
    path: PathBuf,
    actor: &str,
    action: &str,
    outcome: &str,
    message: &str,
) -> Result<PathBuf> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create action log dir {}", parent.display()))?;
        if let Err(e) = set_private_dir_permissions(parent) {
            warn!(
                error = %e,
                path = %parent.display(),
                "failed to tighten action log directory permissions"
            );
        }
    }

    let (max_bytes, keep_count) = action_log_retention_policy();
    rotate_action_log_if_large(&path, max_bytes, keep_count)?;

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .with_context(|| format!("failed to open action log file {}", path.display()))?;
    if let Err(e) = set_private_file_permissions(&path) {
        warn!(
            error = %e,
            path = %path.display(),
            "failed to tighten action log file permissions"
        );
    }

    let entry = ActionLogEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        actor: sanitize_value(actor),
        action: sanitize_value(action),
        outcome: sanitize_value(outcome),
        message: sanitize_message(message),
    };
    let line =
        serde_json::to_string(&entry).context("failed to serialize action log entry as JSON")?;
    file.write_all(line.as_bytes())
        .with_context(|| format!("failed to append action log {}", path.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("failed to append action log newline {}", path.display()))?;
    Ok(path)
}

pub fn action_logs_root() -> PathBuf {
    adapteros_core::resolve_var_dir()
        .join("logs")
        .join("actions")
}

fn sanitize_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

fn sanitize_value(raw: &str) -> String {
    raw.chars()
        .map(|ch| if ch.is_ascii_whitespace() { '_' } else { ch })
        .collect()
}

fn sanitize_message(raw: &str) -> String {
    raw.replace('\n', "\\n").replace('\r', "\\r")
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn action_log_retention_policy() -> (u64, usize) {
    let max_bytes = std::env::var("AOS_ACTION_LOG_MAX_BYTES")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_ACTION_LOG_MAX_BYTES);

    let keep_count = std::env::var("AOS_ACTION_LOG_KEEP_COUNT")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_ACTION_LOG_KEEP_COUNT);

    (max_bytes, keep_count)
}

fn rotate_action_log_if_large(path: &Path, max_bytes: u64, keep_count: usize) -> Result<()> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(e).with_context(|| format!("failed to stat action log {}", path.display()));
        }
    };

    if metadata.len() <= max_bytes {
        return Ok(());
    }

    let archive = archive_log_path(path);
    std::fs::rename(path, &archive).with_context(|| {
        format!(
            "failed to archive action log {} to {}",
            path.display(),
            archive.display()
        )
    })?;
    prune_action_log_archives(path, keep_count)
}

fn archive_log_path(path: &Path) -> PathBuf {
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let base = format!("{}.{}", path.to_string_lossy(), ts);
    let mut candidate = PathBuf::from(&base);
    let mut suffix = 1usize;
    while candidate.exists() {
        candidate = PathBuf::from(format!("{}.{}", base, suffix));
        suffix += 1;
    }
    candidate
}

fn prune_action_log_archives(path: &Path, keep_count: usize) -> Result<()> {
    if keep_count == 0 {
        return Ok(());
    }

    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("action log path has no parent: {}", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("action log path has no file name: {}", path.display()))?;
    let prefix = format!("{}.", file_name);

    let mut archives = Vec::new();
    for entry in std::fs::read_dir(parent)
        .with_context(|| format!("failed to read action log dir {}", parent.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to inspect action log archive entry in {}",
                parent.display()
            )
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.starts_with(&prefix) {
            archives.push(path);
        }
    }

    archives.sort_by(|a, b| {
        let a_name = a.file_name().and_then(|v| v.to_str()).unwrap_or_default();
        let b_name = b.file_name().and_then(|v| v.to_str()).unwrap_or_default();
        b_name.cmp(a_name)
    });

    for stale in archives.into_iter().skip(keep_count) {
        std::fs::remove_file(&stale)
            .with_context(|| format!("failed to prune stale action log {}", stale.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use serde_json::Value;
    use tokio::io::AsyncBufReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::sync::broadcast;
    use tokio::time::{sleep, Duration};

    #[test]
    fn resolve_requested_log_path_rejects_unsafe_inputs() {
        let logs_root = PathBuf::from("var/logs");

        assert!(resolve_requested_log_path(&logs_root, "/abs.log").is_err());
        assert!(resolve_requested_log_path(&logs_root, "../escape.log").is_err());
        assert!(resolve_requested_log_path(&logs_root, "nested/../escape.log").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_requested_log_path_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp = adapteros_core::tempdir_in_var("local-log-symlink-escape")
            .expect("temp directory should be created");
        let logs_root = temp.path().join("logs");
        std::fs::create_dir_all(logs_root.join("actions/jobs")).expect("create log tree");

        let outside = temp.path().join("outside.log");
        std::fs::write(&outside, "secret").expect("create outside file");

        let link_path = logs_root.join("actions/jobs/escape.log");
        symlink(&outside, &link_path).expect("create symlink");

        let err = resolve_requested_log_path(&logs_root, "actions/jobs/escape.log")
            .expect_err("symlink escape should be rejected");
        assert!(
            err.to_string().contains("outside logs root"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn tail_lines_returns_bounded_suffix() {
        let temp = adapteros_core::tempdir_in_var("local-log-tail")
            .expect("temp directory should be created");
        let path = temp.path().join("sample.log");
        std::fs::write(&path, "l1\nl2\nl3\nl4\n").expect("seed log file");

        let lines = tail_lines(&path, 2).expect("tail should succeed");
        assert_eq!(lines, vec!["l3".to_string(), "l4".to_string()]);
    }

    #[test]
    fn append_action_writes_jsonl_entry() {
        let temp = adapteros_core::tempdir_in_var("local-log-jsonl")
            .expect("temp directory should be created");
        let path = temp.path().join("entry.log");

        append_action(
            path.clone(),
            "api user",
            "start training",
            "success",
            "job accepted",
        )
        .expect("append should succeed");

        let raw = std::fs::read_to_string(&path).expect("read entry");
        let line = raw.trim();
        let value: Value = serde_json::from_str(line).expect("line should be valid JSON");
        assert_eq!(value.get("actor").and_then(Value::as_str), Some("api_user"));
        assert_eq!(
            value.get("action").and_then(Value::as_str),
            Some("start_training")
        );
        assert_eq!(
            value.get("outcome").and_then(Value::as_str),
            Some("success")
        );
        assert_eq!(
            value.get("message").and_then(Value::as_str),
            Some("job accepted")
        );
        assert!(
            value
                .get("timestamp")
                .and_then(Value::as_str)
                .is_some_and(|ts| !ts.is_empty()),
            "timestamp must be present"
        );
    }

    #[cfg(unix)]
    #[test]
    fn append_action_sets_private_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = adapteros_core::tempdir_in_var("local-log-perms")
            .expect("temp directory should be created");
        let path = temp.path().join("jobs").join("job-1.log");

        append_action(path.clone(), "api", "tail", "ok", "permission check")
            .expect("append should succeed");

        let file_mode = std::fs::metadata(&path)
            .expect("file metadata")
            .permissions()
            .mode()
            & 0o777;
        let dir_mode = std::fs::metadata(path.parent().expect("parent dir"))
            .expect("dir metadata")
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(file_mode, 0o600, "file mode should be 0600");
        assert_eq!(dir_mode, 0o700, "dir mode should be 0700");
    }

    #[test]
    fn rotate_action_log_prunes_old_archives() {
        let temp = adapteros_core::tempdir_in_var("local-log-rotate")
            .expect("temp directory should be created");
        let path = temp.path().join("service.log");

        for i in 0..5 {
            std::fs::write(&path, format!("entry-{i}-{}", "x".repeat(128))).expect("seed log");
            rotate_action_log_if_large(&path, 8, 2).expect("rotation should succeed");
        }

        let prefix = "service.log.";
        let mut archives = std::fs::read_dir(temp.path())
            .expect("list archives")
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(prefix))
            })
            .collect::<Vec<_>>();
        archives.sort();
        assert!(
            archives.len() <= 2,
            "expected at most 2 archived files, found {}: {:?}",
            archives.len(),
            archives
        );
    }

    #[tokio::test]
    async fn uds_tail_reads_lines_and_rejects_traversal() {
        let temp = adapteros_core::tempdir_in_var("local-log-uds")
            .expect("temp directory should be created");
        let logs_root = temp.path().join("logs");
        let run_root = temp.path().join("run");
        let socket_path = run_root.join("action-logs.sock");

        std::fs::create_dir_all(logs_root.join("actions/jobs")).expect("create logs tree");
        std::fs::write(
            logs_root.join("actions/jobs/job-uds.log"),
            "line-1\nline-2\nline-3\n",
        )
        .expect("write sample log");

        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let service = tokio::spawn(run_local_log_service(
            LocalLogServiceConfig {
                socket_path: socket_path.clone(),
                logs_root: logs_root.clone(),
            },
            shutdown_rx,
        ));

        let mut ready = false;
        for _ in 0..100 {
            if socket_path.exists() {
                ready = true;
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
        assert!(ready, "local log service socket was not created");

        let ok_response = tail_via_socket(&socket_path, "actions/jobs/job-uds.log", 2).await;
        assert!(ok_response.ok, "expected successful tail response");
        assert_eq!(
            ok_response.lines,
            vec!["line-2".to_string(), "line-3".to_string()]
        );

        let blocked_response = tail_via_socket(&socket_path, "../secrets.log", 2).await;
        assert!(!blocked_response.ok, "expected traversal path rejection");
        assert!(
            blocked_response
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("traversal"),
            "expected traversal rejection error"
        );

        let _ = shutdown_tx.send(());
        let result = service.await.expect("service join should succeed");
        assert!(result.is_ok(), "service should shut down cleanly");
    }

    async fn tail_via_socket(socket_path: &Path, path: &str, lines: usize) -> TailResponse {
        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("connect to local log service socket");
        let request = json!({
            "path": path,
            "lines": lines,
        })
        .to_string();
        stream
            .write_all(request.as_bytes())
            .await
            .expect("write tail request");
        stream
            .write_all(b"\n")
            .await
            .expect("write request terminator");

        let mut reader = BufReader::new(stream);
        let mut payload = String::new();
        reader
            .read_line(&mut payload)
            .await
            .expect("read tail response");
        assert!(
            !payload.trim().is_empty(),
            "expected non-empty JSON response payload"
        );
        serde_json::from_str(payload.trim()).expect("response should be valid JSON")
    }
}
