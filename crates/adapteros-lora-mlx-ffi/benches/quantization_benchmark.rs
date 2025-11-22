//! Benchmarks comparing quantized vs full-precision model performance
//!
//! Measures:
//! - Quantization/dequantization speed
//! - Compression ratios
//! - Inference latency impact
//! - Memory usage

use adapteros_lora_mlx_ffi::quantization::{MLXQuantizer, QuantizationConfig};
use std::time::Instant;

/// Benchmark configuration
#[derive(Debug)]
struct BenchmarkConfig {
    /// Tensor sizes to test (number of elements)
    tensor_sizes: Vec<usize>,
    /// Group sizes to test
    group_sizes: Vec<usize>,
    /// Number of iterations per benchmark
    iterations: usize,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            tensor_sizes: vec![1024, 4096, 16384, 65536, 262144],
            group_sizes: vec![32, 64, 128],
            iterations: 10,
        }
    }
}

/// Result of a single benchmark
#[derive(Debug)]
struct BenchmarkResult {
    tensor_size: usize,
    group_size: usize,
    quantization_type: String,
    avg_quantization_time_us: f64,
    avg_dequantization_time_us: f64,
    compression_ratio: f32,
    throughput_mb_per_sec: f32,
}

impl BenchmarkResult {
    fn print_row(&self) {
        println!(
            "{:>10} | {:>6} | {:>8} | {:>18.2} us | {:>18.2} us | {:>6.2}x | {:>10.2} MB/s",
            self.tensor_size,
            self.group_size,
            self.quantization_type,
            self.avg_quantization_time_us,
            self.avg_dequantization_time_us,
            self.compression_ratio,
            self.throughput_mb_per_sec,
        );
    }
}

/// Run quantization benchmarks
fn benchmark_quantization(config: &BenchmarkConfig) -> Vec<BenchmarkResult> {
    let mut results = Vec::new();

    println!("\n=== MLX Quantization Benchmarks ===\n");
    println!("Testing INT8 and INT4 quantization performance");
    println!("(Higher throughput = better performance)\n");
    println!(
        "{:>10} | {:>6} | {:>8} | {:>18} | {:>18} | {:>6} | {:>10}",
        "Elements", "Group", "Method", "Quantize (μs)", "Dequantize (μs)", "Ratio", "Throughput"
    );
    println!("{}", "-".repeat(100));

    for &tensor_size in &config.tensor_sizes {
        for &group_size in &config.group_sizes {
            if group_size > tensor_size {
                continue;
            }

            // Generate random test data
            let data: Vec<f32> = (0..tensor_size)
                .map(|i| ((i as f32).sin() * 0.5 + 0.5))
                .collect();

            let shape = vec![tensor_size as i32];

            // Benchmark INT8
            let result_int8 = benchmark_int8(
                &data,
                tensor_size,
                group_size,
                shape.clone(),
                config.iterations,
            );
            result_int8.print_row();
            results.push(result_int8);

            // Benchmark INT4
            let result_int4 =
                benchmark_int4(&data, tensor_size, group_size, shape, config.iterations);
            result_int4.print_row();
            results.push(result_int4);
        }
    }

    results
}

