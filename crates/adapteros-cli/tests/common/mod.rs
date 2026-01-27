use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{oneshot, Mutex};

/// Captured UDS request for assertions in tests.
#[derive(Clone, Debug, Default)]
pub struct CapturedRequest {
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
}

impl CapturedRequest {
    /// Returns request body as UTF-8 text (lossy).
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Programmed HTTP-style response emitted by the stub server.
#[derive(Clone, Debug)]
pub struct StubHttpResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl StubHttpResponse {
    /// Create a JSON response with `200 OK` status.
    pub fn ok_json(value: serde_json::Value) -> Self {
        let body = serde_json::to_vec(&value).expect("serialize stub json");
        Self {
            status: 200,
            content_type: "application/json",
            body,
        }
    }

    /// Create an arbitrary body response with provided status and content type.
    pub fn with_body(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            content_type,
            body: body.into(),
        }
    }

    fn into_parts(self) -> (u16, &'static str, Vec<u8>) {
        (self.status, self.content_type, self.body)
    }
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK",
    }
}

/// Stub Unix Domain Socket server for exercising CLI networking code.
pub struct StubUdsServer {
    #[allow(dead_code)]
    tempdir: TempDir,
    socket_path: PathBuf,
    requests: Arc<Mutex<Vec<CapturedRequest>>>,
    #[allow(dead_code)]
    responses: Arc<Mutex<VecDeque<StubHttpResponse>>>,
    shutdown: Option<oneshot::Sender<()>>,
    accept_loop: Option<tokio::task::JoinHandle<()>>,
}

impl StubUdsServer {
    /// Launch stub server with pre-programmed responses returned FIFO per request.
    pub async fn start(responses: Vec<StubHttpResponse>) -> Result<Self> {
        let tempdir = TempDir::with_prefix("aos-test-")?;
        let socket_path = tempdir.path().join("worker.sock");
        Self::start_with_socket(socket_path, tempdir, responses)
    }

    /// Launch stub server bound to a specific socket path.
    pub async fn start_at<P: AsRef<Path>>(
        socket_path: P,
        responses: Vec<StubHttpResponse>,
    ) -> Result<Self> {
        let tempdir = TempDir::with_prefix("aos-test-")?;
        Self::start_with_socket(socket_path.as_ref().to_path_buf(), tempdir, responses)
    }

    fn start_with_socket(
        socket_path: PathBuf,
        tempdir: TempDir,
        responses: Vec<StubHttpResponse>,
    ) -> Result<Self> {
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Ensure stale socket is removed if present
        let _ = std::fs::remove_file(&socket_path);

        let listener = UnixListener::bind(&socket_path)?;

        let requests = Arc::new(Mutex::new(Vec::new()));
        let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let requests_clone = Arc::clone(&requests);
        let responses_clone = Arc::clone(&responses);

        let accept_loop = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        break;
                    }
                    accept_result = listener.accept() => {
                        let (stream, _) = match accept_result {
                            Ok(pair) => pair,
                            Err(_) => continue,
                        };

                        let requests = Arc::clone(&requests_clone);
                        let responses = Arc::clone(&responses_clone);

                        tokio::spawn(async move {
                            if let Err(err) = handle_connection(stream, requests, responses).await {
                                eprintln!("stub uds connection error: {err}");
                            }
                        });
                    }
                }
            }
        });

        Ok(Self {
            tempdir,
            socket_path,
            requests,
            responses,
            shutdown: Some(shutdown_tx),
            accept_loop: Some(accept_loop),
        })
    }

    /// Returns socket path for client connections.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Retrieve captured requests (clone) for assertions.
    pub async fn captured_requests(&self) -> Vec<CapturedRequest> {
        let guard = self.requests.lock().await;
        guard.clone()
    }
}

impl Drop for StubUdsServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }

        if let Some(handle) = self.accept_loop.take() {
            handle.abort();
        }
    }
}

async fn handle_connection(
    stream: UnixStream,
    requests: Arc<Mutex<Vec<CapturedRequest>>>,
    responses: Arc<Mutex<VecDeque<StubHttpResponse>>>,
) -> Result<()> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();

    if reader.read_line(&mut request_line).await? == 0 {
        return Ok(());
    }

    if request_line.trim().is_empty() {
        return Ok(());
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();

    let mut content_length: usize = 0;
    loop {
        let mut header_line = String::new();
        let read = reader.read_line(&mut header_line).await?;
        if read == 0 {
            break;
        }

        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break;
        }

        if let Some((name, value)) = trimmed.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).await?;
    }

    {
        let mut guard = requests.lock().await;
        guard.push(CapturedRequest {
            method: method.clone(),
            path: path.clone(),
            body: body.clone(),
        });
    }

    let response = {
        let mut guard = responses.lock().await;
        guard.pop_front().unwrap_or_else(|| {
            StubHttpResponse::with_body(
                500,
                "application/json",
                b"{\"error\":\"no stub response\"}".to_vec(),
            )
        })
    };

    let (status, content_type, body_bytes) = response.into_parts();
    let reason = reason_phrase(status);
    let mut http_response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
        status,
        reason,
        content_type,
        body_bytes.len()
    )
    .into_bytes();

    http_response.extend_from_slice(&body_bytes);

    let mut stream = reader.into_inner();
    stream.write_all(&http_response).await?;
    stream.shutdown().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn captures_and_replays_responses() {
        let server = StubUdsServer::start(vec![StubHttpResponse::ok_json(serde_json::json!({
            "hello": "world"
        }))])
        .await
        .expect("start stub");

        let client = adapteros_client::UdsClient::new(std::time::Duration::from_millis(100));
        let socket_path = server.socket_path();

        let body = serde_json::json!({ "ping": "pong" }).to_string();
        let response = client
            .send_request(socket_path, "POST", "/example", Some(&body))
            .await
            .expect("response");

        assert!(response.contains("hello"));

        let requests = server.captured_requests().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].path, "/example");
        assert_eq!(requests[0].body_text(), body);
    }
}
