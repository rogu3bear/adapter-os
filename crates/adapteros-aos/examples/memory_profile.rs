//! Memory profiling for AOS 2.0 adapter loading
//!
//! Measures actual memory usage during adapter loading.
//! Run with: cargo run --release --example memory_profile --features mmap
//!
//! Outputs:
//! - Peak memory usage
//! - Memory growth over time
//! - Comparison of different loading strategies

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;

#[derive(Serialize, Deserialize, Clone)]
struct TestManifest {
    version: String,
    adapter_id: String,
    weights_offset: u64,
    tensor_shapes: HashMap<String, Vec<usize>>,
}

/// Get current process memory usage (RSS) in bytes
#[cfg(target_os = "macos")]
fn get_memory_usage() -> Option<usize> {
    use std::process::Command;

    let output = Command::new("ps")
        .args(["-o", "rss=", "-p"])
        .arg(std::process::id().to_string())
        .output()
        .ok()?;

    let rss_kb = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<usize>()
        .ok()?;

    Some(rss_kb * 1024) // Convert KB to bytes
}

#[cfg(not(target_os = "macos"))]
fn get_memory_usage() -> Option<usize> {
    // Fallback for non-macOS systems
    // Read from /proc/self/status on Linux
    use std::io::Read;

    let mut file = File::open("/proc/self/status").ok()?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).ok()?;

    for line in contents.lines() {
        if line.starts_with("VmRSS:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let rss_kb = parts[1].parse::<usize>().ok()?;
                return Some(rss_kb * 1024); // Convert KB to bytes
            }
        }
    }

    None
}

/// Memory usage snapshot
#[derive(Debug, Clone)]
struct MemorySnapshot {
    timestamp: Duration,
    rss_bytes: usize,
    description: String,
}

/// Memory profiler that tracks usage over time
struct MemoryProfiler {
    start_time: Instant,
    snapshots: Vec<MemorySnapshot>,
    baseline: Option<usize>,
}

impl MemoryProfiler {
    fn new() -> Self {
        let mut profiler = Self {
            start_time: Instant::now(),
            snapshots: Vec::new(),
            baseline: None,
        };

        // Take baseline snapshot
        if let Some(mem) = get_memory_usage() {
            profiler.baseline = Some(mem);
            profiler.snapshot("baseline");
        }

        profiler
    }

    fn snapshot(&mut self, description: &str) {
        if let Some(rss) = get_memory_usage() {
            self.snapshots.push(MemorySnapshot {
                timestamp: self.start_time.elapsed(),
                rss_bytes: rss,
                description: description.to_string(),
            });
        }
    }

    fn report(&self) -> String {
        let mut output = String::new();
        output.push_str("=== Memory Profile Report ===\n\n");

        let baseline = self.baseline.unwrap_or(0);

        output.push_str(&format!(
            "Baseline Memory: {:.2} MB\n\n",
            baseline as f64 / (1024.0 * 1024.0)
        ));

        output.push_str("Snapshots:\n");
        for snapshot in &self.snapshots {
            let delta_mb = (snapshot.rss_bytes as f64 - baseline as f64) / (1024.0 * 1024.0);
            output.push_str(&format!(
                "{:8.3}s | {:12} | RSS: {:8.2} MB | Delta: {:+8.2} MB\n",
                snapshot.timestamp.as_secs_f64(),
                snapshot.description,
                snapshot.rss_bytes as f64 / (1024.0 * 1024.0),
                delta_mb
            ));
        }

        if let Some(peak) = self.snapshots.iter().map(|s| s.rss_bytes).max() {
            let peak_mb = peak as f64 / (1024.0 * 1024.0);
            let peak_delta_mb = (peak as f64 - baseline as f64) / (1024.0 * 1024.0);
            output.push_str(&format!(
                "\nPeak Memory: {:.2} MB (Delta: +{:.2} MB)\n",
                peak_mb, peak_delta_mb
            ));
        }

        output
    }
}

