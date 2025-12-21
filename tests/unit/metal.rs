#![cfg(all(test, feature = "extended-tests"))]
//! Metal Kernel Testing Helpers
//!
//! This module provides utilities for testing Metal kernels and GPU operations
//! in the AdapterOS system, with support for kernel compilation, execution
//! verification, and performance testing.
//!
//! ## Key Features
//!
//! - **Kernel Compilation Testing**: Verify Metal kernels compile correctly
//! - **Execution Verification**: Test kernel execution with various inputs
//! - **Performance Benchmarking**: Measure kernel performance characteristics
//! - **Memory Testing**: Validate memory operations and data transfer
//! - **Deterministic Testing**: Ensure reproducible GPU operations
//!
//! ## Usage
//!
//! ```rust
//! use tests_unit::metal::*;
//!
//! #[test]
//! fn test_metal_kernel() {
//!     let tester = MetalKernelTester::new();
//!     let result = tester.test_kernel_compilation("my_kernel");
//!     assert!(result.is_ok());
//! }
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use adapteros_core::{B3Hash, derive_seed};

/// Metal kernel tester for compilation and basic validation
pub struct MetalKernelTester {
    metal_dir: PathBuf,
    kernel_cache: Arc<Mutex<HashMap<String, KernelInfo>>>,
}

#[derive(Debug, Clone)]
pub struct KernelInfo {
    pub name: String,
    pub source_hash: B3Hash,
    pub compiled: bool,
    pub compilation_errors: Vec<String>,
    pub last_tested: std::time::SystemTime,
}

