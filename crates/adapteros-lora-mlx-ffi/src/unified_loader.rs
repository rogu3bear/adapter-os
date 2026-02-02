//! Unified SafeTensors loader with MLX native path and Rust fallback
//!
//! Provides zero-copy GPU loading when MLX is available, falling back
//! to the Rust safetensors crate for compatibility.

use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use std::path::Path;

/// Loading strategy preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadStrategy {
    /// Try MLX native first, fallback to Rust crate
    #[default]
    MlxPreferred,
    /// Always use Rust crate (or mlx-rs when that feature is enabled)
    RustOnly,
}

/// Tensor metadata
#[derive(Debug, Clone)]
pub struct TensorMetadata {
    pub name: String,
    pub shape: Vec<usize>,
    pub dtype: String,
    pub size_bytes: usize,
}

/// Unified SafeTensors loader
pub struct UnifiedSafeTensorsLoader {
    /// MLX weights handle (if loaded via MLX FFI)
    mlx_weights: Option<*mut crate::mlx_weights_t>,
    /// Rust safetensors data (if loaded via Rust)
    rust_data: Option<Vec<u8>>,
    /// Tensor metadata
    tensor_info: HashMap<String, TensorMetadata>,
    /// Strategy used
    strategy_used: LoadStrategy,
}

