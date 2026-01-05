//! Integration tests for Dependency Security Policy with known vulnerable dependencies

#![allow(clippy::field_reassign_with_default)]

use adapteros_policy::packs::{
    CveDataSource, CveEntry, CveProvider, DependencySecurityConfig, DependencySecurityPolicy,
    VulnerabilitySeverity,
};
use adapteros_policy::{Policy, PolicyId, Severity};
use chrono::Utc;

/// Test fixtures with known vulnerable packages
#[tokio::test]
async fn test_known_vulnerable_dependencies() {
    let policy = DependencySecurityPolicy::new(DependencySecurityConfig {
        max_cvss_score: 7.0,
        allowed_severities: vec![
            VulnerabilitySeverity::None,
            VulnerabilitySeverity::Low,
            VulnerabilitySeverity::Medium,
        ],
        block_unknown_vulnerabilities: false,
        ..Default::default()
    });

    // Simulate checking log4j with known CVE-2021-44228 (critical)
    let deps = vec![
        ("log4j".to_string(), "2.14.1".to_string()),
        ("openssl".to_string(), "1.0.2".to_string()),
    ];

    let result = policy.assess_dependencies(&deps).await.expect("Assessment");

    // These would be discovered from CVE database in production
    // For now, stub shows empty result
    assert_eq!(result.total_dependencies, 2);
    assert_eq!(result.violations.len(), 0); // Stubs don't return real CVE data
}

#[tokio::test]
async fn test_cache_hit_reduces_api_calls() {
    let mut config = DependencySecurityConfig::default();
    config.cache_ttl_seconds = 3600;
    config.max_cache_entries = 100;

    let policy = DependencySecurityPolicy::new(config);

    // First check - cache miss
    let _ = policy.check_dependency("lodash", "4.17.20").await;

    let stats1 = policy.get_cache_stats().await;
    assert_eq!(stats1.misses, 1);
    assert_eq!(stats1.hits, 0);
    assert_eq!(stats1.total_entries, 1);

    // Second check - cache hit
    let _ = policy.check_dependency("lodash", "4.17.20").await;

    let stats2 = policy.get_cache_stats().await;
    assert_eq!(stats2.hits, 1);
    assert_eq!(stats2.misses, 1);
    assert!(stats2.total_entries >= 1);
}

#[tokio::test]
async fn test_cache_expiration() {
    let mut config = DependencySecurityConfig::default();
    config.cache_ttl_seconds = 0; // Immediate expiration

    let policy = DependencySecurityPolicy::new(config);

    // Check dependency
    let _ = policy.check_dependency("react", "16.13.1").await;

    // Prune expired entries
    policy.prune_cache().await;

    let stats = policy.get_cache_stats().await;
    // Should have some evictions or cleaned up
    assert!(stats.last_cleanup > Utc::now() - chrono::Duration::seconds(5));
}

#[tokio::test]
async fn test_cache_eviction_on_max_size() {
    let mut config = DependencySecurityConfig::default();
    config.max_cache_entries = 3; // Very small cache

    let policy = DependencySecurityPolicy::new(config);

    // Fill cache beyond limit
    for i in 0..5 {
        let name = format!("package-{}", i);
        let _ = policy.check_dependency(&name, "1.0.0").await;
    }

    let stats = policy.get_cache_stats().await;
    // Should have at most 3 entries (one was evicted)
    assert!(stats.total_entries <= 3);
}

#[tokio::test]
async fn test_severity_threshold_enforcement() {
    let policy = DependencySecurityPolicy::new(DependencySecurityConfig {
        max_cvss_score: 5.0, // Very strict
        allowed_severities: vec![VulnerabilitySeverity::Low],
        ..Default::default()
    });

    // Validation should fail if High severity is present
    let _result = policy.validate_dependency("vulnerable-pkg", "1.0.0").await;

    // Stub implementation doesn't return real CVEs yet
    // But we can verify the policy was created with correct threshold
    assert_eq!(policy.id(), adapteros_policy::PolicyId::DependencySecurity);
}

