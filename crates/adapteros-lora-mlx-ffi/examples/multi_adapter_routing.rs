//! Example: K-Sparse Multi-Adapter Routing with MLX Backend
//!
//! This example demonstrates how to use the multi-adapter LoRA routing
//! functionality with Q15 quantized gates.

use adapteros_lora_mlx_ffi::*;
use std::ptr;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MLX Multi-Adapter Routing Example ===\n");

    // Step 1: Create test input tensor
    println!("1. Creating test input tensor...");
    let hidden_dim = 128;
    let seq_len = 16;
    let input_data: Vec<f32> = (0..seq_len * hidden_dim)
        .map(|i| (i as f32) * 0.01)
        .collect();

    let input_array = unsafe { mlx_array_from_data(input_data.as_ptr(), input_data.len() as i32) };
    if input_array.is_null() {
        eprintln!("Failed to create input array");
        return Err("Input array creation failed".into());
    }
    println!(
        "   Input shape: [seq_len={}, hidden_dim={}]",
        seq_len, hidden_dim
    );

    // Step 2: Create LoRA adapter matrices (A and B)
    println!("\n2. Creating LoRA adapter matrices...");
    let rank = 8;
    let num_adapters = 3;

    // Create LoRA A matrices: [hidden_dim, rank]
    let mut lora_a_list: Vec<*mut mlx_array_t> = Vec::new();
    for i in 0..num_adapters {
        let a_data: Vec<f32> = (0..hidden_dim * rank)
            .map(|j| ((j as f32) + (i as f32) * 100.0) * 0.001)
            .collect();
        let a_array = unsafe { mlx_array_from_data(a_data.as_ptr(), a_data.len() as i32) };
        if a_array.is_null() {
            eprintln!("Failed to create LoRA A matrix for adapter {}", i);
            cleanup_arrays(&lora_a_list);
            unsafe { mlx_array_free(input_array) };
            return Err("LoRA A matrix creation failed".into());
        }
        lora_a_list.push(a_array);
    }

    // Create LoRA B matrices: [rank, hidden_dim]
    let mut lora_b_list: Vec<*mut mlx_array_t> = Vec::new();
    for i in 0..num_adapters {
        let b_data: Vec<f32> = (0..rank * hidden_dim)
            .map(|j| ((j as f32) + (i as f32) * 200.0) * 0.002)
            .collect();
        let b_array = unsafe { mlx_array_from_data(b_data.as_ptr(), b_data.len() as i32) };
        if b_array.is_null() {
            eprintln!("Failed to create LoRA B matrix for adapter {}", i);
            cleanup_arrays(&lora_a_list);
            cleanup_arrays(&lora_b_list);
            unsafe { mlx_array_free(input_array) };
            return Err("LoRA B matrix creation failed".into());
        }
        lora_b_list.push(b_array);
    }
    println!("   Created {} adapters with rank {}", num_adapters, rank);

    // Step 3: Define routing gates (Q15 quantized)
    println!("\n3. Defining routing gates...");
    let gates_float = vec![1.0, 0.75, 0.5]; // Normalized gate weights
    let gates_q15: Vec<u16> = gates_float
        .iter()
        .map(|&g| (g * 32767.0).round() as u16)
        .collect();

    println!("   Gate weights (float): {:?}", gates_float);
    println!("   Gate weights (Q15):   {:?}", gates_q15);

    // Step 4: Run multi-adapter forward pass
    println!("\n4. Running multi-adapter LoRA forward pass...");
    let alpha = 16.0;
    let rank_float = rank as f32;

    unsafe {
        mlx_clear_error();
    }

    let output_array = unsafe {
        mlx_multi_lora_forward(
            input_array,
            lora_a_list.as_ptr(),
            lora_b_list.as_ptr(),
            num_adapters as i32,
            gates_q15.as_ptr(),
            alpha,
            rank_float,
        )
    };

    if output_array.is_null() {
        let error_msg = unsafe {
            let err_ptr = mlx_get_last_error();
            if !err_ptr.is_null() {
                std::ffi::CStr::from_ptr(err_ptr)
                    .to_string_lossy()
                    .to_string()
            } else {
                "Unknown error".to_string()
            }
        };
        eprintln!("Multi-adapter forward failed: {}", error_msg);

        // Cleanup
        cleanup_arrays(&lora_a_list);
        cleanup_arrays(&lora_b_list);
        unsafe { mlx_array_free(input_array) };
        return Err(error_msg.into());
    }

    // Step 5: Extract and display results
    println!("\n5. Extracting results...");
    let output_size = unsafe { mlx_array_size(output_array) };
    let output_ptr = unsafe { mlx_array_data(output_array) };

    if !output_ptr.is_null() {
        let output_slice = unsafe { std::slice::from_raw_parts(output_ptr, output_size as usize) };
        println!("   Output size: {}", output_size);
        println!(
            "   First 5 values: {:?}",
            &output_slice[..5.min(output_size as usize)]
        );
        println!(
            "   Last 5 values:  {:?}",
            &output_slice[(output_size as usize).saturating_sub(5)..]
        );

        // Calculate statistics
        let sum: f32 = output_slice.iter().sum();
        let mean = sum / output_size as f32;
        let min = output_slice.iter().fold(f32::INFINITY, |a, &b| a.min(b));
        let max = output_slice
            .iter()
            .fold(f32::NEG_INFINITY, |a, &b| a.max(b));

        println!("\n   Statistics:");
        println!("     Mean: {:.6}", mean);
        println!("     Min:  {:.6}", min);
        println!("     Max:  {:.6}", max);
    } else {
        println!("   Warning: Could not access output data");
    }

    // Step 6: Cleanup
    println!("\n6. Cleaning up...");
    cleanup_arrays(&lora_a_list);
    cleanup_arrays(&lora_b_list);
    unsafe {
        mlx_array_free(input_array);
        mlx_array_free(output_array);
    }

    println!("\n=== Example completed successfully! ===");
    Ok(())
}

/// Helper function to cleanup array lists
fn cleanup_arrays(arrays: &[*mut mlx_array_t]) {
    for &array in arrays {
        if !array.is_null() {
            unsafe {
                mlx_array_free(array);
            }
        }
    }
}
