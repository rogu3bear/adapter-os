use adapteros_server_api::handlers::list_process_health_metrics;
use adapteros_server_api::handlers::list_process_monitoring_reports;
use adapteros_server_api::types::{ProcessHealthMetricResponse, ProcessMonitoringReportResponse};
use axum::{extract::State, Extension};
use std::collections::HashMap;

mod common;
use common::{setup_state, test_admin_claims};

#[tokio::test]
async fn monitoring_reports_alias_health_metrics() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    // No filters; should still return a synthesized metrics-backed report
    let metrics_response = list_process_health_metrics(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Query(HashMap::new()),
    )
    .await
    .expect("metrics");
    let metrics: Vec<ProcessHealthMetricResponse> = metrics_response.0;

    let reports_response = list_process_monitoring_reports(
        State(state),
        Extension(claims),
        axum::extract::Query(HashMap::new()),
    )
    .await
    .expect("reports");
    let reports: Vec<ProcessMonitoringReportResponse> = reports_response.0;

    assert_eq!(
        reports.len(),
        1,
        "alias should return a single synthesized report"
    );
    let report = &reports[0];
    assert_eq!(report.report_type, "metrics_alias");

    let data = report.report_data.clone().expect("report_data");
    let aliased: Vec<ProcessHealthMetricResponse> =
        serde_json::from_value(data).expect("parse report_data");
    assert_eq!(
        aliased.len(),
        metrics.len(),
        "metrics alias should mirror health-metrics payload"
    );
}
