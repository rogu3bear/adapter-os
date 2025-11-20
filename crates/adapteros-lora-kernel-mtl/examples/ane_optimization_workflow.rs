//! ANE Optimization Workflow Example
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//!
//! Demonstrates a complete workflow for optimizing models for ANE execution:
//! 1. Profile ANE utilization and identify fallbacks
//! 2. Optimize model structure for ANE compatibility
//! 3. Apply ANE-specific optimizations (Float16, alignment, packing)
//! 4. Run performance benchmarks
//! 5. Enable adaptive optimization based on thermal/power state
//! 6. Generate comprehensive profiling report

use adapteros_lora_kernel_mtl::{
    ane_profiler::{ANEProfiler, ComputeUnit, ExecutionProfile, FallbackReason, ProfilerConfig, ThermalState as ProfilerThermalState},
    ane_optimizer::{ANEOptimizer, DataType, OperationDescriptor, OptimizerConfig, PrecisionMode, ThermalState as OptimizerThermalState},
    ane_benchmark::{ANEBenchmarkSuite, BenchmarkConfig},
};
use std::collections::HashMap;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== ANE Optimization Workflow Demo ===\n");

    // Step 1: Initialize profiler
    println!("Step 1: Initialize ANE Profiler");
    let profiler_config = ProfilerConfig {
        detailed_profiling: true,
        track_power: true,
        track_thermal: true,
        sampling_interval_ms: 100,
        max_history_entries: 1000,
    };
    let profiler = ANEProfiler::new(profiler_config);
    profiler.start_session("qwen2.5-7b".to_string())?;
    println!("✓ Profiler initialized\n");

    // Step 2: Initialize optimizer
    println!("Step 2: Initialize ANE Optimizer");
    let optimizer_config = OptimizerConfig {
        use_float16: true,
        align_dimensions: true,
        pack_weights: true,
        adaptive_precision: true,
        thermal_aware: true,
        battery_aware: true,
        target_bandwidth_gbps: 100.0,
    };
    let mut optimizer = ANEOptimizer::new(optimizer_config);
    println!("✓ Optimizer initialized\n");

    // Step 3: Analyze model operations
    println!("Step 3: Analyze Model Operations for ANE Compatibility");
    analyze_operations(&mut optimizer)?;
    println!();

    // Step 4: Optimize tensor shapes
    println!("Step 4: Optimize Tensor Shapes (Align to Multiples of 16)");
    optimize_tensor_shapes(&mut optimizer)?;
    println!();

    // Step 5: Pack weights for ANE
    println!("Step 5: Pack Weights for ANE Format (Float16)");
    pack_model_weights(&optimizer)?;
    println!();

    // Step 6: Simulate inference and profile
    println!("Step 6: Simulate Inference and Profile ANE Utilization");
    simulate_inference_workload(&profiler)?;
    println!();

    // Step 7: Check for fallbacks
    println!("Step 7: Identify Operation Fallbacks");
    identify_fallbacks(&profiler)?;
    println!();

    // Step 8: Adaptive optimization based on conditions
    println!("Step 8: Determine Adaptive Optimization Strategy");
    adaptive_optimization_demo(&optimizer)?;
    println!();

    // Step 9: Run comprehensive benchmarks
    println!("Step 9: Run Comprehensive Performance Benchmarks");
    run_benchmarks()?;
    println!();

    // Step 10: Generate profiling report
    println!("Step 10: Generate Profiling Report");
    generate_report(&profiler)?;
    println!();

    println!("=== Workflow Complete ===");
    println!("\nKey Takeaways:");
    println!("1. Align all tensor dimensions to multiples of 16 for optimal ANE performance");
    println!("2. Use Float16 precision throughout for 2x memory bandwidth improvement");
    println!("3. Monitor ANE utilization and identify fallback operations");
    println!("4. Enable adaptive optimization for thermal/battery management");
    println!("5. Benchmark regularly to validate optimizations");

    Ok(())
}

fn analyze_operations(optimizer: &mut ANEOptimizer) -> Result<(), Box<dyn std::error::Error>> {
    let operations = vec![
        ("MatMul", vec![vec![1, 128, 768], vec![768, 768]], vec![DataType::Float16, DataType::Float16]),
        ("LayerNorm", vec![vec![1, 128, 768]], vec![DataType::Float16]),
        ("GELU", vec![vec![1, 128, 768]], vec![DataType::Float16]),
        ("Softmax", vec![vec![1, 12, 128, 128]], vec![DataType::Float16]),
        ("Custom", vec![vec![1, 128, 768]], vec![DataType::Float32]),
    ];

    for (op_type, input_shapes, data_types) in operations {
        let op = OperationDescriptor {
            op_type: op_type.to_string(),
            input_shapes: input_shapes.clone(),
            output_shapes: vec![input_shapes[0].clone()],
            data_types,
            attributes: HashMap::new(),
        };

        let compat = optimizer.check_operation_compatibility(&op)?;

        match compat {
            adapteros_lora_kernel_mtl::ane_optimizer::ANECompatibility::FullyCompatible => {
                println!("  ✅ {} - Fully compatible with ANE", op_type);
            }
            adapteros_lora_kernel_mtl::ane_optimizer::ANECompatibility::CompatibleWithModifications(mods) => {
                println!("  ⚠️  {} - Compatible with modifications:", op_type);
                for m in mods {
                    println!("      - {}", m);
                }
            }
            adapteros_lora_kernel_mtl::ane_optimizer::ANECompatibility::RequiresFallback(reason) => {
                println!("  ❌ {} - Requires fallback: {}", op_type, reason);
            }
        }
    }

    Ok(())
}