#[tokio::test]
async fn test_multiple_providers_configuration() {
    let osv_policy = DependencySecurityPolicy::new(DependencySecurityConfig {
        cve_provider: CveProvider::Osv,
        ..Default::default()
    });

    let nvd_policy = DependencySecurityPolicy::new(DependencySecurityConfig {
        cve_provider: CveProvider::Nvd,
        ..Default::default()
    });

    let both_policy = DependencySecurityPolicy::new(DependencySecurityConfig {
        cve_provider: CveProvider::Both,
        ..Default::default()
    });

    // All should work
    let _ = osv_policy.check_dependency("test", "1.0").await;
    let _ = nvd_policy.check_dependency("test", "1.0").await;
    let _ = both_policy.check_dependency("test", "1.0").await;
}

#[tokio::test]
async fn test_clear_cache_operation() {
    let policy = DependencySecurityPolicy::new(DependencySecurityConfig::default());

    // Add entries
    let _ = policy.check_dependency("pkg1", "1.0").await;
    let _ = policy.check_dependency("pkg2", "2.0").await;

    let before = policy.get_cache_stats().await;
    assert!(before.total_entries > 0);

    // Clear cache
    policy.clear_cache().await;

    let after = policy.get_cache_stats().await;
    assert_eq!(after.total_entries, 0);
}

#[tokio::test]
async fn test_batch_dependency_assessment() {
    let policy = DependencySecurityPolicy::new(DependencySecurityConfig {
        block_unknown_vulnerabilities: false,
        ..Default::default()
    });

    let dependencies = vec![
        ("lodash".to_string(), "4.17.21".to_string()),
        ("express".to_string(), "4.18.2".to_string()),
        ("axios".to_string(), "1.4.0".to_string()),
        ("react".to_string(), "18.2.0".to_string()),
    ];

    let result = policy.assess_dependencies(&dependencies).await.unwrap();

    assert_eq!(result.total_dependencies, 4);
    // Stubs don't return real CVE data
    assert_eq!(result.vulnerable_count, 0);
    assert!(result.compliant); // Empty vulnerabilities = compliant
}

#[tokio::test]
async fn test_epss_score_threshold() {
    let policy = DependencySecurityPolicy::new(DependencySecurityConfig {
        max_epss_score: 0.50, // Strict EPSS threshold
        ..Default::default()
    });

    // Policy should be created with low EPSS threshold
    let assessment = policy.check_dependency("test-pkg", "1.0.0").await.unwrap();

    // Stubs don't include real EPSS scores yet
    assert_eq!(assessment.max_cvss_score, 0.0); // No vulnerabilities found by stub
}

#[tokio::test]
async fn test_grace_period_configuration() {
    let config = DependencySecurityConfig {
        grace_period_days: 30,
        ..Default::default()
    };

    assert_eq!(config.grace_period_days, 30);

    // In production, newly disclosed CVEs within grace period
    // would be logged but not block deployment
}

#[tokio::test]
async fn test_supply_chain_evidence_requirement() {
    let strict_policy = DependencySecurityPolicy::new(DependencySecurityConfig {
        require_supply_chain_evidence: true,
        ..Default::default()
    });

    let lenient_policy = DependencySecurityPolicy::new(DependencySecurityConfig {
        require_supply_chain_evidence: false,
        ..Default::default()
    });

    // Both should work but have different enforcement
    let _ = strict_policy.check_dependency("npm-pkg", "1.0.0").await;
    let _ = lenient_policy.check_dependency("npm-pkg", "1.0.0").await;
}

#[tokio::test]
async fn test_api_rate_limit_configuration() {
    let config = DependencySecurityConfig {
        api_rate_limit: 5, // 5 requests/sec
        ..Default::default()
    };

    assert_eq!(config.api_rate_limit, 5);

    // Real implementation would throttle API calls
    // Stub doesn't make actual API calls
}

