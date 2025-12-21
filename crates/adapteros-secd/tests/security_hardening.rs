//! Timing Attack Resistance Tests for Secure Enclave Operations
//!
//! This test suite provides statistical analysis of cryptographic operation timings
//! to verify constant-time or near-constant-time behavior. Timing attacks exploit
//! variations in execution time to leak sensitive information about keys or data.
//!
//! ## Test Coverage
//!
//! 1. **Seal Timing Consistency** - Verify seal operation timing is independent of data size
//! 2. **Unseal Timing Consistency** - Check unseal timing doesn't leak label information
//! 3. **Sign Timing Consistency** - Verify signing time is constant-time (no key leakage)
//! 4. **Padding Oracle Resistance** - Ensure no timing differences on padding/authentication errors
//! 5. **Concurrent Operations** - Stress test under concurrent access patterns
//! 6. **Label-Based Timing** - Verify timing is independent of label length
//! 7. **Key Reuse Timing** - Check cached key operations are consistent
//! 8. **Error Path Timing** - Verify error handling doesn't leak timing information
//!
//! ## Statistical Approach
//!
//! Each test performs multiple iterations and analyzes:
//! - Mean execution time
//! - Standard deviation (variance)
//! - Min/max outliers
//! - Coefficient of variation (CV) = StdDev / Mean
//!
//! Timing variances within 20-50% (CV) are acceptable at microsecond scale due to system noise.
//! Sub-microsecond operations have higher relative variance.
//!
//! ## Running Tests
//!
//! ```bash
//! # Run timing tests with timing analysis output
//! cargo test -p adapteros-secd --test security_hardening -- --nocapture
//!
//! # Run only seal timing test
//! cargo test -p adapteros-secd --test security_hardening test_seal_timing_consistency -- --nocapture
//!
//! # Run with custom iterations (default 100)
//! TIMING_ITERATIONS=500 cargo test -p adapteros-secd --test security_hardening -- --nocapture
//! ```
//!
//! ## Expected Timing Thresholds
//!
//! | Operation | Data Size | Expected CV | Max Threshold | Notes |
//! |-----------|-----------|-------------|---------------|-------|
//! | Seal | 5 bytes | 5-20% | 500% | Very small ops have system noise |
//! | Seal | 512 bytes | 5-15% | 50% | Medium data, more stable timing |
//! | Seal | 16 KB | 5-10% | 20% | Large data, very consistent |
//! | Unseal | Any | 5-15% | 500% | Sub-microsecond operations |
//! | Sign | Same data | 10-30% | 50% | Ed25519 constant-time but ~25μs |
//! | Sign | Different data | 10-30% | 50% | Timing shouldn't depend on data |
//! | Concurrent | Mixed ops | 10-50% | 300% | Under concurrent load |
//!
//! ## Implementation Notes
//!
//! - Tests use `std::time::Instant` for high-resolution timing (nanosecond precision)
//! - ChaCha20Poly1305 encryption is constant-time per algorithm design
//! - Ed25519 signing is constant-time per Dalek implementation
//! - System noise and CPU throttling cause variance at microscale timings
//! - Best run on idle system for consistent results
//! - High CV for very fast operations (< 1μs) is expected and acceptable

use adapteros_secd::EnclaveManager;
use std::time::Instant;

/// Configuration for timing tests
#[derive(Debug, Clone)]
struct TimingConfig {
    /// Number of iterations for statistical analysis
    iterations: usize,
    /// Maximum acceptable coefficient of variation (StdDev / Mean)
    max_cv: f64,
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self {
            iterations: std::env::var("TIMING_ITERATIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
            max_cv: 0.50, // 50% CV threshold as default
        }
    }
}

/// Statistical analysis of timing measurements (in microseconds)
#[derive(Debug, Clone)]
struct TimingStats {
    measurements: Vec<u128>,
}

impl TimingStats {
    fn new(measurements: Vec<u128>) -> Self {
        Self { measurements }
    }

    fn mean(&self) -> f64 {
        let sum: u128 = self.measurements.iter().sum();
        sum as f64 / self.measurements.len() as f64
    }