impl UnifiedSafeTensorsLoader {
    /// Load safetensors file with specified strategy
    pub fn load<P: AsRef<Path>>(path: P, strategy: LoadStrategy) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(AosError::NotFound(format!(
                "SafeTensors file not found: {}",
                path.display()
            )));
        }

        match strategy {
            LoadStrategy::MlxPreferred => {
                // Try MLX native first
                match Self::load_mlx_native(path) {
                    Ok(loader) => {
                        tracing::info!(path = %path.display(), "Loaded via MLX native");
                        return Ok(loader);
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "MLX native load failed, using Rust fallback"
                        );
                    }
                }
                // Fallback to Rust
                Self::load_rust_crate(path)
            }
            LoadStrategy::RustOnly => Self::load_rust_crate(path),
        }
    }

    /// Load using MLX C++ FFI
    fn load_mlx_native(path: &Path) -> Result<Self> {
        // Ensure MLX is initialized
        crate::mlx_ensure_initialized(true)?;

        let path_str = path.to_string_lossy();
        let path_cstr = std::ffi::CString::new(path_str.as_bytes())
            .map_err(|e| AosError::Internal(format!("Invalid path: {}", e)))?;

        unsafe {
            crate::mlx_clear_error();
            let weights = crate::mlx_load_safetensors(path_cstr.as_ptr());

            if weights.is_null() {
                let err = crate::mlx_get_last_error();
                let err_str = if err.is_null() {
                    "Unknown error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(err).to_string_lossy().into_owned()
                };
                crate::mlx_clear_error();
                return Err(AosError::Mlx(format!("MLX load failed: {}", err_str)));
            }

            // Get tensor names
            let mut tensor_info = HashMap::new();
            let mut names_ptrs: Vec<*const std::os::raw::c_char> = vec![std::ptr::null(); 512];
            let num = crate::mlx_weights_list(weights, names_ptrs.as_mut_ptr(), 512);

            for name_ptr in names_ptrs.iter().take((num as usize).min(512)) {
                if !name_ptr.is_null() {
                    let name = std::ffi::CStr::from_ptr(*name_ptr)
                        .to_string_lossy()
                        .into_owned();

                    let name_cstr = match std::ffi::CString::new(name.as_bytes()) {
                        Ok(cstr) => Some(cstr),
                        Err(_) => {
                            tracing::trace!(tensor_name = %name, "Skipping tensor with invalid name (contains NUL byte)");
                            None
                        }
                    };
                    if let Some(cstr) = name_cstr {
                        let tensor = crate::mlx_weights_get(weights, cstr.as_ptr());
                        if !tensor.is_null() {
                            let size = crate::mlx_array_size(tensor) as usize;
                            let ndim = crate::mlx_array_ndim(tensor) as usize;
                            let mut shape_buf = vec![0i32; 8];
                            crate::mlx_array_shape(tensor, shape_buf.as_mut_ptr(), 8);
                            let shape: Vec<usize> =
                                shape_buf[..ndim].iter().map(|&s| s as usize).collect();

                            tensor_info.insert(
                                name.clone(),
                                TensorMetadata {
                                    name,
                                    shape,
                                    dtype: "float32".to_string(),
                                    size_bytes: size * 4,
                                },
                            );
                        }
                    }
                }
            }

            Ok(Self {
                mlx_weights: Some(weights),
                rust_data: None,
                tensor_info,
                strategy_used: LoadStrategy::MlxPreferred,
            })
        }
    }

    /// Load using Rust safetensors crate
    fn load_rust_crate(path: &Path) -> Result<Self> {
        let data =
            std::fs::read(path).map_err(|e| AosError::Io(format!("Failed to read file: {}", e)))?;

        let tensors = safetensors::SafeTensors::deserialize(&data)
            .map_err(|e| AosError::Parse(format!("Failed to parse: {}", e)))?;

        let mut tensor_info = HashMap::new();
        for (name, _) in tensors.tensors() {
            if let Ok(view) = tensors.tensor(&name) {
                let shape: Vec<usize> = view.shape().to_vec();
                let dtype = format!("{:?}", view.dtype());
                let elem_size = match view.dtype() {
                    safetensors::Dtype::F32 | safetensors::Dtype::I32 | safetensors::Dtype::U32 => {
                        4
                    }
                    safetensors::Dtype::F16
                    | safetensors::Dtype::BF16
                    | safetensors::Dtype::I16
                    | safetensors::Dtype::U16 => 2,
                    safetensors::Dtype::I8 | safetensors::Dtype::U8 | safetensors::Dtype::BOOL => 1,
                    safetensors::Dtype::F64 | safetensors::Dtype::I64 | safetensors::Dtype::U64 => {
                        8
                    }
                    _ => 4, // Default to 4 bytes
                };
                let size: usize = shape.iter().product();
                tensor_info.insert(
                    name.to_string(),
                    TensorMetadata {
                        name: name.to_string(),
                        shape,
                        dtype,
                        size_bytes: size * elem_size,
                    },
                );
            }
        }

        tracing::info!(path = %path.display(), tensors = tensor_info.len(), "Loaded via Rust crate");

        Ok(Self {
            mlx_weights: None,
            rust_data: Some(data),
            tensor_info,
            strategy_used: LoadStrategy::RustOnly,
        })
    }

    /// Get tensor as f32 vec
    pub fn get_tensor_f32(&self, name: &str) -> Result<Vec<f32>> {
        if let Some(weights) = self.mlx_weights {
            // MLX path
            let name_cstr = std::ffi::CString::new(name)
                .map_err(|e| AosError::Internal(format!("Invalid name: {}", e)))?;

            unsafe {
                let array = crate::mlx_weights_get(weights, name_cstr.as_ptr());
                if array.is_null() {
                    return Err(AosError::NotFound(format!("Tensor not found: {}", name)));
                }

                crate::mlx_eval(array);
                crate::mlx_synchronize();

                let size = crate::mlx_array_size(array) as usize;
                let ptr = crate::mlx_array_data(array);
                if ptr.is_null() {
                    return Err(AosError::Mlx("Failed to get data".to_string()));
                }

                Ok(std::slice::from_raw_parts(ptr, size).to_vec())
            }
        } else if let Some(ref data) = self.rust_data {
            // Rust path
            let tensors = safetensors::SafeTensors::deserialize(data)
                .map_err(|e| AosError::Parse(format!("Parse error: {}", e)))?;

            let view = tensors
                .tensor(name)
                .map_err(|_| AosError::NotFound(format!("Tensor not found: {}", name)))?;

            Self::convert_tensor_to_f32(&view)
        } else {
            Err(AosError::Internal("No data loaded".to_string()))
        }
    }

    /// Convert safetensors view to f32 vec (handles multiple dtypes)
    fn convert_tensor_to_f32(view: &safetensors::tensor::TensorView<'_>) -> Result<Vec<f32>> {
        let bytes = view.data();
        match view.dtype() {
            safetensors::Dtype::F32 => Ok(bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect()),
            safetensors::Dtype::F16 => {
                // Convert f16 to f32
                Ok(bytes
                    .chunks_exact(2)
                    .map(|chunk| {
                        let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                        half_to_f32(bits)
                    })
                    .collect())
            }
            safetensors::Dtype::BF16 => {
                // Convert bf16 to f32
                Ok(bytes
                    .chunks_exact(2)
                    .map(|chunk| {
                        let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                        bf16_to_f32(bits)
                    })
                    .collect())
            }
            safetensors::Dtype::I32 => Ok(bytes
                .chunks_exact(4)
                .map(|chunk| i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as f32)
                .collect()),
            safetensors::Dtype::I8 => Ok(bytes.iter().map(|&b| b as i8 as f32).collect()),
            other => Err(AosError::Mlx(format!("Unsupported dtype: {:?}", other))),
        }
    }

    /// List tensor names
    pub fn tensor_names(&self) -> Vec<String> {
        self.tensor_info.keys().cloned().collect()
    }

    /// Get tensor metadata
    pub fn get_metadata(&self, name: &str) -> Option<&TensorMetadata> {
        self.tensor_info.get(name)
    }

    /// Get strategy used
    pub fn strategy_used(&self) -> LoadStrategy {
        self.strategy_used
    }
}