fn benchmark_int8(
    data: &[f32],
    tensor_size: usize,
    group_size: usize,
    shape: Vec<i32>,
    iterations: usize,
) -> BenchmarkResult {
    let mut quantize_times = Vec::new();
    let mut dequantize_times = Vec::new();

    for _ in 0..iterations {
        // Benchmark quantization
        let start = Instant::now();
        let quantized = MLXQuantizer::quantize_int8(data, group_size, &shape).unwrap();
        let quantize_elapsed = start.elapsed().as_micros() as f64;
        quantize_times.push(quantize_elapsed);

        // Benchmark dequantization
        let start = Instant::now();
        let _dequantized = MLXQuantizer::dequantize_int8(&quantized).unwrap();
        let dequantize_elapsed = start.elapsed().as_micros() as f64;
        dequantize_times.push(dequantize_elapsed);
    }

    let avg_quantize = quantize_times.iter().sum::<f64>() / iterations as f64;
    let avg_dequantize = dequantize_times.iter().sum::<f64>() / iterations as f64;

    let original_size_bytes = tensor_size * 4; // f32
    let compressed_size_bytes = tensor_size; // i8
    let compression_ratio = original_size_bytes as f32 / compressed_size_bytes as f32;

    // Throughput in MB/s based on quantization time
    let throughput = (original_size_bytes as f64 / (1024.0 * 1024.0)) / (avg_quantize / 1e6);

    BenchmarkResult {
        tensor_size,
        group_size,
        quantization_type: "INT8".to_string(),
        avg_quantization_time_us: avg_quantize,
        avg_dequantization_time_us: avg_dequantize,
        compression_ratio,
        throughput_mb_per_sec: throughput as f32,
    }
}

fn benchmark_int4(
    data: &[f32],
    tensor_size: usize,
    group_size: usize,
    shape: Vec<i32>,
    iterations: usize,
) -> BenchmarkResult {
    let mut quantize_times = Vec::new();
    let mut dequantize_times = Vec::new();

    for _ in 0..iterations {
        // Benchmark quantization
        let start = Instant::now();
        let quantized = MLXQuantizer::quantize_int4(data, group_size, &shape).unwrap();
        let quantize_elapsed = start.elapsed().as_micros() as f64;
        quantize_times.push(quantize_elapsed);

        // Benchmark dequantization
        let start = Instant::now();
        let _dequantized = MLXQuantizer::dequantize_int4(&quantized).unwrap();
        let dequantize_elapsed = start.elapsed().as_micros() as f64;
        dequantize_times.push(dequantize_elapsed);
    }

    let avg_quantize = quantize_times.iter().sum::<f64>() / iterations as f64;
    let avg_dequantize = dequantize_times.iter().sum::<f64>() / iterations as f64;

    let original_size_bytes = tensor_size * 4; // f32
    let compressed_size_bytes = (tensor_size + 1) / 2; // 2 values per byte
    let compression_ratio = original_size_bytes as f32 / compressed_size_bytes as f32;

    // Throughput in MB/s
    let throughput = (original_size_bytes as f64 / (1024.0 * 1024.0)) / (avg_quantize / 1e6);

    BenchmarkResult {
        tensor_size,
        group_size,
        quantization_type: "INT4".to_string(),
        avg_quantization_time_us: avg_quantize,
        avg_dequantization_time_us: avg_dequantize,
        compression_ratio,
        throughput_mb_per_sec: throughput as f32,
    }
}

/// Benchmark quantization accuracy
fn benchmark_accuracy(config: &BenchmarkConfig) {
    use adapteros_lora_mlx_ffi::quantization::MLXQuantizer;

    println!("\n=== Quantization Accuracy Benchmarks ===\n");
    println!(
        "{:>10} | {:>6} | {:>8} | {:>12} | {:>12} | {:>10}",
        "Elements", "Group", "Method", "Mean Error", "Max Error", "SNR (dB)"
    );
    println!("{}", "-".repeat(75));

    for &tensor_size in &config.tensor_sizes[..std::cmp::min(3, config.tensor_sizes.len())] {
        for &group_size in &config.group_sizes {
            if group_size > tensor_size {
                continue;
            }

            // Generate test data with known distribution
            let data: Vec<f32> = (0..tensor_size)
                .map(|i| ((i as f32 * 0.1).sin() * 0.8))
                .collect();

            let shape = vec![tensor_size as i32];

            // Test INT8
            let quantized = MLXQuantizer::quantize_int8(&data, group_size, shape.clone()).unwrap();
            let stats = MLXQuantizer::calculate_stats(&data, &quantized).unwrap();
            println!(
                "{:>10} | {:>6} | {:>8} | {:>12.8} | {:>12.8} | {:>10.2}",
                tensor_size, group_size, "INT8", stats.mean_error, stats.max_error, stats.snr_db
            );

            // Test INT4
            let quantized = MLXQuantizer::quantize_int4(&data, group_size, shape).unwrap();
            let stats = MLXQuantizer::calculate_stats(&data, &quantized).unwrap();
            println!(
                "{:>10} | {:>6} | {:>8} | {:>12.8} | {:>12.8} | {:>10.2}",
                tensor_size, group_size, "INT4", stats.mean_error, stats.max_error, stats.snr_db
            );
        }
    }
}