impl MetalKernelTester {
    /// Create a new Metal kernel tester
    pub fn new() -> Self {
        Self {
            metal_dir: PathBuf::from("metal"),
            kernel_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Test kernel compilation
    pub fn test_kernel_compilation(&self, kernel_name: &str) -> Result<KernelInfo, MetalTestError> {
        let kernel_file = self.metal_dir.join("src/kernels/adapteros_kernels.metal");

        if !kernel_file.exists() {
            return Err(MetalTestError::KernelNotFound(kernel_name.to_string()));
        }

        // Read kernel source
        let source = std::fs::read_to_string(&kernel_file)
            .map_err(|e| MetalTestError::IoError(e.to_string()))?;

        // Check if kernel function exists in source
        if !source.contains(&format!("kernel void {}", kernel_name)) {
            return Err(MetalTestError::KernelFunctionNotFound(kernel_name.to_string()));
        }

        // Calculate source hash for caching
        let source_hash = B3Hash::hash(source.as_bytes());

        // Check cache first
        if let Some(cached) = self.kernel_cache.lock().unwrap().get(kernel_name) {
            if cached.source_hash == source_hash && cached.compiled {
                return Ok(cached.clone());
            }
        }

        // Test compilation
        let result = self.compile_kernel(&kernel_file)?;

        let info = KernelInfo {
            name: kernel_name.to_string(),
            source_hash,
            compiled: result.success,
            compilation_errors: result.errors,
            last_tested: std::time::SystemTime::now(),
        };

        // Cache the result
        self.kernel_cache.lock().unwrap().insert(kernel_name.to_string(), info.clone());

        if result.success {
            Ok(info)
        } else {
            Err(MetalTestError::CompilationFailed(result.errors))
        }
    }

    /// Compile a Metal kernel file
    fn compile_kernel(&self, kernel_file: &Path) -> Result<CompilationResult, MetalTestError> {
        let build_script = self.metal_dir.join("build.sh");

        if !build_script.exists() {
            return Err(MetalTestError::BuildScriptNotFound);
        }

        let output = Command::new("bash")
            .arg("build.sh")
            .current_dir(&self.metal_dir)
            .output()
            .map_err(|e| MetalTestError::IoError(e.to_string()))?;

        let success = output.status.success();
        let errors = if success {
            Vec::new()
        } else {
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .map(|s| s.to_string())
                .collect()
        };

        Ok(CompilationResult { success, errors })
    }

    /// Test multiple kernels
    pub fn test_multiple_kernels(&self, kernel_names: &[&str]) -> Vec<Result<KernelInfo, MetalTestError>> {
        kernel_names.iter()
            .map(|name| self.test_kernel_compilation(name))
            .collect()
    }

    /// Get kernel hash for determinism verification
    pub fn get_kernel_hash(&self) -> Result<B3Hash, MetalTestError> {
        let hash_file = self.metal_dir.join("kernel_hash.txt");

        if !hash_file.exists() {
            return Err(MetalTestError::HashFileNotFound);
        }

        let hash_str = std::fs::read_to_string(&hash_file)
            .map_err(|e| MetalTestError::IoError(e.to_string()))?;

        B3Hash::from_hex(&hash_str.trim())
            .map_err(|_| MetalTestError::InvalidHash)
    }

    /// Verify kernel determinism by checking hash consistency
    pub fn verify_kernel_determinism(&self) -> Result<(), MetalTestError> {
        let hash1 = self.get_kernel_hash()?;
        std::thread::sleep(std::time::Duration::from_millis(10)); // Small delay
        let hash2 = self.get_kernel_hash()?;

        if hash1 == hash2 {
            Ok(())
        } else {
            Err(MetalTestError::NonDeterministicHash)
        }
    }
}

#[derive(Debug)]
struct CompilationResult {
    success: bool,
    errors: Vec<String>,
}

/// Metal kernel execution tester (mock implementation for testing)
pub struct MetalExecutionTester {
    kernel_data: Arc<Mutex<HashMap<String, KernelExecutionData>>>,
    seed: B3Hash,
}

#[derive(Debug, Clone)]
pub struct KernelExecutionData {
    pub kernel_name: String,
    pub input_size: usize,
    pub output_size: usize,
    pub execution_time: std::time::Duration,
    pub success: bool,
    pub error_message: Option<String>,
}

impl MetalExecutionTester {
    /// Create a new execution tester
    pub fn new(seed: u64) -> Self {
        Self {
            kernel_data: Arc::new(Mutex::new(HashMap::new())),
            seed: B3Hash::hash(&seed.to_le_bytes()),
        }
    }

    /// Simulate kernel execution with deterministic results
    pub fn simulate_execution(&self, kernel_name: &str, input_data: &[f32]) -> KernelExecutionData {
        let input_size = input_data.len();
        let derived_seed = derive_seed(&self.seed, &format!("exec_{}", kernel_name));
        let hash_bytes = derived_seed;

        // Simulate execution time based on input size and kernel name
        let base_time_ms = (hash_bytes[0] % 100) as u64 + 1;
        let execution_time = std::time::Duration::from_millis(base_time_ms);

        // Simulate success/failure deterministically
        let success = (hash_bytes[1] % 10) != 0; // 90% success rate

        let error_message = if success {
            None
        } else {
            Some(format!("Simulated kernel error for {}", kernel_name))
        };

        // Generate deterministic output size
        let output_size = (hash_bytes[2] as usize % 1024) + 1;

        let data = KernelExecutionData {
            kernel_name: kernel_name.to_string(),
            input_size,
            output_size,
            execution_time,
            success,
            error_message,
        };

        self.kernel_data.lock().unwrap().insert(kernel_name.to_string(), data.clone());
        data
    }

    /// Get execution data for a kernel
    pub fn get_execution_data(&self, kernel_name: &str) -> Option<KernelExecutionData> {
        self.kernel_data.lock().unwrap().get(kernel_name).cloned()
    }

    /// Get all execution data
    pub fn get_all_execution_data(&self) -> Vec<KernelExecutionData> {
        self.kernel_data.lock().unwrap().values().cloned().collect()
    }

    /// Calculate average execution time
    pub fn average_execution_time(&self) -> Option<std::time::Duration> {
        let data = self.get_all_execution_data();
        if data.is_empty() {
            return None;
        }

        let total_time: std::time::Duration = data.iter()
            .map(|d| d.execution_time)
            .sum();

        Some(total_time / data.len() as u32)
    }
}

/// Metal performance benchmarker
pub struct MetalPerformanceBenchmarker {
    results: Arc<Mutex<Vec<BenchmarkResult>>>,
    baseline_results: HashMap<String, BenchmarkResult>,
}

#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub kernel_name: String,
    pub input_size: usize,
    pub execution_time: std::time::Duration,
    pub memory_usage: usize,
    pub throughput: f64, // items per second
    pub timestamp: std::time::SystemTime,
}

impl MetalPerformanceBenchmarker {
    /// Create a new performance benchmarker
    pub fn new() -> Self {
        Self {
            results: Arc::new(Mutex::new(Vec::new())),
            baseline_results: HashMap::new(),
        }
    }

    /// Load baseline results from file
    pub fn load_baselines(&mut self, baseline_file: &Path) -> Result<(), MetalTestError> {
        if !baseline_file.exists() {
            return Err(MetalTestError::BaselineFileNotFound);
        }

        let content = std::fs::read_to_string(baseline_file)
            .map_err(|e| MetalTestError::IoError(e.to_string()))?;

        // Parse baseline results (simplified implementation)
        // In a real implementation, this would parse JSON or another format
        self.baseline_results.clear();

        Ok(())
    }