impl Drop for UnifiedSafeTensorsLoader {
    fn drop(&mut self) {
        if let Some(weights) = self.mlx_weights.take() {
            unsafe {
                crate::mlx_weights_free(weights);
            }
        }
    }
}

// f16/bf16 conversion helpers

/// Convert IEEE 754 half-precision (f16) to f32
fn half_to_f32(bits: u16) -> f32 {
    let sign = ((bits >> 15) & 1) as u32;
    let exp = ((bits >> 10) & 0x1F) as u32;
    let mantissa = (bits & 0x3FF) as u32;

    if exp == 0 {
        // Denormalized number or zero
        if mantissa == 0 {
            f32::from_bits(sign << 31)
        } else {
            // Denormalized
            let mut m = mantissa;
            let mut e = 0i32;
            while (m & 0x400) == 0 {
                m <<= 1;
                e -= 1;
            }
            m &= 0x3FF;
            let new_exp = (127 - 15 + 1 + e) as u32;
            f32::from_bits((sign << 31) | (new_exp << 23) | (m << 13))
        }
    } else if exp == 31 {
        // Infinity or NaN
        if mantissa == 0 {
            f32::from_bits((sign << 31) | (0xFF << 23))
        } else {
            f32::from_bits((sign << 31) | (0xFF << 23) | (mantissa << 13))
        }
    } else {
        // Normalized number
        let new_exp = (exp as i32 - 15 + 127) as u32;
        f32::from_bits((sign << 31) | (new_exp << 23) | (mantissa << 13))
    }
}

