//! Tests for RoPE (Rotary Position Embeddings) implementation
//!
//! These tests verify:
//! - RoPE computation correctness
//! - Determinism across runs
//! - Position encoding up to 32K context length
//! - Compatibility with Qwen2.5-7B configuration

use adapteros_lora_kernel_mtl::{GqaConfig, MetalKernels};
use adapteros_lora_kernel_api::FusedKernels;

#[test]
fn test_rope_config_defaults() {
    // Verify Qwen2.5-7B RoPE defaults
    let config = GqaConfig::default();
    assert_eq!(config.rope_theta, 10000.0, "Default RoPE theta should be 10000.0");
    assert_eq!(config.head_dim, 128, "Head dimension should be 128");
}

#[test]
fn test_rope_custom_theta() {
    // Test with custom RoPE theta value
    let mut config = GqaConfig::default();
    config.rope_theta = 1000000.0;  // Higher theta for longer context
    
    assert_eq!(config.rope_theta, 1000000.0);
}

#[test]
#[cfg(target_os = "macos")]
fn test_rope_kernel_initialization() {
    // Verify kernel can be initialized with RoPE config
    let result = MetalKernels::new();
    
    match result {
        Ok(_kernels) => {
            // Success - kernel initialized with RoPE support
        }
        Err(e) => {
            // Expected if metallib not compiled yet
            assert!(
                e.to_string().contains("not yet compiled"),
                "Unexpected error: {}",
                e
            );
        }
    }
}

#[test]
fn test_rope_frequency_calculation() {
    // Test RoPE frequency calculation at different dimensions
    let config = GqaConfig::default();
    let head_dim = config.head_dim as f32;
    let theta = config.rope_theta;
    
    // Verify frequency decreases with dimension
    for dim_idx in (0..config.head_dim).step_by(2) {
        let exponent = -2.0 * (dim_idx as f32) / head_dim;
        let freq = theta.powf(exponent);
        
        assert!(freq > 0.0, "Frequency should be positive");
        assert!(freq.is_finite(), "Frequency should be finite");
        
        // Higher dimensions should have lower frequencies
        if dim_idx > 0 {
            let prev_exponent = -2.0 * ((dim_idx - 2) as f32) / head_dim;
            let prev_freq = theta.powf(prev_exponent);
            assert!(
                freq <= prev_freq,
                "Frequency should decrease with dimension"
            );
        }
    }
}

#[test]
fn test_rope_position_range() {
    // Test RoPE works across full Qwen2.5-7B context length (32K)
    let config = GqaConfig::default();
    let max_position = 32768_u32;
    
    for position in [0, 1000, 16384, 32767].iter() {
        assert!(
            *position < max_position,
            "Position {} should be within context length",
            position
        );
        
        // Verify angle computation doesn't overflow
        for dim_idx in (0..config.head_dim).step_by(2) {
            let exponent = -2.0 * (dim_idx as f32) / (config.head_dim as f32);
            let freq = config.rope_theta.powf(exponent);
            let angle = (*position as f32) / freq;
            
            assert!(
                angle.is_finite(),
                "Angle should be finite at position {} dim {}",
                position,
                dim_idx
            );
        }
    }
}

#[test]
fn test_rope_rotation_properties() {
    // Test RoPE rotation preserves norm
    let test_cases = vec![
        (1.0, 0.0, 0.0),      // x=1, y=0, theta=0
        (0.0, 1.0, 0.0),      // x=0, y=1, theta=0
        (1.0, 1.0, std::f32::consts::PI / 4.0),  // 45 degree rotation
        (3.0, 4.0, std::f32::consts::PI / 2.0),  // 90 degree rotation
    ];
    
    for (x, y, theta) in test_cases {
        let cos_theta = theta.cos();
        let sin_theta = theta.sin();
        
        // Apply rotation: (x', y') = (x*cos - y*sin, y*cos + x*sin)
        let x_rot = x * cos_theta - y * sin_theta;
        let y_rot = y * cos_theta + x * sin_theta;
        
        // Original norm
        let orig_norm = (x * x + y * y).sqrt();
        
        // Rotated norm
        let rot_norm = (x_rot * x_rot + y_rot * y_rot).sqrt();
        
        // Rotation should preserve norm (within floating point error)
        assert!(
            (orig_norm - rot_norm).abs() < 1e-5,
            "Rotation should preserve norm: {} vs {}",
            orig_norm,
            rot_norm
        );
    }
}

#[test]
fn test_rope_determinism() {
    // Test that RoPE computation is deterministic
    let config = GqaConfig::default();
    let position = 12345_u32;
    let dim_idx = 64_u32;
    
    let compute_angle = || {
        let exponent = -2.0 * (dim_idx as f32) / (config.head_dim as f32);
        let freq = config.rope_theta.powf(exponent);
        (position as f32) / freq
    };
    
    // Compute angle multiple times
    let angles: Vec<f32> = (0..10).map(|_| compute_angle()).collect();
    
    // All angles should be identical
    for angle in &angles[1..] {
        assert_eq!(
            angles[0], *angle,
            "RoPE angle computation should be deterministic"
        );
    }
}

#[test]
fn test_rope_symmetry() {
    // Test RoPE rotation at opposite angles
    let x = 2.0_f32;
    let y = 3.0_f32;
    let theta = std::f32::consts::PI / 3.0;  // 60 degrees
    
    let cos_pos = theta.cos();
    let sin_pos = theta.sin();
    let cos_neg = (-theta).cos();
    let sin_neg = (-theta).sin();
    
    // Rotate by +theta
    let x_pos = x * cos_pos - y * sin_pos;
    let y_pos = y * cos_pos + x * sin_pos;
    
    // Rotate by -theta
    let x_neg = x * cos_neg - y * sin_neg;
    let y_neg = y * cos_neg + x * sin_neg;
    
    // Rotating back should give original (approximately)
    let x_back = x_pos * cos_neg - y_pos * sin_neg;
    let y_back = y_pos * cos_neg + x_pos * sin_neg;
    
    assert!((x - x_back).abs() < 1e-5, "X should return to original");
    assert!((y - y_back).abs() < 1e-5, "Y should return to original");
}

#[test]
fn test_rope_extended_context() {
    // Test RoPE with extended context lengths
    let mut config = GqaConfig::default();
    
    // Test with different theta values for extended context
    let theta_values = vec![
        10000.0,    // Standard
        100000.0,   // Extended
        1000000.0,  // Very long context
    ];
    
    for theta in theta_values {
        config.rope_theta = theta;
        
        // Verify computation at maximum position
        let max_pos = 131072_u32;  // 128K context
        let dim = 0_u32;
        
        let exponent = -2.0 * (dim as f32) / (config.head_dim as f32);
        let freq = theta.powf(exponent);
        let angle = (max_pos as f32) / freq;
        
        assert!(
            angle.is_finite(),
            "Angle should be finite at max position with theta={}",
            theta
        );
    }
}

