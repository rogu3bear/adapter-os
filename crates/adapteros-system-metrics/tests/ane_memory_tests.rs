//! Integration tests for ANE memory tracking
//!
//! These tests verify that the ANE memory collection features work correctly
//! across different macOS versions and system configurations.

use adapteros_system_metrics::ane::{AneMemoryStats, AneMetricsCollector};

#[test]
fn test_ane_memory_collection_doesnt_panic() {
    let collector = AneMetricsCollector::new();
    let stats = collector.collect_metrics();
    // Should not panic on any platform
    println!("ANE stats collected successfully: {:?}", stats);
}

#[test]
#[cfg(all(target_os = "macos", feature = "coreml"))]
fn test_direct_ane_stats_on_apple_silicon() {
    let collector = AneMetricsCollector::new();

    if !collector.is_available() {
        println!("Skipping: ANE not available on this system");
        return;
    }

    let stats = collector.collect_metrics();

    // If ANE is available, we should get either direct or estimated data
    assert!(
        stats.source == "direct" || stats.source == "estimated",
        "Invalid source: {}",
        stats.source
    );

    if stats.source == "direct" {
        println!("\n=== Direct ANE Memory Stats ===");
        println!("  Allocated: {} MB", stats.allocated_mb);
        println!("  Used: {} MB", stats.used_mb);
        println!("  Available: {} MB", stats.available_mb);
        println!("  Cached: {} MB", stats.cached_mb);
        println!("  Peak: {} MB", stats.peak_mb);
        println!("  Usage: {:.1}%", stats.usage_percent);
        println!("  Throttled: {}", stats.throttled);
        println!("  Generation: {}", stats.generation);

        // Direct stats should have reasonable values
        if stats.allocated_mb > 0 {
            assert!(stats.allocated_mb < 100_000, "Allocated MB too large");
            assert!(
                stats.used_mb <= stats.allocated_mb,
                "Used should not exceed allocated"
            );
        }
    } else {
        println!("Using estimated ANE memory stats (direct query unavailable)");
    }
}

#[test]
#[cfg(target_os = "macos")]
fn test_ane_stats_source_field_populated() {
    let collector = AneMetricsCollector::new();
    let stats = collector.collect_metrics();

    // Source field should always be populated
    assert!(!stats.source.is_empty(), "Source field should not be empty");
    assert!(
        ["direct", "estimated", "unavailable"].contains(&stats.source.as_str()),
        "Invalid source value: {}",
        stats.source
    );

    println!("ANE data source: {}", stats.source);
}

#[test]
#[cfg(not(target_os = "macos"))]
fn test_ane_graceful_degradation_non_macos() {
    let collector = AneMetricsCollector::new();
    let stats = collector.collect_metrics();

    // On non-macOS, ANE should be unavailable
    assert!(!stats.available, "ANE should not be available on non-macOS");
    assert_eq!(
        stats.source, "unavailable",
        "Source should be 'unavailable' on non-macOS"
    );
    assert_eq!(stats.generation, 0);
    assert_eq!(stats.allocated_mb, 0);
}

#[test]
#[cfg(target_os = "macos")]
fn test_ane_stats_consistency() {
    let collector = AneMetricsCollector::new();

    // Collect stats multiple times
    let stats1 = collector.collect_metrics();
    let stats2 = collector.collect_metrics();

    // Availability and generation should be consistent
    assert_eq!(stats1.available, stats2.available);
    assert_eq!(stats1.generation, stats2.generation);
    assert_eq!(stats1.source, stats2.source);

    // Memory values can vary but should be in reasonable ranges
    if stats1.available && stats1.allocated_mb > 0 {
        assert!(
            (stats1.allocated_mb as i64 - stats2.allocated_mb as i64).abs() < 1000,
            "Allocated memory should be relatively stable"
        );
    }
}

#[test]
fn test_ane_memory_stats_default_values() {
    let stats = AneMemoryStats::default();

    assert!(!stats.available);
    assert_eq!(stats.generation, 0);
    assert_eq!(stats.allocated_mb, 0);
    assert_eq!(stats.used_mb, 0);
    assert_eq!(stats.available_mb, 0);
    assert_eq!(stats.cached_mb, 0);
    assert_eq!(stats.peak_mb, 0);
    assert_eq!(stats.usage_percent, 0.0);
    assert!(!stats.throttled);
    assert_eq!(stats.source, "");
}

#[test]
#[cfg(all(target_os = "macos", feature = "coreml"))]
fn test_ane_generation_detection() {
    let collector = AneMetricsCollector::new();

    if collector.is_available() {
        let generation = collector.generation();
        println!("Detected ANE generation: {}", generation);

        // Generation should be reasonable (1-10 range for known chips)
        assert!(
            (1..=10).contains(&generation),
            "Unexpected generation: {}",
            generation
        );
    }
}

#[test]
#[cfg(all(target_os = "macos", feature = "coreml"))]
fn test_ane_metrics_under_load() {
    // This test would ideally run some ANE workload and verify memory increases
    // For now, just verify metrics can be collected during operation
    let collector = AneMetricsCollector::new();

    if !collector.is_available() {
        println!("Skipping load test: ANE not available");
        return;
    }

    let before = collector.collect_metrics();

    // Simulate some activity (in practice, would run a CoreML model)
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after = collector.collect_metrics();

    println!("\n=== ANE Memory Under Load ===");
    println!("Before - Used: {} MB", before.used_mb);
    println!("After  - Used: {} MB", after.used_mb);

    // Both should succeed
    assert_eq!(before.available, after.available);
}