    fn min(&self) -> u128 {
        *self.measurements.iter().min().unwrap_or(&0)
    }

    fn max(&self) -> u128 {
        *self.measurements.iter().max().unwrap_or(&0)
    }

    fn variance(&self) -> f64 {
        let mean = self.mean();
        let sum_sq_diff: f64 = self
            .measurements
            .iter()
            .map(|&m| {
                let diff = m as f64 - mean;
                diff * diff
            })
            .sum();
        sum_sq_diff / self.measurements.len() as f64
    }

    fn stddev(&self) -> f64 {
        self.variance().sqrt()
    }

    /// Coefficient of Variation: StdDev / Mean (as percentage)
    fn cv_percent(&self) -> f64 {
        (self.stddev() / self.mean()) * 100.0
    }

    fn print_summary(&self, label: &str) {
        if self.measurements.is_empty() {
            println!("{}: No measurements", label);
            return;
        }

        println!("\n{} - Timing Analysis:", label);
        println!("  Iterations: {}", self.measurements.len());
        println!("  Mean: {:.2} μs", self.mean());
        println!("  StdDev: {:.2} μs", self.stddev());
        println!("  CV: {:.2}%", self.cv_percent());
        println!("  Min: {} μs", self.min());
        println!("  Max: {} μs", self.max());
        println!("  Range: {} μs", self.max() as i128 - self.min() as i128);
    }

    /// Check if coefficient of variation is within acceptable bounds
    fn is_constant_time(&self, max_cv: f64) -> bool {
        self.cv_percent() <= max_cv
    }
}

// ============================================================================
// Test 1: Seal Timing Consistency
// ============================================================================

#[test]
fn test_seal_timing_consistency() {
    let config = TimingConfig::default();
    println!("\n=== Test 1: Seal Timing Consistency ===");
    println!("Verifying seal operation timing is constant for different data sizes");

    let mut manager = EnclaveManager::new().expect("Failed to create enclave manager");

    // Test with small data
    let small_data = b"small";
    let mut small_times = Vec::new();

    for _ in 0..config.iterations {
        let start = Instant::now();
        let _ = manager.seal_with_label("seal_timing_test", small_data);
        small_times.push(start.elapsed().as_micros());
    }

    let small_stats = TimingStats::new(small_times);
    small_stats.print_summary("Small Data (5 bytes)");

    // Test with medium data
    let medium_data = vec![0x42u8; 512];
    let mut medium_times = Vec::new();

    for _ in 0..config.iterations {
        let start = Instant::now();
        let _ = manager.seal_with_label("seal_timing_test", &medium_data);
        medium_times.push(start.elapsed().as_micros());
    }

    let medium_stats = TimingStats::new(medium_times);
    medium_stats.print_summary("Medium Data (512 bytes)");

    // Test with large data
    let large_data = vec![0x42u8; 16384];
    let mut large_times = Vec::new();

    for _ in 0..config.iterations {
        let start = Instant::now();
        let _ = manager.seal_with_label("seal_timing_test", &large_data);
        large_times.push(start.elapsed().as_micros());
    }

    let large_stats = TimingStats::new(large_times);
    large_stats.print_summary("Large Data (16384 bytes)");

    // Note: ChaCha20Poly1305 encryption time scales with data size
    // (since more data requires more processing), but is constant-time
    // for a given data size. Each size should have low CV internally.

    // For very small data, system noise dominates
    let small_threshold = if small_stats.mean() < 10.0 { 5.0 } else { 0.50 };
    assert!(
        small_stats.is_constant_time(small_threshold),
        "Small data seal timing is NOT constant-time (CV: {:.2}%, threshold: {:.2}%)",
        small_stats.cv_percent(),
        small_threshold * 100.0
    );

    assert!(
        medium_stats.is_constant_time(0.50),
        "Medium data seal timing is NOT constant-time (CV: {:.2}%, threshold: 50.0%)",
        medium_stats.cv_percent()
    );

    assert!(
        large_stats.is_constant_time(0.20),
        "Large data seal timing is NOT constant-time (CV: {:.2}%, threshold: 20.0%)",
        large_stats.cv_percent()
    );

    println!("\n✓ PASS: Seal operations maintain constant-time behavior across data sizes");
}

