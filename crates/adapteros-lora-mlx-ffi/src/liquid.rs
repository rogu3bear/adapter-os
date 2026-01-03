//! Liquid blending for MLX backends.
//!
//! This module provides a thin MLX graph that blends up to three LoRA adapters
//! without materializing an intermediate weight matrix. It falls back to the
//! reference CPU path when mlx-rs is not available.

#[cfg(feature = "mlx-rs-backend")]
use std::time::Instant;

#[cfg(feature = "mlx-rs-backend")]
use adapteros_core::AosError;
use adapteros_core::Result;
use adapteros_lora_kernel_api::liquid::{
    blend_and_forward_reference, LiquidBlendRequest, LiquidBlendStats,
};
#[cfg(feature = "mlx-rs-backend")]
use adapteros_lora_kernel_api::liquid::{
    LiquidPrecision, LiquidSlice, LiquidTensor, LIQUID_MAX_ADAPTERS,
};

#[cfg(feature = "mlx-rs-backend")]
use crate::array::MlxArray;

#[cfg(feature = "mlx-rs-backend")]
fn validate_request(request: &LiquidBlendRequest<'_>) -> Result<LiquidPrecision> {
    if request.adapters.is_empty() {
        return Err(AosError::Kernel(
            "Liquid blend requested with zero adapters".to_string(),
        ));
    }
    if request.adapters.len() > LIQUID_MAX_ADAPTERS {
        return Err(AosError::Kernel(format!(
            "Liquid blend exceeds max adapters ({} > {})",
            request.adapters.len(),
            LIQUID_MAX_ADAPTERS
        )));
    }
    if request.adapters.len() != request.coefficients.len() {
        return Err(AosError::Kernel(format!(
            "Liquid blend coefficient mismatch: {} adapters vs {} coefficients",
            request.adapters.len(),
            request.coefficients.len()
        )));
    }

    let mut precision = LiquidPrecision::F32;
    for (idx, adapter) in request.adapters.iter().enumerate() {
        adapter.down.validate("down")?;
        adapter.up.validate("up")?;

        if adapter.down.cols != request.input.len() {
            return Err(AosError::Kernel(format!(
                "Liquid adapter {} input mismatch: down.cols={} vs input len={}",
                idx,
                adapter.down.cols,
                request.input.len()
            )));
        }
        if adapter.up.rows > request.output.len() {
            return Err(AosError::Kernel(format!(
                "Liquid adapter {} output mismatch: up.rows={} vs output len={}",
                idx,
                adapter.up.rows,
                request.output.len()
            )));
        }
        if adapter.up.cols != adapter.down.rows {
            return Err(AosError::Kernel(format!(
                "Liquid adapter {} rank mismatch: up.cols={} vs down.rows={}",
                idx, adapter.up.cols, adapter.down.rows
            )));
        }

        if adapter.down.precision() == LiquidPrecision::BFloat16
            || adapter.up.precision() == LiquidPrecision::BFloat16
        {
            precision = LiquidPrecision::BFloat16;
        }
    }

    Ok(precision)
}

#[cfg(feature = "mlx-rs-backend")]
fn tensor_to_mlx(tensor: &LiquidTensor<'_>) -> Result<MlxArray> {
    match tensor.data {
        LiquidSlice::F32(data) => Ok(MlxArray::from_slice_f32(
            data,
            &[tensor.rows as i32, tensor.cols as i32],
        )?),
        LiquidSlice::BFloat16(data) => {
            let mut tmp = Vec::with_capacity(data.len());
            for &bits in data {
                tmp.push(f32::from_bits((bits as u32) << 16));
            }
            Ok(MlxArray::from_slice_f32(
                &tmp,
                &[tensor.rows as i32, tensor.cols as i32],
            )?)
        }
    }
}

/// Blend adapters using MLX arrays (with CPU fallback).
pub fn blend_and_forward_mlx(mut request: LiquidBlendRequest<'_>) -> Result<LiquidBlendStats> {
    // Early exit to CPU reference when mlx-rs is not compiled in.
    #[cfg(not(feature = "mlx-rs-backend"))]
    {
        return blend_and_forward_reference(request);
    }

    // mlx-rs path
    #[cfg(feature = "mlx-rs-backend")]
    {
        let mut precision = validate_request(&request)?;
        request.output.fill(0.0);

        let start = Instant::now();
        let input = MlxArray::from_slice_f32(request.input, &[request.input.len() as i32, 1])?;

        for (adapter, &coeff) in request.adapters.iter().zip(request.coefficients.iter()) {
            if coeff == 0.0 {
                continue;
            }

            if adapter.down.precision() == LiquidPrecision::BFloat16
                || adapter.up.precision() == LiquidPrecision::BFloat16
            {
                precision = LiquidPrecision::BFloat16;
            }

            let rank = adapter.down.rows;
            let down = tensor_to_mlx(&adapter.down)?;
            let up = tensor_to_mlx(&adapter.up)?;

            let rank_vec = down.matmul(&input)?;
            let scaled = rank_vec.scale(adapter.alpha / rank as f32)?;
            let out = up.matmul(&scaled)?;
            let weighted = out.scale(coeff)?;
            let host = weighted.to_vec_f32()?;

            for (idx, val) in host.iter().enumerate().take(request.output.len()) {
                request.output[idx] += *val;
            }
        }

        return Ok(LiquidBlendStats {
            adapters: request.adapters.len(),
            precision,
            elapsed: start.elapsed(),
        });
    }
}