/// Create test archive with specified size
fn create_test_archive(
    num_tensors: usize,
    weights_size_mb: usize,
) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
    let mut tensor_shapes = HashMap::new();
    for i in 0..num_tensors {
        tensor_shapes.insert(format!("layer.{}.weight", i), vec![768, 768]);
    }

    let manifest = TestManifest {
        version: "2.0".to_string(),
        adapter_id: "memory-profile-test".to_string(),
        weights_offset: 8,
        tensor_shapes,
    };

    let temp_file = NamedTempFile::new()?;
    let weights_data = vec![0u8; weights_size_mb * 1024 * 1024];

    // Serialize manifest
    let manifest_json = serde_json::to_vec(&manifest)?;

    // Calculate offsets
    let header_size = 8;
    let manifest_offset = header_size + weights_data.len();
    let manifest_len = manifest_json.len();

    // Write archive
    let mut file = File::create(temp_file.path())?;

    file.write_all(&(manifest_offset as u32).to_le_bytes())?;
    file.write_all(&(manifest_len as u32).to_le_bytes())?;
    file.write_all(&weights_data)?;
    file.write_all(&manifest_json)?;

    file.flush()?;

    Ok(temp_file)
}

/// Profile regular file reading
fn profile_regular_read(profiler: &mut MemoryProfiler, path: &std::path::Path) {
    use std::io::Read;

    profiler.snapshot("before_regular_read");

    let mut file = File::open(path).expect("Failed to open file");
    let mut data = Vec::new();
    file.read_to_end(&mut data).expect("Failed to read file");

    profiler.snapshot("after_regular_read");

    // Keep data alive
    std::hint::black_box(data);

    profiler.snapshot("after_regular_read_blackbox");
}

/// Profile memory-mapped file reading
fn profile_mmap_read(profiler: &mut MemoryProfiler, path: &std::path::Path) {
    profiler.snapshot("before_mmap");

    let file = File::open(path).expect("Failed to open file");
    let mmap = unsafe { memmap2::Mmap::map(&file).expect("Failed to mmap file") };

    profiler.snapshot("after_mmap");

    // Access the data to ensure it's paged in
    let _checksum: u64 = mmap.iter().map(|&b| b as u64).sum();

    profiler.snapshot("after_mmap_access");

    // Keep mmap alive
    std::hint::black_box(mmap);

    profiler.snapshot("after_mmap_blackbox");
}

/// Main profiling entry point
fn main() {
    println!("AOS 2.0 Memory Profiling Tool\n");

    let test_configs = vec![
        ("Small", 10, 1),     // 10 tensors, 1MB
        ("Medium", 50, 10),   // 50 tensors, 10MB
        ("Large", 100, 50),   // 100 tensors, 50MB
        ("XLarge", 500, 100), // 500 tensors, 100MB
    ];

    for (name, num_tensors, size_mb) in test_configs {
        println!(
            "=== Testing {} Archive ({} tensors, {} MB) ===\n",
            name, num_tensors, size_mb
        );

        // Create test archive
        let archive =
            create_test_archive(num_tensors, size_mb).expect("Failed to create test archive");

        // Profile regular file reading
        println!("--- Regular File Read ---");
        let mut profiler = MemoryProfiler::new();
        profile_regular_read(&mut profiler, archive.path());
        println!("{}\n", profiler.report());

        // Give system time to reclaim memory
        std::thread::sleep(Duration::from_millis(500));

        // Profile memory-mapped reading
        println!("--- Memory-Mapped Read ---");
        let mut profiler = MemoryProfiler::new();
        profile_mmap_read(&mut profiler, archive.path());
        println!("{}\n", profiler.report());

        println!("\n{}\n", "=".repeat(80));
    }

    // Theoretical MPLoRA comparison
    println!("\n=== Theoretical Memory Savings (MPLoRA) ===\n");

    let num_adapters = 10;
    let adapter_size_mb = 50.0;
    let base_model_size_mb = 7000.0; // 7B model

    let traditional_lora_mb = base_model_size_mb + (num_adapters as f64 * adapter_size_mb);
    let mplora_mb = base_model_size_mb + adapter_size_mb; // Only one adapter in memory

    println!("Configuration:");
    println!("  Base model: {:.0} MB", base_model_size_mb);
    println!("  Per-adapter size: {:.0} MB", adapter_size_mb);
    println!("  Number of adapters: {}", num_adapters);
    println!();
    println!("Traditional LoRA (all in memory):");
    println!("  Total: {:.0} MB", traditional_lora_mb);
    println!();
    println!("MPLoRA (hot-swap from disk):");
    println!("  Total: {:.0} MB", mplora_mb);
    println!(
        "  Savings: {:.0} MB ({:.1}%)",
        traditional_lora_mb - mplora_mb,
        ((traditional_lora_mb - mplora_mb) / traditional_lora_mb) * 100.0
    );
}
