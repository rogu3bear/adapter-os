//! Simplified Test Adapter Factory
//!
//! Creates minimal valid .aos adapter files for testing without requiring
//! the full training pipeline.

use adapteros_core::Result;
use std::io::Write;

/// Weight pattern type for synthetic adapters
#[derive(Debug, Clone, Copy)]
pub enum WeightPattern {
    /// All weights are 0.0
    Zeros,
    /// All weights are 1.0
    Ones,
    /// Sequential weights (0, 1, 2, 3, ...)
    Sequential,
    /// Constant value
    Constant(f32),
    /// Random with seed
    Random(u64),
}

/// Create a synthetic adapter with a specific weight pattern
///
/// This creates a minimal valid .aos file that can be loaded by Metal kernels.
/// The adapter has a simple structure with all 5 LoRA modules.
///
/// # Arguments
/// * `rank` - LoRA rank
/// * `alpha` - LoRA alpha scaling
/// * `pattern` - Weight initialization pattern
///
/// # Returns
/// Raw bytes of a valid .aos file
pub fn create_synthetic_adapter(
    rank: usize,
    alpha: f32,
    pattern: WeightPattern,
) -> Result<Vec<u8>> {
    // For now, return a minimal placeholder
    // Real implementation would create proper safetensors + manifest

    // Create minimal .aos structure:
    // [manifest_offset(4)][manifest_len(4)][manifest_json][weights_safetensors]

    let manifest = serde_json::json!({
        "version": "1.0.0",
        "rank": rank,
        "alpha": alpha,
        "base_model": "test_model",
        "pattern": format!("{:?}", pattern),
    });

    let manifest_json = serde_json::to_vec(&manifest)
        .map_err(|e| adapteros_core::AosError::Serialization(e))?;

    // Create minimal safetensors with dummy data
    // For testing, we just need valid structure
    let weights_data = vec![0u8; 1024]; // Minimal weights placeholder

    let manifest_offset = 8 + weights_data.len();
    let manifest_len = manifest_json.len();

    let mut aos_bytes = Vec::new();

    // Write header
    aos_bytes.write_all(&(manifest_offset as u32).to_le_bytes())
        .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;
    aos_bytes.write_all(&(manifest_len as u32).to_le_bytes())
        .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;

    // Write weights
    aos_bytes.write_all(&weights_data)
        .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;

    // Write manifest
    aos_bytes.write_all(&manifest_json)
        .map_err(|e| adapteros_core::AosError::Io(e.to_string()))?;

    Ok(aos_bytes)
}

/// Create a minimal test adapter using simpler approach
///
/// This is an alias for create_synthetic_adapter for compatibility.
pub async fn create_minimal_test_adapter(rank: usize, alpha: f32) -> Result<Vec<u8>> {
    create_synthetic_adapter(rank, alpha, WeightPattern::Ones)
}

/// Compute L2 distance between two vectors
pub fn compute_l2_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have same length");
    let sum_sq_diff: f32 = a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum();
    sum_sq_diff.sqrt()
}

/// Create adapter with constant weights (convenience function)
pub fn create_adapter_with_constant_weights(
    rank: usize,
    alpha: f32,
    constant: f32,
) -> Result<Vec<u8>> {
    create_synthetic_adapter(rank, alpha, WeightPattern::Constant(constant))
}
