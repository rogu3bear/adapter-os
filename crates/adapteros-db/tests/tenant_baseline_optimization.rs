use adapteros_db::{QueryMetrics, QueryPerformanceMonitor};
use chrono::Utc;

#[test]
fn tenant_baseline_optimization_tracking() {
    let mut monitor = QueryPerformanceMonitor::new(1000);
    let tenant_id = Some("perf-test-tenant");
    let query_name = "tenant_opt_query";

    for latency in [1000_u64, 1100, 1050] {
        monitor.record(QueryMetrics {
            query_name: query_name.to_string(),
            execution_time_us: latency,
            rows_returned: None,
            used_index: true,
            query_plan: None,
            timestamp: Utc::now().to_rfc3339(),
            tenant_id: tenant_id.map(|t| t.to_string()),
        });
    }

    assert!(monitor.capture_baseline(tenant_id, query_name));

    for latency in [900_u64, 880, 870] {
        monitor.record(QueryMetrics {
            query_name: query_name.to_string(),
            execution_time_us: latency,
            rows_returned: None,
            used_index: true,
            query_plan: None,
            timestamp: Utc::now().to_rfc3339(),
            tenant_id: tenant_id.map(|t| t.to_string()),
        });
    }

    let impact = monitor
        .optimization_impact_for(tenant_id, query_name)
        .expect("Impact should be computed");

    assert!(impact.improvement_pct > 0.0);
    assert_eq!(impact.tenant_id.as_deref(), tenant_id);
    assert!(!monitor.optimization_impacts_since_baseline().is_empty());
}
