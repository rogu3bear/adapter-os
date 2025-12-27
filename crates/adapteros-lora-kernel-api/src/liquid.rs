//! Liquid blending kernel interface and reference implementation.
//!
//! This module defines the math-layer trait for blending multiple LoRA adapters
//! without copying weights. Implementations are expected to read directly from
//! mmap-backed weight buffers and fuse the blend with the following matmul
//! where possible.

use adapteros_core::{AosError, Result};
use std::time::{Duration, Instant};

/// Maximum number of adapters supported by liquid blending.
pub const LIQUID_MAX_ADAPTERS: usize = 3;

/// Precision for liquid blending operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiquidPrecision {
    /// 32-bit float weights
    F32,
    /// BFloat16 weights (high 16 bits of f32)
    BFloat16,
}

/// Backing slice for a tensor without copying the underlying data.
#[derive(Debug, Clone, Copy)]
pub enum LiquidSlice<'a> {
    /// f32 slice
    F32(&'a [f32]),
    /// bfloat16 slice encoded as u16
    BFloat16(&'a [u16]),
}

impl<'a> LiquidSlice<'a> {
    fn precision(&self) -> LiquidPrecision {
        match self {
            LiquidSlice::F32(_) => LiquidPrecision::F32,
            LiquidSlice::BFloat16(_) => LiquidPrecision::BFloat16,
        }
    }

    fn len(&self) -> usize {
        match self {
            LiquidSlice::F32(v) => v.len(),
            LiquidSlice::BFloat16(v) => v.len(),
        }
    }

    fn get_f32(&self, idx: usize) -> Result<f32> {
        match self {
            LiquidSlice::F32(v) => v.get(idx).copied().ok_or_else(|| {
                AosError::Kernel(format!("LiquidSlice index {} out of bounds", idx))
            }),
            LiquidSlice::BFloat16(v) => v
                .get(idx)
                .map(|bits| f32::from_bits((*bits as u32) << 16))
                .ok_or_else(|| {
                    AosError::Kernel(format!("LiquidSlice index {} out of bounds", idx))
                }),
        }
    }
}

/// Thin tensor view over LoRA matrices (row-major, contiguous).
#[derive(Debug, Clone, Copy)]
pub struct LiquidTensor<'a> {
    /// Backing data slice (no ownership)
    pub data: LiquidSlice<'a>,
    /// Number of rows
    pub rows: usize,
    /// Number of columns
    pub cols: usize,
}

impl<'a> LiquidTensor<'a> {
    /// Validate shape consistency.
    pub fn validate(&self, label: &str) -> Result<()> {
        if self.rows * self.cols != self.data.len() {
            return Err(AosError::Kernel(format!(
                "Liquid tensor {} has inconsistent shape: rows*cols={} != len={}",
                label,
                self.rows * self.cols,
                self.data.len()
            )));
        }
        Ok(())
    }

    /// Load a single element as f32 (handles bf16 conversion).
    #[inline]
    pub fn get(&self, row: usize, col: usize) -> Result<f32> {
        if row >= self.rows || col >= self.cols {
            return Err(AosError::Kernel(format!(
                "Liquid tensor index out of bounds: row={}, col={}, shape=({}, {})",
                row, col, self.rows, self.cols
            )));
        }
        let idx = row * self.cols + col;
        self.data.get_f32(idx)
    }

    /// Precision of the backing buffer.
    pub fn precision(&self) -> LiquidPrecision {
        self.data.precision()
    }
}

/// Adapter view used for blending.
#[derive(Debug, Clone, Copy)]
pub struct LiquidAdapterRef<'a> {
    /// Adapter identifier (router index)
    pub id: u16,
    /// LoRA "A" matrix (rank × in_features)
    pub down: LiquidTensor<'a>,
    /// LoRA "B" matrix (out_features × rank)
    pub up: LiquidTensor<'a>,
    /// Alpha scaling factor
    pub alpha: f32,
}

/// Request parameters for blending and forward computation.
pub struct LiquidBlendRequest<'a> {
    /// Active adapters to blend (max 3).
    pub adapters: &'a [LiquidAdapterRef<'a>],
    /// Mixing coefficients aligned with `adapters`.
    pub coefficients: &'a [f32],
    /// Input activation vector (length = in_features).
    pub input: &'a [f32],
    /// Output buffer (length = out_features).
    pub output: &'a mut [f32],
}

/// Blend execution statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiquidBlendStats {
    /// Number of adapters blended
    pub adapters: usize,
    /// Precision used during computation
    pub precision: LiquidPrecision,
    /// Wall-clock duration for the blend (reference only)
    pub elapsed: Duration,
}