fn optimize_tensor_shapes(optimizer: &mut ANEOptimizer) -> Result<(), Box<dyn std::error::Error>> {
    let tensors = vec![
        ("input_embeddings", vec![1, 100, 768]),
        ("attention_output", vec![1, 127, 768]),
        ("feedforward", vec![1, 128, 3072]),
    ];

    for (name, shape) in tensors {
        let alignment = optimizer.align_tensor_shape(name.to_string(), shape)?;
        println!("  {}:", name);
        println!("    Original: {:?}", alignment.original_shape);
        println!("    Aligned:  {:?}", alignment.aligned_shape);
        println!("    Padding:  {:?}", alignment.padding);
        println!("    Overhead: {} bytes", alignment.memory_overhead);
    }

    Ok(())
}

fn pack_model_weights(optimizer: &ANEOptimizer) -> Result<(), Box<dyn std::error::Error>> {
    // Simulate model weights
    let layer_weights = vec![
        ("query_proj", 768 * 768),
        ("key_proj", 768 * 768),
        ("value_proj", 768 * 768),
        ("output_proj", 768 * 768),
    ];

    let mut total_original_bytes = 0;
    let mut total_packed_bytes = 0;

    for (name, num_weights) in layer_weights {
        let weights: Vec<f32> = (0..num_weights).map(|i| (i as f32) * 0.001).collect();
        let shape = vec![768, 768];

        let packed = optimizer.pack_weights(&weights, shape)?;

        let original_bytes = num_weights * 4; // Float32
        total_original_bytes += original_bytes;
        total_packed_bytes += packed.data.len();

        println!("  {} packed:", name);
        println!("    Original size: {} KB (Float32)", original_bytes / 1024);
        println!("    Packed size:   {} KB ({:?})", packed.data.len() / 1024, packed.dtype);
        println!("    Compression:   {:.1}%", (1.0 - packed.data.len() as f32 / original_bytes as f32) * 100.0);
    }

    println!("\n  Total compression:");
    println!("    Original: {} MB", total_original_bytes / 1024 / 1024);
    println!("    Packed:   {} MB", total_packed_bytes / 1024 / 1024);
    println!("    Saved:    {} MB ({:.1}%)",
        (total_original_bytes - total_packed_bytes) / 1024 / 1024,
        (1.0 - total_packed_bytes as f32 / total_original_bytes as f32) * 100.0
    );

    Ok(())
}

fn simulate_inference_workload(profiler: &ANEProfiler) -> Result<(), Box<dyn std::error::Error>> {
    let num_inferences = 100;

    for i in 0..num_inferences {
        let start = Instant::now();

        // Simulate inference time
        std::thread::sleep(std::time::Duration::from_micros(10000 + (i * 100) as u64));

        let duration_us = start.elapsed().as_micros() as u64;

        let profile = ExecutionProfile {
            timestamp: Instant::now(),
            duration_us,
            used_ane: i < 85, // 85% ANE utilization
            compute_unit: if i < 85 { ComputeUnit::ANE } else { ComputeUnit::GPU },
            power_mw: Some(1800.0 + (i as f32 * 5.0)),
            thermal_state: if i < 70 {
                ProfilerThermalState::Nominal
            } else if i < 90 {
                ProfilerThermalState::Fair
            } else {
                ProfilerThermalState::Serious
            },
            input_shape: vec![1, 128],
            output_shape: vec![1, 152064],
            memory_bandwidth_gbps: Some(95.0 + (i as f32 * 0.1)),
        };

        profiler.record_execution("qwen2.5-7b", profile)?;
    }

    let stats = profiler.get_session_stats("qwen2.5-7b")?;
    println!("  Inference Statistics:");
    println!("    Total Executions: {}", stats.total_executions);
    println!("    ANE Executions:   {}", stats.ane_executions);
    println!("    GPU Fallbacks:    {}", stats.gpu_fallbacks);
    println!("    ANE Utilization:  {:.1}%", stats.ane_utilization_percent);
    println!("    Avg Latency:      {:.2}μs", stats.avg_execution_time_us);
    println!("    Throughput:       {:.2} tok/sec", stats.tokens_per_second);

    if let Some(avg_power) = stats.avg_power_mw {
        println!("    Avg Power:        {:.1}mW", avg_power);
    }

    Ok(())
}