/// Convert bfloat16 to f32 (simple padding - bf16 is just truncated f32)
fn bf16_to_f32(bits: u16) -> f32 {
    f32::from_bits((bits as u32) << 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============ half_to_f32 tests ============

    #[test]
    fn test_half_to_f32_positive_zero() {
        // f16 positive zero: sign=0, exp=0, mantissa=0
        let bits: u16 = 0x0000;
        let result = half_to_f32(bits);
        assert_eq!(result, 0.0_f32);
        assert!(!result.is_sign_negative());
    }

    #[test]
    fn test_half_to_f32_negative_zero() {
        // f16 negative zero: sign=1, exp=0, mantissa=0
        let bits: u16 = 0x8000;
        let result = half_to_f32(bits);
        assert_eq!(result, -0.0_f32);
        assert!(result.is_sign_negative());
    }

    #[test]
    fn test_half_to_f32_positive_one() {
        // f16 1.0: sign=0, exp=15 (biased), mantissa=0
        // exp=15 means unbiased exponent=0, so 1.0 * 2^0 = 1.0
        let bits: u16 = 0x3C00;
        let result = half_to_f32(bits);
        assert_eq!(result, 1.0_f32);
    }

    #[test]
    fn test_half_to_f32_negative_one() {
        // f16 -1.0: sign=1, exp=15 (biased), mantissa=0
        let bits: u16 = 0xBC00;
        let result = half_to_f32(bits);
        assert_eq!(result, -1.0_f32);
    }

    #[test]
    fn test_half_to_f32_two() {
        // f16 2.0: sign=0, exp=16 (biased), mantissa=0
        // exp=16 means unbiased exponent=1, so 1.0 * 2^1 = 2.0
        let bits: u16 = 0x4000;
        let result = half_to_f32(bits);
        assert_eq!(result, 2.0_f32);
    }

    #[test]
    fn test_half_to_f32_half() {
        // f16 0.5: sign=0, exp=14 (biased), mantissa=0
        // exp=14 means unbiased exponent=-1, so 1.0 * 2^-1 = 0.5
        let bits: u16 = 0x3800;
        let result = half_to_f32(bits);
        assert_eq!(result, 0.5_f32);
    }

    #[test]
    fn test_half_to_f32_one_point_five() {
        // f16 1.5: sign=0, exp=15, mantissa=0x200 (0.5 in binary)
        // 1.5 = 1.1 in binary = (1 + 0.5) * 2^0
        let bits: u16 = 0x3E00;
        let result = half_to_f32(bits);
        assert_eq!(result, 1.5_f32);
    }

    #[test]
    fn test_half_to_f32_various_exponents() {
        // Test various normalized numbers with different exponents
        // f16 max normalized: exp=30, mantissa=0x3FF
        let max_bits: u16 = 0x7BFF;
        let max_result = half_to_f32(max_bits);
        assert!((max_result - 65504.0_f32).abs() < 1.0); // f16 max is ~65504

        // f16 smallest positive normalized: exp=1, mantissa=0
        // 1.0 * 2^(1-15) = 2^-14 ≈ 6.1e-5
        let min_norm_bits: u16 = 0x0400;
        let min_norm_result = half_to_f32(min_norm_bits);
        let expected_min = 2.0_f32.powi(-14);
        assert!(
            (min_norm_result - expected_min).abs() < 1e-10,
            "min_norm: got {}, expected {}",
            min_norm_result,
            expected_min
        );
    }

    #[test]
    fn test_half_to_f32_denormalized_numbers() {
        // Denormalized: exp=0, mantissa!=0
        // Value = 0.mantissa * 2^-14

        // Smallest denorm: mantissa=1
        // Value = 2^-10 * 2^-14 = 2^-24 ≈ 5.96e-8
        let smallest_denorm: u16 = 0x0001;
        let result = half_to_f32(smallest_denorm);
        let expected = 2.0_f32.powi(-24);
        assert!(
            (result - expected).abs() < 1e-12,
            "smallest denorm: got {}, expected {}",
            result,
            expected
        );

        // Larger denorm: mantissa=0x200 (0.5 in 10-bit fraction)
        // Value = 0.5 * 2^-14 = 2^-15 ≈ 3.05e-5
        let half_denorm: u16 = 0x0200;
        let half_result = half_to_f32(half_denorm);
        let half_expected = 2.0_f32.powi(-15);
        assert!(
            (half_result - half_expected).abs() < 1e-10,
            "half denorm: got {}, expected {}",
            half_result,
            half_expected
        );

        // Max denorm: mantissa=0x3FF
        // Value = (1 - 2^-10) * 2^-14
        let max_denorm: u16 = 0x03FF;
        let max_result = half_to_f32(max_denorm);
        let max_expected = (1.0_f32 - 2.0_f32.powi(-10)) * 2.0_f32.powi(-14);
        assert!(
            (max_result - max_expected).abs() < 1e-10,
            "max denorm: got {}, expected {}",
            max_result,
            max_expected
        );

        // Negative denorm
        let neg_denorm: u16 = 0x8001;
        let neg_result = half_to_f32(neg_denorm);
        assert!(neg_result.is_sign_negative());
        assert!((neg_result.abs() - expected).abs() < 1e-12);
    }

    #[test]
    fn test_half_to_f32_positive_infinity() {
        // f16 +inf: sign=0, exp=31, mantissa=0
        let bits: u16 = 0x7C00;
        let result = half_to_f32(bits);
        assert!(result.is_infinite());
        assert!(result.is_sign_positive());
    }

    #[test]
    fn test_half_to_f32_negative_infinity() {
        // f16 -inf: sign=1, exp=31, mantissa=0
        let bits: u16 = 0xFC00;
        let result = half_to_f32(bits);
        assert!(result.is_infinite());
        assert!(result.is_sign_negative());
    }

    #[test]
    fn test_half_to_f32_nan() {
        // f16 NaN: exp=31, mantissa!=0
        // Quiet NaN (mantissa MSB set)
        let qnan: u16 = 0x7E00;
        let qnan_result = half_to_f32(qnan);
        assert!(qnan_result.is_nan());

        // Signaling NaN (mantissa MSB clear, other bits set)
        let snan: u16 = 0x7C01;
        let snan_result = half_to_f32(snan);
        assert!(snan_result.is_nan());

        // Negative NaN
        let neg_nan: u16 = 0xFE00;
        let neg_nan_result = half_to_f32(neg_nan);
        assert!(neg_nan_result.is_nan());
    }

    #[test]
    fn test_half_to_f32_mantissa_preservation() {
        // Test that mantissa bits are correctly preserved during conversion
        // f16: 1.mantissa * 2^(exp-15)
        // Test with mantissa = 0x155 (alternating bits pattern)
        // exp = 15 (unbiased 0), so value = 1.010101010101 in binary
        let bits: u16 = 0x3D55; // sign=0, exp=15, mantissa=0x155
        let result = half_to_f32(bits);
        // 1.0 + 0x155/0x400 = 1.0 + 341/1024 ≈ 1.333
        let expected = 1.0 + (0x155 as f32) / (0x400 as f32);
        assert!(
            (result - expected).abs() < 1e-5,
            "mantissa preservation: got {}, expected {}",
            result,
            expected
        );
    }

    // ============ bf16_to_f32 tests ============

    #[test]
    fn test_bf16_to_f32_positive_zero() {
        // bf16 positive zero: all bits 0
        let bits: u16 = 0x0000;
        let result = bf16_to_f32(bits);
        assert_eq!(result, 0.0_f32);
        assert!(!result.is_sign_negative());
    }

    #[test]
    fn test_bf16_to_f32_negative_zero() {
        // bf16 negative zero: sign=1, rest=0
        let bits: u16 = 0x8000;
        let result = bf16_to_f32(bits);
        assert_eq!(result, -0.0_f32);
        assert!(result.is_sign_negative());
    }

    #[test]
    fn test_bf16_to_f32_positive_one() {
        // bf16 1.0: same as f32 1.0 with bottom 16 bits truncated
        // f32 1.0 = 0x3F800000, bf16 = 0x3F80
        let bits: u16 = 0x3F80;
        let result = bf16_to_f32(bits);
        assert_eq!(result, 1.0_f32);
    }

    #[test]
    fn test_bf16_to_f32_negative_one() {
        // bf16 -1.0 = 0xBF80
        let bits: u16 = 0xBF80;
        let result = bf16_to_f32(bits);
        assert_eq!(result, -1.0_f32);
    }

    #[test]
    fn test_bf16_to_f32_two() {
        // bf16 2.0 = 0x4000
        let bits: u16 = 0x4000;
        let result = bf16_to_f32(bits);
        assert_eq!(result, 2.0_f32);
    }

    #[test]
    fn test_bf16_to_f32_half() {
        // bf16 0.5 = 0x3F00
        let bits: u16 = 0x3F00;
        let result = bf16_to_f32(bits);
        assert_eq!(result, 0.5_f32);
    }

    #[test]
    fn test_bf16_to_f32_various_normalized() {
        // bf16 is truncated f32, so we can test by truncating known f32 values

        // pi ≈ 3.14159... f32 = 0x40490FDB, bf16 = 0x4049
        let pi_bits: u16 = 0x4049;
        let pi_result = bf16_to_f32(pi_bits);
        // Result should be close to pi but truncated
        assert!(
            (pi_result - std::f32::consts::PI).abs() < 0.02,
            "pi: got {}, expected ~{}",
            pi_result,
            std::f32::consts::PI
        );

        // Large number: 1000.0 f32 = 0x447A0000, bf16 = 0x447A
        let thousand_bits: u16 = 0x447A;
        let thousand_result = bf16_to_f32(thousand_bits);
        assert!(
            (thousand_result - 1000.0_f32).abs() < 1.0,
            "1000: got {}, expected ~1000",
            thousand_result
        );

        // Small number: 0.001 f32 ≈ 0x3A83126F, bf16 = 0x3A83
        let small_bits: u16 = 0x3A83;
        let small_result = bf16_to_f32(small_bits);
        assert!(
            (small_result - 0.001_f32).abs() < 0.0001,
            "0.001: got {}, expected ~0.001",
            small_result
        );
    }

    #[test]
    fn test_bf16_to_f32_denormalized() {
        // bf16 denormalized: exp=0, mantissa!=0
        // Smallest denorm bf16: 0x0001
        // This represents a very small denormalized f32
        let smallest: u16 = 0x0001;
        let result = bf16_to_f32(smallest);
        // Should be 2^(-126-16) = 2^-142 (extremely small but not zero)
        assert!(result > 0.0_f32);
        assert!(result.is_subnormal() || result.is_normal());
        assert!(result < 1e-38);

        // Negative denorm
        let neg_smallest: u16 = 0x8001;
        let neg_result = bf16_to_f32(neg_smallest);
        assert!(neg_result < 0.0_f32);
    }

    #[test]
    fn test_bf16_to_f32_positive_infinity() {
        // bf16 +inf: sign=0, exp=255, mantissa=0 -> 0x7F80
        let bits: u16 = 0x7F80;
        let result = bf16_to_f32(bits);
        assert!(result.is_infinite());
        assert!(result.is_sign_positive());
    }

    #[test]
    fn test_bf16_to_f32_negative_infinity() {
        // bf16 -inf: sign=1, exp=255, mantissa=0 -> 0xFF80
        let bits: u16 = 0xFF80;
        let result = bf16_to_f32(bits);
        assert!(result.is_infinite());
        assert!(result.is_sign_negative());
    }

    #[test]
    fn test_bf16_to_f32_nan() {
        // bf16 NaN: exp=255, mantissa!=0

        // Quiet NaN: 0x7FC0
        let qnan: u16 = 0x7FC0;
        let qnan_result = bf16_to_f32(qnan);
        assert!(qnan_result.is_nan());

        // Another NaN pattern: 0x7F81
        let nan2: u16 = 0x7F81;
        let nan2_result = bf16_to_f32(nan2);
        assert!(nan2_result.is_nan());

        // Negative NaN: 0xFFC0
        let neg_nan: u16 = 0xFFC0;
        let neg_nan_result = bf16_to_f32(neg_nan);
        assert!(neg_nan_result.is_nan());
    }

    #[test]
    fn test_bf16_to_f32_roundtrip_truncation() {
        // Verify bf16 conversion is consistent with f32 truncation
        // Take an f32, truncate to bf16, convert back
        let original: f32 = 1.234567_f32;
        let original_bits = original.to_bits();
        let bf16_bits = (original_bits >> 16) as u16;
        let roundtrip = bf16_to_f32(bf16_bits);

        // The roundtrip value should match truncating the original
        let expected = f32::from_bits(original_bits & 0xFFFF0000);
        assert_eq!(
            roundtrip, expected,
            "roundtrip: got {}, expected {}",
            roundtrip, expected
        );
    }
}
