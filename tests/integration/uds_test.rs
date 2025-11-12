use tokio::net::UnixListener, UnixStream;
use tempfile::TempDir;
use adapteros_api::{serve_uds_with_worker, InferenceRequest};
use axum::http::StatusCode;
use hyper::client::conn::http1::Builder;
use hyper::Body;
use hyper::client::HttpConnector;
use hyper::Request;
use hyper::Response;
use serde_json::json;
use std::io::{Read, Write};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::FromRawFd;
use std::os::unix::net::AsFd;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_uds_serving() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::tempdir()?;
    let socket_path = temp_dir.path().join("test.sock");
    let worker = // Mock or real Worker for test; assume default
        adapteros_lora_worker::Worker::new_mock(); // Assume mock fn for test

    let server_task = tokio::spawn(async move {
        if let Err(e) = serve_uds_with_worker(&socket_path, worker).await {
            eprintln!("Server failed: {}", e);
        }
    });

    sleep(Duration::from_millis(100)).await; // Allow bind

    // Test 1: Health over UDS
    let stream = UnixStream::connect(&socket_path).await?;
    let mut http_conn = Builder::new()
        .handshake::<_, Request<Body>>(stream)
        .await?;
    let mut response = http_conn.send_request(Request::get("/health").body(Body::empty()).unwrap()).await?;

    let mut body = Vec::new();
    while let Some(chunk) = response.body_mut().data().next().await {
        let chunk = chunk?;
        body.extend_from_slice(&chunk);
    }
    let status = response.status();
    assert_eq!(status, StatusCode::OK);
    let json: Value = serde_json::from_slice(&body)?;
    assert_eq!(json["status"], "healthy".to_string());

    // Test 2: Valid inference (mock to succeed)
    let stream = UnixStream::connect(&socket_path).await?;
    let mut http_conn = Builder::new()
        .handshake::<_, Request<Body>>(stream)
        .await?;
    let req_json = json!({ "cpid": "test", "max_tokens": 100 }).to_string();
    let mut response = http_conn.send_request(
        Request::post("/inference")
            .header("content-type", "application/json")
            .body(Body::from(req_json))?
    ).await?;

    let mut body = Vec::new();
    while let Some(chunk) = response.body_mut().data().next().await {
        let chunk = chunk?;
        body.extend_from_slice(&chunk);
    }
    let status = response.status();
    assert_eq!(status, StatusCode::OK); // Assuming mock succeeds

    // Test 3: Invalid JSON over UDS (400 from extractor)
    let stream = UnixStream::connect(&socket_path).await?;
    let mut http_conn = Builder::new()
        .handshake::<_, Request<Body>>(stream)
        .await?;
    let invalid_json = r#"{invalid json}"#.as_bytes();
    let mut response = http_conn.send_request(
        Request::post("/inference")
            .header("content-type", "application/json")
            .body(Body::from(invalid_json))?
    ).await?;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = hyper::body::to_bytes(response.into_body()).await?;
    let json: Value = serde_json::from_slice(&body)?;
    let error_resp: ErrorResponse = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(error_resp.code, "BAD_REQUEST".to_string());
    assert!(error_resp.details.is_some());

    // Cleanup (server will exit on drop, but ensure socket removed)
    std::fs::remove_file(&socket_path).ok();

    server_task.abort();
    Ok(())
}