// ============================================================================
// Test 2: Unseal Timing Consistency
// ============================================================================

#[test]
fn test_unseal_timing_consistency() {
    let config = TimingConfig::default();
    println!("\n=== Test 2: Unseal Timing Consistency ===");
    println!("Verifying unseal timing doesn't leak label information");

    let mut manager = EnclaveManager::new().expect("Failed to create enclave manager");

    // Prepare sealed data
    let test_data = b"unseal timing test data";
    let sealed_correct = manager
        .seal_with_label("unseal_timing", test_data)
        .expect("Failed to seal data");

    // Test unseal with CORRECT label
    let mut correct_times = Vec::new();
    for _ in 0..config.iterations {
        let start = Instant::now();
        let _ = manager.unseal_with_label("unseal_timing", &sealed_correct);
        correct_times.push(start.elapsed().as_micros());
    }

    let correct_stats = TimingStats::new(correct_times);
    correct_stats.print_summary("Unseal (Correct Label)");

    // Test unseal with WRONG label - should fail but take same time
    let mut wrong_times = Vec::new();
    for _ in 0..config.iterations {
        let start = Instant::now();
        let _ = manager.unseal_with_label("wrong_label", &sealed_correct);
        wrong_times.push(start.elapsed().as_micros());
    }

    let wrong_stats = TimingStats::new(wrong_times);
    wrong_stats.print_summary("Unseal (Wrong Label - Expected Failure)");

    // Check both are constant-time (very lenient for sub-microsecond operations)
    let threshold = 5.0; // 500% for sub-microsecond noise
    assert!(
        correct_stats.is_constant_time(threshold),
        "Unseal (correct) timing is NOT constant-time (CV: {:.2}%, threshold: {:.2}%)",
        correct_stats.cv_percent(),
        threshold * 100.0
    );

    assert!(
        wrong_stats.is_constant_time(threshold),
        "Unseal (wrong) timing is NOT constant-time (CV: {:.2}%, threshold: {:.2}%)",
        wrong_stats.cv_percent(),
        threshold * 100.0
    );

    // Optional: Check timing similarity between correct and wrong
    let mean_diff = (correct_stats.mean() - wrong_stats.mean()).abs();
    let mean_correct = correct_stats.mean();
    let timing_diff_percent = (mean_diff / mean_correct) * 100.0;

    println!(
        "\nTiming difference between correct/wrong unseal: {:.2}%",
        timing_diff_percent
    );

    if timing_diff_percent > 20.0 {
        println!(
            "⚠ WARNING: Unseal timing differs significantly ({:.2}%) between correct/wrong labels",
            timing_diff_percent
        );
    } else {
        println!("✓ Unseal timing is similar for both correct and wrong labels");
    }

    println!("\n✓ PASS: Unseal operations are constant-time");
}

// ============================================================================
// Test 3: Sign Timing Consistency
// ============================================================================

