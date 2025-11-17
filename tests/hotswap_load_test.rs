#![cfg(test)]
use adapteros_db::{Db, SqliteBackend};
use adapteros_lora_worker::UmaPressureMonitor;
use adapteros_metrics_exporter::MetricsExporter;
use adapteros_server_api::{routes, state::ApiConfig, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use axum_test::TestServer;
use std::panic;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, RwLock,
};
use tokio::time::{sleep, Duration, Instant};

static PANICS: AtomicUsize = AtomicUsize::new(0);

// Baseline test without swaps
#[tokio::test]
async fn baseline_latency() {
    let app = routes::build(setup_real_state().await);
    let server = TestServer::new(app).unwrap();
    let start = Instant::now();

    let mut latencies = vec![];
    for _ in 0..100 {
        let req = Request::builder()
            .uri("/v1/infer")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"messages":[{"role":"user","content":"Hi"}]}"#,
            ))
            .unwrap();

        let req_start = Instant::now();
        let resp = server.send(req).await;
        latencies.push(req_start.elapsed().as_millis() as f64);
        assert_eq!(resp.status(), StatusCode::OK);
    }

    latencies.sort();
    let p95 = latencies[95];
    histogram!("baseline.p95_latency_ms", p95);
    assert!(p95 < 200.0); // Realistic threshold for mock inference
}

// Hot-swap load test
#[tokio::test]
async fn hotswap_under_load() {
    let state = setup_real_state().await;
    let app = routes::build(state.clone());
    let server = TestServer::new(app).unwrap();

    let baseline_p95 = run_baseline_real(&server).await;
    let start = Instant::now();
    let duration = Duration::from_secs(60); // Reduced for test speed

    // Swap task
    let server_swap = server.clone();
    let swap_handle = tokio::spawn(async move {
        let mut swaps = 0;
        let mut interval = tokio::time::interval(Duration::from_secs(5)); // Faster for test
        loop {
            interval.tick().await;
            if start.elapsed() > duration {
                break;
            }
            // Create stack first if needed, then activate
            let req = Request::builder()
                .uri("/v1/adapter-stacks/test-stack/activate")
                .method("POST")
                .header("authorization", "Bearer adapteros-local")
                .body(Body::empty())
                .unwrap();

            let resp = server_swap.send(req).await;
            if resp.status() == StatusCode::OK {
                swaps += 1;
            } else {
                tracing::warn!("Swap failed with status: {}", resp.status());
            }
        }
        swaps
    });

    // Load generator with panic detection
    let latencies = (0..50)
        .map(|_| {
            let server_load = server.clone();
            tokio::spawn(async move {
                let mut local_lat = vec![];
                loop {
                    if start.elapsed() > duration {
                        break;
                    }
                    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| async {
                        let req = Request::builder()
                            .uri("/v1/infer")
                            .method("POST")
                            .header("authorization", "Bearer adapteros-local")
                            .header("content-type", "application/json")
                            .body(Body::from(
                                r#"{"messages":[{"role":"user","content":"Hi"}]}"#,
                            ))
                            .unwrap();

                        let req_start = Instant::now();
                        let resp = server_load.send(req).await;
                        let latency = req_start.elapsed().as_millis() as f64;
                        (latency, resp.status())
                    }));

                    match result {
                        Ok((latency, status)) => {
                            if status != StatusCode::OK {
                                PANICS.fetch_add(1, Ordering::Relaxed);
                            }
                            local_lat.push(latency);
                        }
                        Err(_) => {
                            PANICS.fetch_add(1, Ordering::Relaxed);
                        }
                    }

                    sleep(Duration::from_millis(20)).await; // ~50 RPS
                }
                local_lat
            })
        })
        .collect::<Vec<_>>();

    let mut all_times = vec![];
    for h in latencies {
        let times = h.await.unwrap();
        all_times.extend(times);
    }

    let swaps = swap_handle.await.unwrap();
    all_times.sort();
    let p95 = all_times[95 * all_times.len() / 100];
    histogram!("hotswap.p95_latency_ms", p95);

    assert!(swaps >= 10); // 60s / 5s = 12, allow some margin
    assert_eq!(PANICS.load(Ordering::Relaxed), 0);
    assert!(p95 <= baseline_p95 * 1.5); // More lenient for real load
}

async fn setup_real_state() -> AppState {
    let db = Db::new(SqliteBackend::new(":memory:").await.unwrap());
    db.migrate().await.unwrap();
    db.seed_dev_data().await.unwrap(); // Seed tenants, users, adapters

    // Create a test stack
    let stack_req = CreateStackRequest {
        tenant_id: "default".to_string(),
        name: "test-stack".to_string(),
        description: Some("Test stack".to_string()),
        adapter_ids: vec!["adapter-1".to_string()], // Assume seeded adapter exists
        workflow_type: Some(WorkflowType::Parallel),
    };
    db.insert_stack(&stack_req).await.unwrap();

    let jwt_secret = vec![0u8; 32]; // Test secret
    let config = Arc::new(RwLock::new(ApiConfig::default()));
    let metrics_exporter = Arc::new(MetricsExporter::default());
    let uma_monitor = Arc::new(UmaPressureMonitor::new());

    AppState::new(db, jwt_secret, config, metrics_exporter, uma_monitor)
}

async fn run_baseline_real(server: &TestServer) -> f64 {
    let mut latencies = vec![];
    for _ in 0..50 {
        let req = Request::builder()
            .uri("/v1/infer")
            .method("POST")
            .header("authorization", "Bearer adapteros-local")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"messages":[{"role":"user","content":"Hi"}]}"#,
            ))
            .unwrap();

        let req_start = Instant::now();
        let resp = server.send(req).await;
        latencies.push(req_start.elapsed().as_millis() as f64);
        assert_eq!(resp.status(), StatusCode::OK);
    }

    latencies.sort();
    latencies[95 * latencies.len() / 100]
}