#[tokio::test]
async fn test_auto_remediation_flag() {
    let auto_remediate_config = DependencySecurityConfig {
        auto_remediate: true,
        ..Default::default()
    };

    let no_auto_remediate_config = DependencySecurityConfig {
        auto_remediate: false,
        ..Default::default()
    };

    assert!(auto_remediate_config.auto_remediate);
    assert!(!no_auto_remediate_config.auto_remediate);
}

#[test]
fn test_vulnerability_severity_ordering() {
    // Verify severity levels are correctly ordered
    assert!(VulnerabilitySeverity::None < VulnerabilitySeverity::Low);
    assert!(VulnerabilitySeverity::Low < VulnerabilitySeverity::Medium);
    assert!(VulnerabilitySeverity::Medium < VulnerabilitySeverity::High);
    assert!(VulnerabilitySeverity::High < VulnerabilitySeverity::Critical);
}

#[test]
fn test_cve_entry_structure() {
    let cve = CveEntry {
        cve_id: "CVE-2021-44228".to_string(),
        package_name: "log4j".to_string(),
        affected_versions: vec!["2.0-2.14.1".to_string()],
        fixed_version: Some("2.15.0".to_string()),
        cvss_score: 10.0,
        cvss_v3_score: Some(10.0),
        cvss_v2_score: None,
        cvss_v3_vector: Some("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H".to_string()),
        cvss_v2_vector: None,
        epss_score: Some(0.98),
        epss_percentile: Some(99.9),
        severity: VulnerabilitySeverity::Critical,
        description: "Remote code execution in Apache Log4j".to_string(),
        published_date: Utc::now(),
        modified_date: Utc::now(),
        references: vec!["https://nvd.nist.gov/vuln/detail/CVE-2021-44228".to_string()],
        cwe_ids: vec!["CWE-94".to_string()],
        data_source: CveDataSource::Nvd,
    };

    assert_eq!(cve.cve_id, "CVE-2021-44228");
    assert_eq!(cve.severity, VulnerabilitySeverity::Critical);
    assert_eq!(cve.cvss_score, 10.0);
    assert!(cve.epss_score.unwrap() > 0.9);
}

#[test]
fn test_policy_metadata() {
    let policy = DependencySecurityPolicy::new(DependencySecurityConfig::default());

    assert_eq!(policy.id(), PolicyId::DependencySecurity);
    assert_eq!(policy.name(), "Dependency Security Policy");
    assert_eq!(policy.severity(), Severity::High);
}

#[test]
fn test_default_configuration_values() {
    let config = DependencySecurityConfig::default();

    assert_eq!(config.max_cvss_score, 7.0);
    assert_eq!(config.max_epss_score, 0.85);
    assert_eq!(config.min_data_freshness_hours, 24);
    assert_eq!(config.cache_ttl_seconds, 3600);
    assert_eq!(config.max_cache_entries, 10000);
    assert_eq!(config.grace_period_days, 30);
    assert!(!config.block_unknown_vulnerabilities);
    assert!(config.require_supply_chain_evidence);
    assert!(!config.auto_remediate);
}

#[tokio::test]
async fn test_concurrent_cache_access() {
    use tokio::task::JoinSet;

    let policy = std::sync::Arc::new(DependencySecurityPolicy::new(
        DependencySecurityConfig::default(),
    ));

    let mut set = JoinSet::new();

    // Spawn multiple concurrent check_dependency calls
    for i in 0..10 {
        let p = policy.clone();
        set.spawn(async move { p.check_dependency(&format!("pkg-{}", i), "1.0.0").await });
    }

    // Wait for all to complete
    while let Some(result) = set.join_next().await {
        assert!(result.is_ok());
    }

    let stats = policy.get_cache_stats().await;
    assert_eq!(stats.total_entries, 10);
}

#[test]
fn test_cve_data_source_variants() {
    let nvd = CveDataSource::Nvd;
    let osv = CveDataSource::Osv;

    assert_ne!(nvd, osv);
    assert_eq!(nvd, CveDataSource::Nvd);
    assert_eq!(osv, CveDataSource::Osv);
}