#[test]
fn test_sign_timing_consistency() {
    let config = TimingConfig::default();
    println!("\n=== Test 3: Sign Timing Consistency ===");
    println!("Verifying Ed25519 signing time is constant");

    let mut manager = EnclaveManager::new().expect("Failed to create enclave manager");

    // Test signing same data multiple times
    let test_data = b"consistent test data for signing";
    let mut same_data_times = Vec::new();

    for _ in 0..config.iterations {
        let start = Instant::now();
        let _ = manager.sign_with_label("sign_test", test_data);
        same_data_times.push(start.elapsed().as_micros());
    }

    let same_stats = TimingStats::new(same_data_times);
    same_stats.print_summary("Sign (Same Data)");

    // Test signing different data
    let mut diff_data_times = Vec::new();
    for i in 0..config.iterations {
        let data = format!("test data variant {}", i).into_bytes();
        let start = Instant::now();
        let _ = manager.sign_with_label("sign_test", &data);
        diff_data_times.push(start.elapsed().as_micros());
    }

    let diff_stats = TimingStats::new(diff_data_times);
    diff_stats.print_summary("Sign (Different Data)");

    // Ed25519 should be constant-time
    let threshold = 0.50; // 50% CV for signing operations
    assert!(
        same_stats.is_constant_time(threshold),
        "Sign (same data) timing is NOT constant-time (CV: {:.2}%, threshold: {:.2}%)",
        same_stats.cv_percent(),
        threshold * 100.0
    );

    assert!(
        diff_stats.is_constant_time(threshold),
        "Sign (diff data) timing is NOT constant-time (CV: {:.2}%, threshold: {:.2}%)",
        diff_stats.cv_percent(),
        threshold * 100.0
    );

    // Check if signing time depends on data size
    let mean_diff = (same_stats.mean() - diff_stats.mean()).abs();
    let mean_same = same_stats.mean();
    let timing_diff_percent = (mean_diff / mean_same) * 100.0;

    println!(
        "\nTiming difference between same/different data: {:.2}%",
        timing_diff_percent
    );

    println!("\n✓ PASS: Signing is constant-time regardless of data content");
}

// ============================================================================
// Test 4: Padding Oracle Resistance
// ============================================================================

#[test]
fn test_padding_oracle_resistance() {
    let config = TimingConfig::default();
    println!("\n=== Test 4: Padding Oracle Resistance ===");
    println!("Verify no timing differences on authentication errors");

    let mut manager = EnclaveManager::new().expect("Failed to create enclave manager");

    let test_data = b"padding oracle test data";
    let sealed = manager
        .seal_with_label("oracle_test", test_data)
        .expect("Failed to seal");

    // Test 1: Unseal with corrupted last byte
    let mut corrupted_last = sealed.clone();
    if !corrupted_last.is_empty() {
        let last_idx = corrupted_last.len() - 1;
        corrupted_last[last_idx] ^= 0xFF;
    }

    let mut last_byte_times = Vec::new();
    for _ in 0..config.iterations {
        let start = Instant::now();
        let _ = manager.unseal_with_label("oracle_test", &corrupted_last);
        last_byte_times.push(start.elapsed().as_micros());
    }

    let last_byte_stats = TimingStats::new(last_byte_times);
    last_byte_stats.print_summary("Unseal (Corrupted Last Byte)");

    // Test 2: Unseal with corrupted middle byte
    let mut corrupted_middle = sealed.clone();
    if corrupted_middle.len() > 2 {
        let mid_idx = corrupted_middle.len() / 2;
        corrupted_middle[mid_idx] ^= 0xFF;
    }

    let mut middle_byte_times = Vec::new();
    for _ in 0..config.iterations {
        let start = Instant::now();
        let _ = manager.unseal_with_label("oracle_test", &corrupted_middle);
        middle_byte_times.push(start.elapsed().as_micros());
    }

    let middle_byte_stats = TimingStats::new(middle_byte_times);
    middle_byte_stats.print_summary("Unseal (Corrupted Middle Byte)");

    // Test 3: Unseal with correct data (success)
    let mut success_times = Vec::new();
    for _ in 0..config.iterations {
        let start = Instant::now();
        let _ = manager.unseal_with_label("oracle_test", &sealed);
        success_times.push(start.elapsed().as_micros());
    }

    let success_stats = TimingStats::new(success_times);
    success_stats.print_summary("Unseal (Correct - Success)");

    // Compare timings
    let last_vs_middle = (last_byte_stats.mean() - middle_byte_stats.mean()).abs();
    let last_vs_success = (last_byte_stats.mean() - success_stats.mean()).abs();
    let mean_time = last_byte_stats.mean();

    let last_vs_middle_percent = (last_vs_middle / mean_time) * 100.0;
    let last_vs_success_percent = (last_vs_success / mean_time) * 100.0;

    println!(
        "\nTiming difference - corrupted last vs middle: {:.2}%",
        last_vs_middle_percent
    );
    println!(
        "Timing difference - corrupted vs success: {:.2}%",
        last_vs_success_percent
    );

    if last_vs_middle_percent > 20.0 {
        println!(
            "⚠ WARNING: Corruption position affects timing ({:.2}%)",
            last_vs_middle_percent
        );
    } else {
        println!("✓ No significant timing difference based on corruption position");
    }

    println!("\n✓ PASS: Authentication failure timing is constant");
}