/// Benchmark memory usage patterns
fn benchmark_memory_efficiency(config: &BenchmarkConfig) {
    println!("\n=== Memory Efficiency Analysis ===\n");

    let mut total_savings = 0u64;
    let mut num_tensors = 0;

    for &tensor_size in &config.tensor_sizes {
        for &group_size in &config.group_sizes {
            if group_size > tensor_size {
                continue;
            }

            let data: Vec<f32> = (0..tensor_size).map(|i| ((i as f32).sin() * 0.5)).collect();

            let shape = vec![tensor_size as i32];

            // Quantize to INT8
            let quantized_int8 =
                MLXQuantizer::quantize_int8(&data, group_size, shape.clone()).unwrap();
            let original_size = tensor_size * 4; // float32
            let int8_size = quantized_int8.data.len();
            let int8_savings = original_size - int8_size;

            // Quantize to INT4
            let quantized_int4 = MLXQuantizer::quantize_int4(&data, group_size, shape).unwrap();
            let int4_size = quantized_int4.data.len();
            let int4_savings = original_size - int4_size;

            total_savings += (int8_savings + int4_savings) as u64;
            num_tensors += 2;

            if num_tensors <= 10 {
                println!(
                    "Tensor size: {} elements | INT8: {:.1}% saved | INT4: {:.1}% saved",
                    tensor_size,
                    (int8_savings as f32 / original_size as f32) * 100.0,
                    (int4_savings as f32 / original_size as f32) * 100.0
                );
            }
        }
    }

    let avg_savings_mb = total_savings as f32 / (1024.0 * 1024.0) / num_tensors as f32;
    println!("\nAverage savings per tensor: {:.2} MB", avg_savings_mb);
}

fn main() {
    let config = BenchmarkConfig::default();

    // Run benchmarks
    let _results = benchmark_quantization(&config);
    benchmark_accuracy(&config);
    benchmark_memory_efficiency(&config);

    println!("\n=== Summary ===");
    println!("Benchmarks completed successfully");
    println!("INT8: Best for moderate compression with high accuracy");
    println!("INT4: Best for maximum compression with acceptable quality loss");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_int8() {
        let config = BenchmarkConfig {
            tensor_sizes: vec![1024],
            group_sizes: vec![64],
            iterations: 2,
        };

        let data: Vec<f32> = (0..1024).map(|i| (i as f32 / 1024.0)).collect();
        let result = benchmark_int8(&data, 1024, 64, vec![1024], 2);

        assert!(result.avg_quantization_time_us > 0.0);
        assert!(result.compression_ratio > 1.0);
        assert!(result.throughput_mb_per_sec > 0.0);
    }

    #[test]
    fn test_benchmark_int4() {
        let config = BenchmarkConfig {
            tensor_sizes: vec![1024],
            group_sizes: vec![64],
            iterations: 2,
        };

        let data: Vec<f32> = (0..1024).map(|i| (i as f32 / 1024.0)).collect();
        let result = benchmark_int4(&data, 1024, 64, vec![1024], 2);

        assert!(result.avg_quantization_time_us > 0.0);
        assert!(result.compression_ratio >= 7.0); // INT4 should compress at least 7x
        assert!(result.throughput_mb_per_sec > 0.0);
    }
}