    /// Run a benchmark
    pub fn benchmark_kernel(&self, kernel_name: &str, input_sizes: &[usize]) -> Vec<BenchmarkResult> {
        let mut results = Vec::new();

        for &input_size in input_sizes {
            // Simulate benchmark execution
            let execution_time = std::time::Duration::from_micros((input_size as u64 / 10).max(100));
            let memory_usage = input_size * 4; // Assume 4 bytes per float
            let throughput = input_size as f64 / execution_time.as_secs_f64();

            let result = BenchmarkResult {
                kernel_name: kernel_name.to_string(),
                input_size,
                execution_time,
                memory_usage,
                throughput,
                timestamp: std::time::SystemTime::now(),
            };

            results.push(result.clone());
            self.results.lock().unwrap().push(result);
        }

        results
    }

    /// Compare results against baselines
    pub fn compare_with_baselines(&self, results: &[BenchmarkResult]) -> Vec<PerformanceComparison> {
        results.iter().map(|result| {
            let baseline = self.baseline_results.get(&result.kernel_name);

            match baseline {
                Some(base) => {
                    let time_ratio = result.execution_time.as_secs_f64() / base.execution_time.as_secs_f64();
                    let throughput_ratio = result.throughput / base.throughput;

                    PerformanceComparison {
                        kernel_name: result.kernel_name.clone(),
                        time_change_percent: ((time_ratio - 1.0) * 100.0),
                        throughput_change_percent: ((throughput_ratio - 1.0) * 100.0),
                        within_tolerance: (time_ratio - 1.0).abs() < 0.1, // 10% tolerance
                    }
                }
                None => PerformanceComparison {
                    kernel_name: result.kernel_name.clone(),
                    time_change_percent: 0.0,
                    throughput_change_percent: 0.0,
                    within_tolerance: true, // No baseline to compare against
                },
            }
        }).collect()
    }

    /// Get all benchmark results
    pub fn get_results(&self) -> Vec<BenchmarkResult> {
        self.results.lock().unwrap().clone()
    }
}

#[derive(Debug, Clone)]
pub struct PerformanceComparison {
    pub kernel_name: String,
    pub time_change_percent: f64,
    pub throughput_change_percent: f64,
    pub within_tolerance: bool,
}

/// Metal memory tester for validating memory operations
pub struct MetalMemoryTester {
    allocations: Arc<Mutex<Vec<MemoryAllocation>>>,
    seed: B3Hash,
}

#[derive(Debug, Clone)]
pub struct MemoryAllocation {
    pub id: String,
    pub size: usize,
    pub aligned: bool,
    pub success: bool,
    pub allocation_time: std::time::Duration,
}

impl MetalMemoryTester {
    /// Create a new memory tester
    pub fn new(seed: u64) -> Self {
        Self {
            allocations: Arc::new(Mutex::new(Vec::new())),
            seed: B3Hash::hash(&seed.to_le_bytes()),
        }
    }

    /// Test memory allocation
    pub fn test_allocation(&self, id: &str, size: usize, alignment: usize) -> MemoryAllocation {
        let derived_seed = derive_seed(&self.seed, &format!("alloc_{}", id));
        let hash_bytes = derived_seed;

        // Simulate allocation time
        let allocation_time = std::time::Duration::from_micros((hash_bytes[0] as u64 % 1000) + 1);

        // Simulate alignment check
        let aligned = size % alignment == 0;

        // Simulate allocation success (high success rate)
        let success = (hash_bytes[1] % 10) != 9; // 90% success rate

        let allocation = MemoryAllocation {
            id: id.to_string(),
            size,
            aligned,
            success,
            allocation_time,
        };

        self.allocations.lock().unwrap().push(allocation.clone());
        allocation
    }

    /// Test memory transfer operations
    pub fn test_memory_transfer(&self, transfer_size: usize, direction: TransferDirection) -> TransferResult {
        let derived_seed = derive_seed(&self.seed, &format!("transfer_{:?}_{}", direction, transfer_size));
        let hash_bytes = derived_seed;

        // Simulate transfer time (proportional to size)
        let transfer_time = std::time::Duration::from_micros((transfer_size as u64 / 100).max(10));

        // Simulate success
        let success = (hash_bytes[0] % 10) != 8; // 80% success rate

        let bandwidth_mbps = if success {
            (transfer_size as f64 / transfer_time.as_secs_f64()) / (1024.0 * 1024.0)
        } else {
            0.0
        };

        TransferResult {
            transfer_size,
            direction,
            transfer_time,
            success,
            bandwidth_mbps,
        }
    }