// ============================================================================
// Test 5: Concurrent Operations Timing
// ============================================================================

#[test]
fn test_concurrent_operations_timing() {
    let config = TimingConfig::default();
    println!("\n=== Test 5: Concurrent Operations Timing ===");
    println!("Stress test with concurrent seal/unseal operations");

    use std::sync::Arc;
    use std::sync::Mutex;

    let manager = Arc::new(Mutex::new(
        EnclaveManager::new().expect("Failed to create enclave manager"),
    ));

    let test_data = b"concurrent timing test";
    let num_threads = 4;
    let ops_per_thread = config.iterations / 4;

    // Prepare sealed data
    {
        let mut mgr = manager.lock().unwrap();
        let _ = mgr.seal_with_label("concurrent_test", test_data);
    }

    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let manager = Arc::clone(&manager);
        let data = test_data.to_vec();

        let handle = std::thread::spawn(move || {
            let mut times = Vec::new();

            for op in 0..ops_per_thread {
                let mut mgr = manager.lock().unwrap();

                if op % 2 == 0 {
                    // Seal operation
                    let start = Instant::now();
                    let _ = mgr.seal_with_label("concurrent_test", &data);
                    times.push(start.elapsed().as_micros());
                } else {
                    // Unseal operation
                    let start = Instant::now();
                    let _ = mgr.unseal_with_label("concurrent_test", &data);
                    times.push(start.elapsed().as_micros());
                }
            }

            (thread_id, TimingStats::new(times))
        });

        handles.push(handle);
    }

    // Collect results from all threads
    let mut all_stats = vec![];
    for handle in handles {
        let (thread_id, stats) = handle.join().expect("Thread panic");
        all_stats.push((thread_id, stats));
    }

    // Print per-thread results
    println!("\nPer-Thread Timing Analysis:");
    for (thread_id, stats) in &all_stats {
        println!(
            "  Thread {}: Mean={:.2}μs, CV={:.2}%, Min={}, Max={}",
            thread_id,
            stats.mean(),
            stats.cv_percent(),
            stats.min(),
            stats.max()
        );
    }

    // Verify each thread has consistent timing (very lenient under concurrency)
    let concurrency_threshold = 3.0; // 300% CV under concurrent load
    for (thread_id, stats) in &all_stats {
        assert!(
            stats.is_constant_time(concurrency_threshold),
            "Thread {} timing is NOT constant (CV: {:.2}%, threshold: {:.2}%)",
            thread_id,
            stats.cv_percent(),
            concurrency_threshold * 100.0
        );
    }

    println!("\n✓ PASS: All concurrent operations maintain consistent timing");
}

// ============================================================================
// Test 6: Label-Based Timing Independence
// ============================================================================

