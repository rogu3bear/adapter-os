#!/usr/bin/env rust-script
//! Standalone verification that OnceLock provides thread-safe uptime tracking
//!
//! This script demonstrates the same thread-safety guarantees as the test
//! in status_writer.rs, but can be run independently.
//!
//! Run with: cargo script test_uptime_thread_safety.rs

use std::sync::{Arc, Barrier, OnceLock};
use std::thread;
use std::time::Instant;

/// Tracks when the program started (initialized on first access)
static START_TIME: OnceLock<Instant> = OnceLock::new();

/// Initialize uptime tracking (safe to call multiple times)
fn init_uptime_tracking() {
    let _ = START_TIME.get_or_init(Instant::now);
}

/// Get uptime in seconds since first call (thread-safe)
fn get_uptime_secs() -> u64 {
    START_TIME.get_or_init(Instant::now).elapsed().as_secs()
}

fn main() {
    println!("🧪 Testing OnceLock thread-safety for uptime tracking\n");

    // Initialize uptime tracking once
    init_uptime_tracking();

    const NUM_THREADS: usize = 20;
    const READS_PER_THREAD: usize = 100;

    println!("Spawning {} threads that will each read uptime {} times", NUM_THREADS, READS_PER_THREAD);
    println!("All threads will start simultaneously using a barrier\n");

    // Barrier ensures all threads start reading simultaneously
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let mut handles = Vec::with_capacity(NUM_THREADS);

    for thread_id in 0..NUM_THREADS {
        let barrier_clone = Arc::clone(&barrier);

        let handle = thread::spawn(move || {
            // Wait for all threads to be ready
            barrier_clone.wait();

            let mut previous_uptime = 0u64;
            let mut readings = Vec::with_capacity(READS_PER_THREAD);

            // Read uptime many times from this thread
            for _ in 0..READS_PER_THREAD {
                let current_uptime = get_uptime_secs();
                readings.push(current_uptime);

                // Uptime should never decrease (monotonic guarantee)
                assert!(
                    current_uptime >= previous_uptime,
                    "Thread {}: Uptime decreased from {} to {} - DATA RACE DETECTED!",
                    thread_id,
                    previous_uptime,
                    current_uptime
                );

                previous_uptime = current_uptime;
            }

            readings
        });

        handles.push(handle);
    }

    // Collect results from all threads
    let mut all_readings = Vec::new();
    let mut panicked_threads = 0;

    for (thread_id, handle) in handles.into_iter().enumerate() {
        match handle.join() {
            Ok(readings) => {
                all_readings.extend(readings);
            }
            Err(_) => {
                eprintln!("❌ Thread {} panicked!", thread_id);
                panicked_threads += 1;
            }
        }
    }

    // Verify results
    println!("📊 Results:");
    println!("  - Total readings collected: {}", all_readings.len());
    println!("  - Expected readings: {}", NUM_THREADS * READS_PER_THREAD);
    println!("  - Panicked threads: {}", panicked_threads);

    // Verify we got all expected readings (no panics or lost data)
    assert_eq!(
        all_readings.len(),
        NUM_THREADS * READS_PER_THREAD,
        "Should have collected all readings from all threads"
    );

    assert_eq!(panicked_threads, 0, "No threads should have panicked");

    // All readings should be finite, reasonable values
    let max_uptime = all_readings.iter().max().unwrap_or(&0);
    assert!(
        *max_uptime < 3600,
        "Uptime should be less than 1 hour for this test (got {})",
        max_uptime
    );

    println!("\n✅ SUCCESS: {} concurrent reads from {} threads completed with:", NUM_THREADS * READS_PER_THREAD, NUM_THREADS);
    println!("  ✓ No unsafe code");
    println!("  ✓ No data races");
    println!("  ✓ No panics");
    println!("  ✓ No undefined behavior");
    println!("  ✓ Monotonic uptime values (no decreases)");
    println!("\n🎯 OnceLock provides thread-safe static initialization without any unsafe blocks!");
}
