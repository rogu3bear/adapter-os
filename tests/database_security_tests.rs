//! Tests for database security features
//!
//! Tests SQL injection prevention and network exposure validation

use adapteros_db::Db;

#[tokio::test]
async fn test_sql_injection_prevention_in_list_rules() {
    // Test that SQL injection attempts in tenant_id are prevented
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    use adapteros_db::process_monitoring::ProcessMonitoringRule;
    
    // Attempt SQL injection via tenant_id filter
    let malicious_tenant = "tenant' OR '1'='1";
    let result = ProcessMonitoringRule::list(db.pool(), Some(malicious_tenant), None).await;
    
    // Should succeed but return empty results (no matching tenant)
    // The important thing is that it doesn't execute malicious SQL
    assert!(result.is_ok());
    let rules = result.unwrap();
    assert_eq!(rules.len(), 0); // No rules should match
}

#[tokio::test]
async fn test_sql_injection_prevention_in_query_metrics() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    use adapteros_db::process_monitoring::{ProcessHealthMetric, MetricFilters};
    
    // Attempt SQL injection via metric_name filter
    let malicious_metric = "metric'; DROP TABLE process_health_metrics; --";
    let filters = MetricFilters {
        worker_id: None,
        tenant_id: None,
        metric_name: Some(malicious_metric.to_string()),
        start_time: None,
        end_time: None,
        limit: None,
    };
    
    let result = ProcessHealthMetric::query(db.pool(), filters).await;
    
    // Should succeed but return empty results
    assert!(result.is_ok());
    let metrics = result.unwrap();
    assert_eq!(metrics.len(), 0);
    
    // Verify table still exists (wasn't dropped)
    let verify: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM process_health_metrics")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(verify.0, 0);
}

#[tokio::test]
async fn test_sqlite_path_traversal_prevention() {
    // Test that path traversal attempts are rejected
    let malicious_paths = [
        "../etc/passwd",
        "../../var/aos.db",
        "var/../../etc/passwd",
        "sqlite://../etc/passwd",
    ];
    
    for path in &malicious_paths {
        let result = Db::connect(path).await;
        assert!(result.is_err(), "Path traversal should be rejected: {}", path);
        
        let err_msg = match result {
            Ok(_) => panic!("Expected error for path: {}", path),
            Err(e) => e.to_string(),
        };
        assert!(err_msg.contains("..") || err_msg.contains("path"), 
                "Error should mention path traversal: {}", err_msg);
    }
}

#[tokio::test]
async fn test_sqlite_sensitive_directory_prevention() {
    // Test that sensitive directories are rejected
    let sensitive_paths = [
        "/etc/passwd",
        "/root/.db",
        "/sys/db",
        "/proc/db",
        "/dev/db",
    ];
    
    for path in &sensitive_paths {
        let result = Db::connect(path).await;
        assert!(result.is_err(), "Sensitive directory should be rejected: {}", path);
        
        let err_msg = match result {
            Ok(_) => panic!("Expected error for path: {}", path),
            Err(e) => e.to_string(),
        };
        assert!(err_msg.contains("sensitive") || err_msg.contains("directory"), 
                "Error should mention sensitive directory: {}", err_msg);
    }
}

#[tokio::test]
async fn test_sqlite_valid_paths_allowed() {
    // Test that valid paths are allowed
    let valid_paths = [
        "var/aos.db",
        "sqlite::memory:",
        "sqlite://var/aos.db",
        "./var/aos.db",
    ];
    
    for path in &valid_paths {
        // These might fail for other reasons (file doesn't exist, etc.)
        // but shouldn't fail due to path validation
        let result = Db::connect(path).await;
        if let Err(e) = result {
            // If it fails, check it's not a config error (path validation)
            let err_msg = format!("{}", e);
            if err_msg.contains("path") && (err_msg.contains("..") || err_msg.contains("sensitive")) {
                panic!("Valid path rejected: {} - {}", path, err_msg);
            }
            // Other errors are OK (file doesn't exist, etc.)
        }
    }
}

#[tokio::test]
#[ignore] // Requires PostgreSQL to be running
async fn test_postgres_network_validation_rejects_public_ip() {
    use adapteros_db::PostgresDb;
    
    // Test that public IPs are rejected in production
    std::env::set_var("AOS_ENV", "production");
    
    let public_ip_url = "postgresql://user:pass@8.8.8.8:5432/db";
    let result = PostgresDb::connect(public_ip_url).await;
    
    assert!(result.is_err(), "Public IP should be rejected in production");
    
    let err_msg = match result {
        Ok(_) => panic!("Expected error for public IP"),
        Err(e) => e.to_string(),
    };
    assert!(err_msg.contains("public IP") || err_msg.contains("8.8.8.8"), 
            "Error should mention public IP: {}", err_msg);
    
    std::env::remove_var("AOS_ENV");
}

#[tokio::test]
#[ignore] // Requires PostgreSQL to be running
async fn test_postgres_network_validation_allows_private_ip() {
    use adapteros_db::PostgresDb;
    
    // Test that private IPs are allowed
    let private_ip_urls = [
        "postgresql://user:pass@10.0.0.1:5432/db",
        "postgresql://user:pass@192.168.1.1:5432/db",
        "postgresql://user:pass@172.16.0.1:5432/db",
        "postgresql://user:pass@localhost:5432/db",
        "postgresql://user:pass@127.0.0.1:5432/db",
    ];
    
    for url in &private_ip_urls {
        // These will fail to connect (no actual DB), but shouldn't fail validation
        let result = PostgresDb::connect(url).await;
        match result {
            Err(e) => {
                let err_msg = format!("{}", e);
                if err_msg.contains("public IP") || err_msg.contains("0.0.0.0") {
                    panic!("Private IP rejected: {} - {}", url, err_msg);
                }
                // Connection failure is OK
            }
            Ok(_) => {} // Success is OK
        }
    }
}

#[tokio::test]
#[ignore] // Requires PostgreSQL to be running
async fn test_postgres_network_validation_rejects_all_interfaces() {
    use adapteros_db::PostgresDb;
    
    // Test that 0.0.0.0 is rejected
    std::env::set_var("AOS_ENV", "production");
    
    let all_interfaces_url = "postgresql://user:pass@0.0.0.0:5432/db";
    let result = PostgresDb::connect(all_interfaces_url).await;
    
    assert!(result.is_err(), "0.0.0.0 should be rejected in production");
    
    std::env::remove_var("AOS_ENV");
}
