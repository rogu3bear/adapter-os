//! Test memory monitoring accuracy

use adapteros_lora_worker::MemoryMonitor;

#[test]
fn test_headroom_measurement() {
    let monitor = MemoryMonitor::new(15);
    let headroom = monitor.headroom_pct();

    // Headroom should be a reasonable percentage
    assert!(headroom > 0.0 && headroom <= 100.0, "Headroom out of range: {}%", headroom);
}

#[test]
fn test_headroom_check_passes() {
    // Use a very low threshold that should always pass
    let monitor = MemoryMonitor::new(1);
    assert!(monitor.check_headroom().is_ok());
}

#[test]
fn test_headroom_check_fails_with_impossible_threshold() {
    // Use threshold that will always fail (requires more than 100% free)
    let monitor = MemoryMonitor::new(101);
    assert!(monitor.check_headroom().is_err());
}

#[test]
fn test_should_evict_logic() {
    let monitor_low = MemoryMonitor::new(1);
    let monitor_high = MemoryMonitor::new(101);

    // Low threshold should not trigger eviction
    assert!(!monitor_low.should_evict());

    // Impossible high threshold should trigger eviction
    assert!(monitor_high.should_evict());
}

#[test]
fn test_headroom_consistency() {
    let monitor = MemoryMonitor::new(15);
    
    // Take multiple measurements
    let measurements: Vec<f32> = (0..5)
        .map(|_| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            monitor.headroom_pct()
        })
        .collect();

    // All measurements should be in reasonable range
    for &measurement in &measurements {
        assert!(measurement >= 0.0 && measurement <= 100.0);
    }

    // Measurements should be relatively consistent (within 10% variance)
    let avg = measurements.iter().sum::<f32>() / measurements.len() as f32;
    for &measurement in &measurements {
        let diff = (measurement - avg).abs();
        assert!(diff < 10.0, "Memory measurement variance too high: {} vs avg {}", measurement, avg);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_macos_specific_measurement() {
    let monitor = MemoryMonitor::new(15);
    let headroom = monitor.headroom_pct();

    // Verify measurement is reasonable on macOS
    assert!(headroom > 0.0 && headroom < 100.0);
    
    // macOS typically reports some free memory
    assert!(headroom > 1.0, "macOS should report some free memory");
}

#[cfg(target_os = "linux")]
#[test]
fn test_linux_specific_measurement() {
    let monitor = MemoryMonitor::new(15);
    let headroom = monitor.headroom_pct();

    // Verify measurement is reasonable on Linux
    assert!(headroom > 0.0 && headroom < 100.0);
    
    // Verify we can read /proc/meminfo
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap();
    assert!(meminfo.contains("MemTotal"));
    assert!(meminfo.contains("MemAvailable"));
}