/// Trait for backends that support liquid blending of LoRA weights.
///
/// Implementations are expected to read weights directly from mmap-backed
/// buffers (no copies) and fuse the weighted sum with the following matmul
/// whenever possible.
pub trait LiquidKernel: Send + Sync {
    /// Whether this backend supports the liquid blending fast-path.
    fn supports_liquid_blending(&self) -> bool {
        true
    }

    /// Maximum adapters supported by this backend (default: 3).
    fn max_liquid_adapters(&self) -> usize {
        LIQUID_MAX_ADAPTERS
    }

    /// Blend adapters and apply to the input activation vector.
    fn blend_and_forward(&mut self, request: LiquidBlendRequest<'_>) -> Result<LiquidBlendStats>;
}

/// Slow CPU reference implementation for verification and non-Metal platforms.
pub fn blend_and_forward_reference(request: LiquidBlendRequest<'_>) -> Result<LiquidBlendStats> {
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

    // Validate shapes and accumulate precision usage.
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

        // Track lowest precision present
        if adapter.down.precision() == LiquidPrecision::BFloat16
            || adapter.up.precision() == LiquidPrecision::BFloat16
        {
            precision = LiquidPrecision::BFloat16;
        }
    }

    let start = Instant::now();
    request.output.fill(0.0);

    for (adapter, &coeff) in request.adapters.iter().zip(request.coefficients.iter()) {
        if coeff == 0.0 {
            continue;
        }

        let rank = adapter.down.rows;
        let scaling = adapter.alpha / rank as f32;

        // Compute A @ x (rank vector)
        let mut rank_buf = vec![0.0f32; rank];
        for r in 0..rank {
            let mut acc = 0.0f32;
            for c in 0..adapter.down.cols {
                acc += adapter.down.get(r, c)? * request.input[c];
            }
            rank_buf[r] = acc;
        }

        // Compute B @ (A @ x) and accumulate
        for out_idx in 0..adapter.up.rows {
            let mut acc = 0.0f32;
            for r in 0..adapter.up.cols {
                acc += adapter.up.get(out_idx, r)? * rank_buf[r];
            }
            request.output[out_idx] += coeff * acc * scaling;
        }
    }

    Ok(LiquidBlendStats {
        adapters: request.adapters.len(),
        precision,
        elapsed: start.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bf16_from_f32(val: f32) -> u16 {
        (val.to_bits() >> 16) as u16
    }

    #[test]
    fn blend_two_adapters_half_weight() {
        // Adapter 1 (f32)
        let a1 = [1.0f32, 0.0, 0.0, 1.0]; // 2x2 identity
        let b1 = [1.0f32, 2.0, 3.0, 4.0]; // 2x2
        let adapter1 = LiquidAdapterRef {
            id: 0,
            down: LiquidTensor {
                data: LiquidSlice::F32(&a1),
                rows: 2,
                cols: 2,
            },
            up: LiquidTensor {
                data: LiquidSlice::F32(&b1),
                rows: 2,
                cols: 2,
            },
            alpha: 2.0,
        };

        // Adapter 2 (bf16) to exercise mixed precision path
        let a2 = [
            bf16_from_f32(1.0),
            bf16_from_f32(1.0),
            bf16_from_f32(1.0),
            bf16_from_f32(1.0),
        ];
        let b2 = [
            bf16_from_f32(0.5),
            bf16_from_f32(0.5),
            bf16_from_f32(0.5),
            bf16_from_f32(0.5),
        ];
        let adapter2 = LiquidAdapterRef {
            id: 1,
            down: LiquidTensor {
                data: LiquidSlice::BFloat16(&a2),
                rows: 2,
                cols: 2,
            },
            up: LiquidTensor {
                data: LiquidSlice::BFloat16(&b2),
                rows: 2,
                cols: 2,
            },
            alpha: 1.0,
        };

        let mut output = [0.0f32; 2];
        let stats = blend_and_forward_reference(LiquidBlendRequest {
            adapters: &[adapter1, adapter2],
            coefficients: &[0.5, 0.5],
            input: &[1.0, 1.0],
            output: &mut output,
        })
        .expect("blend should succeed");

        assert_eq!(stats.adapters, 2);
        assert_eq!(stats.precision, LiquidPrecision::BFloat16);
        assert!(
            (output[0] - 2.0).abs() < 1e-5,
            "expected 2.0, got {}",
            output[0]
        );
        assert!(
            (output[1] - 4.0).abs() < 1e-5,
            "expected 4.0, got {}",
            output[1]
        );
    }
}
