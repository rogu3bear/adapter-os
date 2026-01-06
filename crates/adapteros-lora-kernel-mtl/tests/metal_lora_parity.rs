#![allow(clippy::needless_range_loop)]

#[cfg(target_os = "macos")]
#[test]
fn metal_lora_parity_against_cpu_flat() {
    use metal::{CompileOptions, Device, MTLResourceOptions, MTLSize};

    fn cpu_lora_flat(
        input: &[f32],
        a_row_major: &[f32],
        b_row_major: &[f32],
        rank: usize,
        hidden: usize,
        alpha: f32,
    ) -> Vec<f32> {
        assert_eq!(a_row_major.len(), rank * hidden);
        assert_eq!(b_row_major.len(), hidden * rank);
        let len = input.len().min(hidden);
        let mut intermediate = vec![0.0f32; rank];
        for r in 0..rank {
            let base = r * hidden;
            let mut acc = 0.0f32;
            for h in 0..len {
                acc += input[h] * a_row_major[base + h];
            }
            intermediate[r] = acc;
        }
        let mut output = vec![0.0f32; hidden];
        for h in 0..hidden {
            let base = h * rank;
            let mut acc = 0.0f32;
            for r in 0..rank {
                acc += intermediate[r] * b_row_major[base + r];
            }
            output[h] = acc * (alpha / rank as f32);
        }
        output
    }

    // Test a matrix of shapes and alphas
    let shapes = &[(4usize, 1usize), (4, 2), (8, 2), (8, 4), (16, 4)];
    let alphas = &[8.0f32, 16.0, 32.0];

    // Build a minimal Metal kernel implementing the same math
    let msl = r#"#include <metal_stdlib>
using namespace metal;

kernel void lora_flat(
    device const float* input   [[ buffer(0) ]],
    device const float* a       [[ buffer(1) ]], // rank x hidden (row-major)
    device const float* b       [[ buffer(2) ]], // hidden x rank (row-major)
    device float*       output  [[ buffer(3) ]],
    constant uint&      rank    [[ buffer(4) ]],
    constant uint&      hidden  [[ buffer(5) ]],
    constant float&     alpha   [[ buffer(6) ]],
    uint3               tid     [[ thread_position_in_grid ]]
) {
    uint h = tid.x;
    if (h >= hidden) return;

    // intermediate[r] = sum_i input[i] * A[r, i]
    float acc_out = 0.0f;
    for (uint r = 0; r < rank; ++r) {
        float inter = 0.0f;
        uint baseA = r * hidden;
        for (uint i = 0; i < hidden; ++i) {
            inter += input[i] * a[baseA + i];
        }
        // output[h] accumulates inter * B[h, r]
        uint baseB = h * rank;
        acc_out += inter * b[baseB + r];
    }
    float scaling = alpha / float(rank);
    output[h] = acc_out * scaling;
}
"#;

    let device = Device::system_default().expect("Metal device required for test");
    let options = CompileOptions::new();
    let library = device
        .new_library_with_source(msl, &options)
        .expect("Failed to compile MSL");
    let function = library.get_function("lora_flat", None).unwrap();
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .expect("Failed to create pipeline");
    let queue = device.new_command_queue();

    for &(hidden, rank) in shapes {
        for &alpha in alphas {
            // Input and weights
            let input: Vec<f32> = (0..hidden).map(|i| (i as f32 * 0.1) - 0.2).collect();
            let a: Vec<f32> = (0..(rank * hidden))
                .map(|i| ((i % hidden) as f32 + 1.0) * if i % 3 == 0 { -0.5 } else { 0.5 })
                .collect();
            let b: Vec<f32> = (0..(hidden * rank))
                .map(|i| ((i % rank) as f32 + 0.1) * if i % 2 == 0 { 0.2 } else { -0.3 })
                .collect();

            let cpu = cpu_lora_flat(&input, &a, &b, rank, hidden, alpha);

            // Buffers
            let input_buf = device.new_buffer_with_data(
                input.as_ptr() as *const _,
                (input.len() * std::mem::size_of::<f32>()) as u64,
                MTLResourceOptions::StorageModeShared,
            );
            let a_buf = device.new_buffer_with_data(
                a.as_ptr() as *const _,
                (a.len() * std::mem::size_of::<f32>()) as u64,
                MTLResourceOptions::StorageModeShared,
            );
            let b_buf = device.new_buffer_with_data(
                b.as_ptr() as *const _,
                (b.len() * std::mem::size_of::<f32>()) as u64,
                MTLResourceOptions::StorageModeShared,
            );
            let out_buf = device.new_buffer(
                (hidden * std::mem::size_of::<f32>()) as u64,
                MTLResourceOptions::StorageModeShared,
            );
            let rank_u = rank as u32;
            let hidden_u = hidden as u32;
            let rank_buf = device.new_buffer_with_data(
                &rank_u as *const u32 as *const _,
                std::mem::size_of::<u32>() as u64,
                MTLResourceOptions::StorageModeShared,
            );
            let hidden_buf = device.new_buffer_with_data(
                &hidden_u as *const u32 as *const _,
                std::mem::size_of::<u32>() as u64,
                MTLResourceOptions::StorageModeShared,
            );
            let alpha_buf = device.new_buffer_with_data(
                &alpha as *const f32 as *const _,
                std::mem::size_of::<f32>() as u64,
                MTLResourceOptions::StorageModeShared,
            );

            // Encode and dispatch
            let cmd = queue.new_command_buffer();
            let enc = cmd.new_compute_command_encoder();
            enc.set_compute_pipeline_state(&pipeline);
            enc.set_buffer(0, Some(&input_buf), 0);
            enc.set_buffer(1, Some(&a_buf), 0);
            enc.set_buffer(2, Some(&b_buf), 0);
            enc.set_buffer(3, Some(&out_buf), 0);
            enc.set_buffer(4, Some(&rank_buf), 0);
            enc.set_buffer(5, Some(&hidden_buf), 0);
            enc.set_buffer(6, Some(&alpha_buf), 0);
            let grid = MTLSize::new(hidden as u64, 1, 1);
            let tgs = MTLSize::new(1, 1, 1);
            enc.dispatch_thread_groups(grid, tgs);
            enc.end_encoding();
            cmd.commit();
            cmd.wait_until_completed();

            // Read back
            let gpu: Vec<f32> = unsafe {
                let ptr = out_buf.contents() as *const f32;
                std::slice::from_raw_parts(ptr, hidden).to_vec()
            };

            // Compare using blended absolute/relative tolerance to account for floating rounding
            let abs_eps = 1e-6f32;
            let rel_eps = 1e-6f32;
            let max_cpu = cpu.iter().fold(0.0f32, |acc, &v| acc.max(v.abs()));
            let mut max_err = 0.0f32;
            let mut l2 = 0.0f32;
            let mut mean = 0.0f32;
            for (c, g) in cpu.iter().zip(gpu.iter()) {
                let d = (c - g).abs();
                let tol = abs_eps + c.abs() * rel_eps;
                max_err = max_err.max(d);
                l2 += d * d;
                mean += d;
                assert!(
                    d <= tol,
                    "mismatch (hidden={}, rank={}): cpu={} gpu={} Δ={} tol={}",
                    hidden,
                    rank,
                    c,
                    g,
                    d,
                    tol
                );
            }
            mean /= cpu.len() as f32;
            l2 = l2.sqrt();
            let mean_tol = abs_eps + max_cpu * rel_eps;
            let max_tol = mean_tol;
            let l2_tol = (cpu.len() as f32).sqrt() * mean_tol;
            assert!(max_err <= max_tol);
            assert!(mean <= mean_tol, "mean error {} exceeds tolerance", mean);
            assert!(
                l2 <= l2_tol,
                "L2 error {} exceeds tolerance (max {}, mean {})",
                l2,
                max_err,
                mean
            );
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn metal_lora_repeatability() {
    use metal::{CompileOptions, Device, MTLResourceOptions, MTLSize};

    // Minimal LoRA flat kernel (same as in parity test)
    let msl = r#"#include <metal_stdlib>
using namespace metal;

kernel void lora_flat(
    device const float* input   [[ buffer(0) ]],
    device const float* a       [[ buffer(1) ]], // rank x hidden (row-major)
    device const float* b       [[ buffer(2) ]], // hidden x rank (row-major)
    device float*       output  [[ buffer(3) ]],
    constant uint&      rank    [[ buffer(4) ]],
    constant uint&      hidden  [[ buffer(5) ]],
    constant float&     alpha   [[ buffer(6) ]],
    uint3               tid     [[ thread_position_in_grid ]]
) {
    uint h = tid.x;
    if (h >= hidden) return;

    float acc_out = 0.0f;
    for (uint r = 0; r < rank; ++r) {
        float inter = 0.0f;
        uint baseA = r * hidden;
        for (uint i = 0; i < hidden; ++i) {
            inter += input[i] * a[baseA + i];
        }
        uint baseB = h * rank;
        acc_out += inter * b[baseB + r];
    }
    float scaling = alpha / float(rank);
    output[h] = acc_out * scaling;
}
"#;

    let device = Device::system_default().expect("Metal device required for test");
    let options = CompileOptions::new();
    let library = device
        .new_library_with_source(msl, &options)
        .expect("Failed to compile MSL");
    let function = library.get_function("lora_flat", None).unwrap();
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .expect("Failed to create pipeline");
    let queue = device.new_command_queue();

    // Fixed tiny dims and data
    let hidden = 8usize;
    let rank = 2usize;
    let alpha = 16.0f32;
    let input: Vec<f32> = (0..hidden).map(|i| (i as f32 * 0.05) - 0.1).collect();
    let a: Vec<f32> = (0..(rank * hidden))
        .map(|i| ((i % hidden) as f32 + 0.25) * if i % 3 == 0 { -0.5 } else { 0.5 })
        .collect();
    let b: Vec<f32> = (0..(hidden * rank))
        .map(|i| ((i % rank) as f32 + 0.2) * if i % 2 == 0 { 0.2 } else { -0.3 })
        .collect();

    // Common buffers
    let input_buf = device.new_buffer_with_data(
        input.as_ptr() as *const _,
        (input.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let a_buf = device.new_buffer_with_data(
        a.as_ptr() as *const _,
        (a.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let b_buf = device.new_buffer_with_data(
        b.as_ptr() as *const _,
        (b.len() * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let rank_u = rank as u32;
    let hidden_u = hidden as u32;
    let rank_buf = device.new_buffer_with_data(
        &rank_u as *const u32 as *const _,
        std::mem::size_of::<u32>() as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let hidden_buf = device.new_buffer_with_data(
        &hidden_u as *const u32 as *const _,
        std::mem::size_of::<u32>() as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let alpha_buf = device.new_buffer_with_data(
        &alpha as *const f32 as *const _,
        std::mem::size_of::<f32>() as u64,
        MTLResourceOptions::StorageModeShared,
    );

    // Run twice with identical setup
    let run_once = |out_buf: &metal::Buffer| {
        let cmd = queue.new_command_buffer();
        let enc = cmd.new_compute_command_encoder();
        enc.set_compute_pipeline_state(&pipeline);
        enc.set_buffer(0, Some(&input_buf), 0);
        enc.set_buffer(1, Some(&a_buf), 0);
        enc.set_buffer(2, Some(&b_buf), 0);
        enc.set_buffer(3, Some(out_buf), 0);
        enc.set_buffer(4, Some(&rank_buf), 0);
        enc.set_buffer(5, Some(&hidden_buf), 0);
        enc.set_buffer(6, Some(&alpha_buf), 0);
        let grid = MTLSize::new(hidden as u64, 1, 1);
        let tgs = MTLSize::new(1, 1, 1);
        enc.dispatch_thread_groups(grid, tgs);
        enc.end_encoding();
        cmd.commit();
        cmd.wait_until_completed();
    };

    let out1 = device.new_buffer(
        (hidden * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let out2 = device.new_buffer(
        (hidden * std::mem::size_of::<f32>()) as u64,
        MTLResourceOptions::StorageModeShared,
    );

    run_once(&out1);
    run_once(&out2);

    let gpu1: Vec<f32> = unsafe {
        let ptr = out1.contents() as *const f32;
        std::slice::from_raw_parts(ptr, hidden).to_vec()
    };
    let gpu2: Vec<f32> = unsafe {
        let ptr = out2.contents() as *const f32;
        std::slice::from_raw_parts(ptr, hidden).to_vec()
    };

    // Exact repeatability expected
    for (a, b) in gpu1.iter().zip(gpu2.iter()) {
        assert!(
            a.to_bits() == b.to_bits(),
            "Non-repeatable output: {} vs {}",
            a,
            b
        );
    }
}
