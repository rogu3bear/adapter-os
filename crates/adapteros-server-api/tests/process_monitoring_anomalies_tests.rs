use adapteros_api_types::workers::WorkerCapabilities;
use adapteros_db::process_monitoring::{
    AlertSeverity, AnomalyStatus, CreateAnomalyRequest, ProcessAnomaly,
};
use adapteros_server_api::handlers::list_process_anomalies;
use adapteros_server_api::types::ProcessAnomalyResponse;
use axum::{extract::State, Extension};
use std::collections::HashMap;

mod common;
use common::{register_test_worker, setup_state, test_admin_claims};

#[tokio::test]
async fn list_process_anomalies_filters_and_paginates() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    // Register a worker to satisfy the foreign key constraint
    let worker_id = register_test_worker(
        &state,
        "tenant-1",
        WorkerCapabilities {
            backend_kind: "mlx".to_string(),
            implementation: None,
            supports_step: true,
            supports_bulk: true,
            supports_logits: false,
            supports_streaming: true,
            gpu_backward: false,
            multi_backend: false,
        },
    )
    .await
    .expect("register worker");

    let anomaly_one = CreateAnomalyRequest {
        worker_id: worker_id.clone(),
        tenant_id: "tenant-1".to_string(),
        anomaly_type: "cpu_spike".to_string(),
        metric_name: "cpu_usage".to_string(),
        detected_value: 92.1,
        expected_range_min: Some(10.0),
        expected_range_max: Some(80.0),
        confidence_score: 0.92,
        severity: AlertSeverity::Warning,
        description: Some("CPU spike".to_string()),
        detection_method: "threshold".to_string(),
        model_version: None,
        status: AnomalyStatus::Detected,
    };

    let anomaly_two = CreateAnomalyRequest {
        worker_id: worker_id.clone(),
        tenant_id: "tenant-1".to_string(),
        anomaly_type: "memory_leak".to_string(),
        metric_name: "memory_usage".to_string(),
        detected_value: 88.0,
        expected_range_min: Some(15.0),
        expected_range_max: Some(75.0),
        confidence_score: 0.88,
        severity: AlertSeverity::Warning,
        description: Some("Memory spike".to_string()),
        detection_method: "threshold".to_string(),
        model_version: None,
        status: AnomalyStatus::Investigating,
    };

    let anomaly_one_id = ProcessAnomaly::insert(state.db.pool(), anomaly_one)
        .await
        .expect("insert anomaly one");
    let anomaly_two_id = ProcessAnomaly::insert(state.db.pool(), anomaly_two)
        .await
        .expect("insert anomaly two");

    sqlx::query("UPDATE process_anomalies SET created_at = ? WHERE id = ?")
        .bind("2025-01-01T00:00:00Z")
        .bind(&anomaly_one_id)
        .execute(state.db.pool())
        .await
        .expect("update anomaly one timestamp");

    sqlx::query("UPDATE process_anomalies SET created_at = ? WHERE id = ?")
        .bind("2025-01-02T00:00:00Z")
        .bind(&anomaly_two_id)
        .execute(state.db.pool())
        .await
        .expect("update anomaly two timestamp");

    let mut params = HashMap::new();
    params.insert("limit".to_string(), "1".to_string());
    params.insert("offset".to_string(), "0".to_string());

    let response = list_process_anomalies(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Query(params),
    )
    .await
    .expect("list anomalies");
    let anomalies: Vec<ProcessAnomalyResponse> = response.0;

    assert_eq!(anomalies.len(), 1);
    assert_eq!(anomalies[0].id, anomaly_two_id);

    let mut type_params = HashMap::new();
    type_params.insert("anomaly_type".to_string(), "cpu_spike".to_string());

    let response = list_process_anomalies(
        State(state),
        Extension(claims),
        axum::extract::Query(type_params),
    )
    .await
    .expect("list anomalies by type");
    let anomalies: Vec<ProcessAnomalyResponse> = response.0;

    assert_eq!(anomalies.len(), 1);
    assert_eq!(anomalies[0].id, anomaly_one_id);
}
