//! Liquid blending for MLX backends.
//!
//! This module provides liquid blending functionality using the
//! reference CPU implementation from kernel-api.

use adapteros_core::Result;
use adapteros_lora_kernel_api::liquid::{
    blend_and_forward_reference, LiquidBlendRequest, LiquidBlendStats,
};

/// Blend adapters using the reference CPU implementation.
///
/// This function delegates to the kernel-api reference implementation.
/// Future versions may add GPU-accelerated paths.
pub fn blend_and_forward_mlx(request: LiquidBlendRequest<'_>) -> Result<LiquidBlendStats> {
    blend_and_forward_reference(request)
}