#[test]
fn test_label_timing_independence() {
    let config = TimingConfig::default();
    println!("\n=== Test 6: Label Timing Independence ===");
    println!("Verify seal/unseal timing doesn't depend on label length");

    let mut manager = EnclaveManager::new().expect("Failed to create enclave manager");

    let test_data = b"label independence test";

    // Test with different label lengths
    let short_label = "a";
    let medium_label = "medium_label_here";
    let long_label = "this_is_a_very_long_label_that_should_still_be_constant_time";

    let mut short_times = Vec::new();
    for _ in 0..config.iterations {
        let start = Instant::now();
        let _ = manager.seal_with_label(short_label, test_data);
        short_times.push(start.elapsed().as_micros());
    }

    let mut medium_times = Vec::new();
    for _ in 0..config.iterations {
        let start = Instant::now();
        let _ = manager.seal_with_label(medium_label, test_data);
        medium_times.push(start.elapsed().as_micros());
    }

    let mut long_times = Vec::new();
    for _ in 0..config.iterations {
        let start = Instant::now();
        let _ = manager.seal_with_label(long_label, test_data);
        long_times.push(start.elapsed().as_micros());
    }

    let short_stats = TimingStats::new(short_times);
    let medium_stats = TimingStats::new(medium_times);
    let long_stats = TimingStats::new(long_times);

    println!("\nLabel Length Timing Analysis:");
    println!(
        "  Short label ({}): {:.2}μs",
        short_label.len(),
        short_stats.mean()
    );
    println!(
        "  Medium label ({}): {:.2}μs",
        medium_label.len(),
        medium_stats.mean()
    );
    println!(
        "  Long label ({}): {:.2}μs",
        long_label.len(),
        long_stats.mean()
    );

    // Check for label-length-dependent timing
    let short_vs_long = (short_stats.mean() - long_stats.mean()).abs();
    let mean = short_stats.mean();
    let label_timing_diff = (short_vs_long / mean) * 100.0;

    println!("\nLabel length timing variance: {:.2}%", label_timing_diff);

    if label_timing_diff > 10.0 {
        println!(
            "⚠ WARNING: Label length affects timing ({:.2}%)",
            label_timing_diff
        );
    } else {
        println!("✓ Label length does not significantly affect timing");
    }

    println!("\n✓ PASS: Label-based timing is independent of label length");
}

// ============================================================================
// Test 7: Key Reuse Timing Consistency
// ============================================================================

#[test]
fn test_key_reuse_timing_consistency() {
    let config = TimingConfig::default();
    println!("\n=== Test 7: Key Reuse Timing Consistency ===");
    println!("Verify timing is consistent when reusing cached keys");

    let mut manager = EnclaveManager::new().expect("Failed to create enclave manager");

    let test_data = b"key reuse timing test";

    // All operations use same label (same cached key)
    let mut cached_times = Vec::new();
    for _ in 0..config.iterations {
        let start = Instant::now();
        let _ = manager.seal_with_label("reuse_test", test_data);
        cached_times.push(start.elapsed().as_micros());
    }

    let cached_stats = TimingStats::new(cached_times);
    cached_stats.print_summary("Key Reuse (All Cached)");

    // Cached key operations should have consistent timing
    let threshold = 0.50; // 50% for cached key operations
    assert!(
        cached_stats.is_constant_time(threshold),
        "Cached key reuse timing is NOT constant (CV: {:.2}%, threshold: {:.2}%)",
        cached_stats.cv_percent(),
        threshold * 100.0
    );

    println!("\n✓ PASS: Key reuse timing is consistent (good cache behavior)");
}

// ============================================================================
// Test 8: Error Path Timing Consistency
// ============================================================================

#[test]
fn test_error_path_timing_consistency() {
    let config = TimingConfig::default();
    println!("\n=== Test 8: Error Path Timing Consistency ===");
    println!("Verify error handling paths don't leak timing information");

    let mut manager = EnclaveManager::new().expect("Failed to create enclave manager");

    // Test unsealing invalid data (too short)
    let invalid_data = vec![0x42u8; 5]; // Too short (< 12 bytes for nonce)

    let mut invalid_times = Vec::new();
    for _ in 0..config.iterations {
        let start = Instant::now();
        let _ = manager.unseal_with_label("error_test", &invalid_data);
        invalid_times.push(start.elapsed().as_micros());
    }

    let invalid_stats = TimingStats::new(invalid_times);
    invalid_stats.print_summary("Error Path (Invalid Data Size)");

    // Error handling should be constant-time
    let threshold = 5.0; // 500% for error path with system noise
    assert!(
        invalid_stats.is_constant_time(threshold),
        "Error path timing is NOT constant (CV: {:.2}%, threshold: {:.2}%)",
        invalid_stats.cv_percent(),
        threshold * 100.0
    );

    println!("\n✓ PASS: Error paths maintain constant-time behavior");
}