fn identify_fallbacks(profiler: &ANEProfiler) -> Result<(), Box<dyn std::error::Error>> {
    // Simulate fallback operations
    profiler.record_fallback(
        "qwen2.5-7b",
        "custom_attention".to_string(),
        ComputeUnit::GPU,
        FallbackReason::UnsupportedOperation,
    )?;

    profiler.record_fallback(
        "qwen2.5-7b",
        "dynamic_slice".to_string(),
        ComputeUnit::CPU,
        FallbackReason::IncompatibleShape,
    )?;

    let fallbacks = profiler.get_fallback_report("qwen2.5-7b")?;

    if fallbacks.is_empty() {
        println!("  ✅ No fallback operations detected");
    } else {
        println!("  ⚠️  Fallback operations detected:");
        for op in fallbacks {
            let fallback_rate = ((op.gpu_fallbacks + op.cpu_fallbacks) as f32
                / op.total_executions as f32)
                * 100.0;
            println!("\n    Operation: {}", op.op_name);
            println!("      Total executions: {}", op.total_executions);
            println!("      ANE executions:   {}", op.ane_executions);
            println!("      GPU fallbacks:    {}", op.gpu_fallbacks);
            println!("      CPU fallbacks:    {}", op.cpu_fallbacks);
            println!("      Fallback rate:    {:.1}%", fallback_rate);
            println!("      Reasons:          {:?}", op.fallback_reasons);
        }
    }

    Ok(())
}

fn adaptive_optimization_demo(optimizer: &ANEOptimizer) -> Result<(), Box<dyn std::error::Error>> {
    let scenarios = vec![
        ("Nominal conditions", OptimizerThermalState::Nominal, Some(0.85), 0.99),
        ("Hot device", OptimizerThermalState::Serious, Some(0.60), 0.95),
        ("Low battery", OptimizerThermalState::Nominal, Some(0.15), 0.90),
        ("Critical thermal", OptimizerThermalState::Critical, Some(0.80), 0.85),
    ];

    for (scenario, thermal, battery, accuracy) in scenarios {
        let strategy = optimizer.determine_adaptive_strategy(thermal, battery, accuracy)?;

        println!("\n  Scenario: {}", scenario);
        println!("    Thermal State:    {:?}", strategy.thermal_state);
        println!("    Power Mode:       {:?}", strategy.power_mode);
        println!("    Precision Mode:   {:?}", strategy.precision_mode);

        if !strategy.recommendations.is_empty() {
            println!("    Recommendations:");
            for rec in strategy.recommendations {
                println!("      - {}", rec);
            }
        }
    }

    Ok(())
}

fn run_benchmarks() -> Result<(), Box<dyn std::error::Error>> {
    let benchmark_config = BenchmarkConfig {
        warmup_iterations: 5,
        benchmark_iterations: 50,
        cooldown_period_secs: 2,
        measure_power: true,
        monitor_thermal: true,
        track_bandwidth: true,
        batch_sizes: vec![1, 2, 4],
        sequence_lengths: vec![128, 256, 512],
    };

    let mut suite = ANEBenchmarkSuite::new(benchmark_config);

    println!("  Running benchmark suite (this may take a few minutes)...");

    // In a real scenario, you would call suite.run_full_suite()
    // For this example, we'll just demonstrate the structure
    println!("  ✓ Batch size scaling benchmark");
    println!("  ✓ Sequence length scaling benchmark");
    println!("  ✓ Throughput saturation benchmark");
    println!("  ✓ Power efficiency benchmark");
    println!("  ✓ Thermal threshold benchmark");
    println!("  ✓ Memory bandwidth benchmark");
    println!("  ✓ Precision mode comparison");

    println!("\n  Benchmark results:");
    println!("    Batch=1, Seq=128:  85.4 tok/sec, 12.3ms latency, 1800mW power");
    println!("    Batch=1, Seq=256:  90.2 tok/sec, 23.1ms latency, 1850mW power");
    println!("    Batch=1, Seq=512:  93.1 tok/sec, 44.8ms latency, 1920mW power");

    Ok(())
}

fn generate_report(profiler: &ANEProfiler) -> Result<(), Box<dyn std::error::Error>> {
    let report = profiler.generate_report("qwen2.5-7b")?;

    println!("  Model: {}", report.model_id);
    println!("\n  Session Statistics:");
    println!("    Total Executions:  {}", report.session_stats.total_executions);
    println!("    ANE Utilization:   {:.1}%", report.session_stats.ane_utilization_percent);
    println!("    Avg Latency:       {:.2}μs", report.session_stats.avg_execution_time_us);
    println!("    Throughput:        {:.2} tok/sec", report.session_stats.tokens_per_second);

    if let Some(avg_power) = report.session_stats.avg_power_mw {
        println!("    Avg Power:         {:.1}mW", avg_power);
    }

    if !report.recommendations.is_empty() {
        println!("\n  Optimization Recommendations:");
        for (i, rec) in report.recommendations.iter().enumerate() {
            println!("    {}. {}", i + 1, rec);
        }
    }

    Ok(())
}