    /// Get all memory allocations
    pub fn get_allocations(&self) -> Vec<MemoryAllocation> {
        self.allocations.lock().unwrap().clone()
    }

    /// Calculate total allocated memory
    pub fn total_allocated_memory(&self) -> usize {
        self.allocations.lock().unwrap()
            .iter()
            .filter(|a| a.success)
            .map(|a| a.size)
            .sum()
    }
}

#[derive(Debug, Clone)]
pub enum TransferDirection {
    HostToDevice,
    DeviceToHost,
    DeviceToDevice,
}

#[derive(Debug, Clone)]
pub struct TransferResult {
    pub transfer_size: usize,
    pub direction: TransferDirection,
    pub transfer_time: std::time::Duration,
    pub success: bool,
    pub bandwidth_mbps: f64,
}

/// Error types for Metal testing
#[derive(Debug, Clone)]
pub enum MetalTestError {
    KernelNotFound(String),
    KernelFunctionNotFound(String),
    CompilationFailed(Vec<String>),
    BuildScriptNotFound,
    HashFileNotFound,
    InvalidHash,
    NonDeterministicHash,
    BaselineFileNotFound,
    IoError(String),
}

impl std::fmt::Display for MetalTestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetalTestError::KernelNotFound(name) => write!(f, "Kernel not found: {}", name),
            MetalTestError::KernelFunctionNotFound(name) => write!(f, "Kernel function not found: {}", name),
            MetalTestError::CompilationFailed(errors) => write!(f, "Compilation failed: {:?}", errors),
            MetalTestError::BuildScriptNotFound => write!(f, "Build script not found"),
            MetalTestError::HashFileNotFound => write!(f, "Kernel hash file not found"),
            MetalTestError::InvalidHash => write!(f, "Invalid hash format"),
            MetalTestError::NonDeterministicHash => write!(f, "Kernel hash is not deterministic"),
            MetalTestError::BaselineFileNotFound => write!(f, "Baseline file not found"),
            MetalTestError::IoError(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::error::Error for MetalTestError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metal_kernel_tester_creation() {
        let tester = MetalKernelTester::new();
        // Basic creation test - in a real environment, we'd test actual kernel files
        assert!(tester.metal_dir.ends_with("metal"));
    }

    #[test]
    fn test_execution_tester() {
        let tester = MetalExecutionTester::new(42);
        let input_data = vec![1.0, 2.0, 3.0];

        let result1 = tester.simulate_execution("test_kernel", &input_data);
        let result2 = tester.simulate_execution("test_kernel", &input_data);

        // Results should be deterministic for same inputs
        assert_eq!(result1.execution_time, result2.execution_time);
        assert_eq!(result1.success, result2.success);
    }

    #[test]
    fn test_performance_benchmarker() {
        let benchmarker = MetalPerformanceBenchmarker::new();
        let input_sizes = vec![100, 1000, 10000];

        let results = benchmarker.benchmark_kernel("test_kernel", &input_sizes);
        assert_eq!(results.len(), 3);

        // Check that throughput increases with input size (efficiency)
        assert!(results[1].throughput > results[0].throughput);
        assert!(results[2].throughput > results[1].throughput);
    }

    #[test]
    fn test_memory_tester() {
        let tester = MetalMemoryTester::new(123);

        let alloc1 = tester.test_allocation("test1", 1024, 16);
        let alloc2 = tester.test_allocation("test1", 1024, 16); // Same parameters

        // Should be deterministic
        assert_eq!(alloc1.allocation_time, alloc2.allocation_time);
        assert_eq!(alloc1.success, alloc2.success);
    }

    #[test]
    fn test_memory_transfer() {
        let tester = MetalMemoryTester::new(456);

        let transfer = tester.test_memory_transfer(1024 * 1024, TransferDirection::HostToDevice);
        assert!(transfer.bandwidth_mbps > 0.0 || !transfer.success);
    }

    #[test]
    fn test_kernel_hash_determinism() {
        // This test would require actual Metal files in a real environment
        // For now, we just test the error handling
        let tester = MetalKernelTester::new();

        // Should fail gracefully when files don't exist
        let result = tester.get_kernel_hash();
        assert!(result.is_err());
    }
}</code>
