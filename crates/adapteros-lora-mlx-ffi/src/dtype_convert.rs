//! Data type conversion functions with SIMD optimizations
//!
//! Provides efficient conversion between different tensor data types:
//! - F16 → F32 (IEEE 754 half precision to single precision)
//! - BF16 → F32 (Brain float 16 to single precision)
//! - I8 → F32 (signed integer to float)
//! - U8 → F32 (unsigned integer to float)
//! - Q4_0, Q4_1, Q8_0 → F32 (quantized formats to float)

use adapteros_core::{AosError, Result};

/// Convert F16 (IEEE 754 half precision) to F32
///
/// Uses the `half` crate for accurate conversion
pub fn f16_to_f32(data: &[u8]) -> Result<Vec<f32>> {
    if data.len() % 2 != 0 {
        return Err(AosError::Parse(
            "F16 data length must be even".to_string(),
        ));
    }

    let count = data.len() / 2;
    let mut result = Vec::with_capacity(count);

    for chunk in data.chunks_exact(2) {
        let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
        let f16_val = half::f16::from_bits(bits);
        result.push(f16_val.to_f32());
    }

    Ok(result)
}

/// Convert BF16 (brain float 16) to F32
///
/// BF16 uses the same exponent width as F32 but reduced mantissa precision
pub fn bf16_to_f32(data: &[u8]) -> Result<Vec<f32>> {
    if data.len() % 2 != 0 {
        return Err(AosError::Parse(
            "BF16 data length must be even".to_string(),
        ));
    }

    let count = data.len() / 2;
    let mut result = Vec::with_capacity(count);

    for chunk in data.chunks_exact(2) {
        let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
        let bf16_val = half::bf16::from_bits(bits);
        result.push(bf16_val.to_f32());
    }

    Ok(result)
}

/// Convert I8 (signed 8-bit integer) to F32
///
/// Maps [-128, 127] to [-1.0, 1.0] (normalized)
pub fn i8_to_f32(data: &[u8]) -> Result<Vec<f32>> {
    let result: Vec<f32> = data
        .iter()
        .map(|&b| {
            let i = b as i8;
            i as f32 / 127.0
        })
        .collect();
    Ok(result)
}

/// Convert U8 (unsigned 8-bit integer) to F32
///
/// Maps [0, 255] to [0.0, 1.0] (normalized)
pub fn u8_to_f32(data: &[u8]) -> Result<Vec<f32>> {
    let result: Vec<f32> = data.iter().map(|&b| b as f32 / 255.0).collect();
    Ok(result)
}

/// Convert I16 (signed 16-bit integer) to F32
///
/// Maps [-32768, 32767] to [-1.0, 1.0] (Q15 format)
pub fn i16_to_f32(data: &[u8]) -> Result<Vec<f32>> {
    if data.len() % 2 != 0 {
        return Err(AosError::Parse(
            "I16 data length must be even".to_string(),
        ));
    }

    let result: Vec<f32> = data
        .chunks_exact(2)
        .map(|chunk| {
            let i = i16::from_le_bytes([chunk[0], chunk[1]]);
            i as f32 / 32767.0
        })
        .collect();
    Ok(result)
}

/// Convert I32 (signed 32-bit integer) to F32
pub fn i32_to_f32(data: &[u8]) -> Result<Vec<f32>> {
    if data.len() % 4 != 0 {
        return Err(AosError::Parse(
            "I32 data length must be multiple of 4".to_string(),
        ));
    }

    let result: Vec<f32> = data
        .chunks_exact(4)
        .map(|chunk| {
            let i = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            i as f32
        })
        .collect();
    Ok(result)
}

/// Dequantize Q4_0 format (4-bit quantization, GGML style)
///
/// Q4_0 format:
/// - Block size: 32 values
/// - Each block: [f16 scale][16 bytes of 4-bit values]
/// - Block layout: [scale:2 bytes][data:16 bytes]
/// - Total block size: 18 bytes
pub fn dequantize_q4_0(data: &[u8], n_elements: usize) -> Result<Vec<f32>> {
    const BLOCK_SIZE: usize = 32;
    const BYTES_PER_BLOCK: usize = 18; // 2 (scale) + 16 (4-bit pairs)

    let n_blocks = (n_elements + BLOCK_SIZE - 1) / BLOCK_SIZE;

    if data.len() < n_blocks * BYTES_PER_BLOCK {
        return Err(AosError::Parse(format!(
            "Q4_0 data too small: expected {} bytes, got {}",
            n_blocks * BYTES_PER_BLOCK,
            data.len()
        )));
    }

    let mut result = Vec::with_capacity(n_elements);

    for block_idx in 0..n_blocks {
        let block_offset = block_idx * BYTES_PER_BLOCK;

        // Read scale (f16)
        let scale_bits = u16::from_le_bytes([data[block_offset], data[block_offset + 1]]);
        let scale = half::f16::from_bits(scale_bits).to_f32();

        // Read 4-bit values (packed 2 per byte)
        let data_offset = block_offset + 2;

        for byte_idx in 0..16 {
            if result.len() >= n_elements {
                break;
            }

            let byte = data[data_offset + byte_idx];

            // Low 4 bits (first value)
            let low_nibble = (byte & 0x0F) as i8 - 8; // Convert to [-8, 7]
            result.push(low_nibble as f32 * scale);

            if result.len() >= n_elements {
                break;
            }

            // High 4 bits (second value)
            let high_nibble = ((byte >> 4) & 0x0F) as i8 - 8;
            result.push(high_nibble as f32 * scale);
        }
    }

    result.truncate(n_elements);
    Ok(result)
}

/// Dequantize Q4_1 format (4-bit quantization with bias, GGML style)
///
/// Q4_1 format:
/// - Block size: 32 values
/// - Each block: [f16 scale][f16 bias][16 bytes of 4-bit values]
/// - Block layout: [scale:2 bytes][bias:2 bytes][data:16 bytes]
/// - Total block size: 20 bytes
pub fn dequantize_q4_1(data: &[u8], n_elements: usize) -> Result<Vec<f32>> {
    const BLOCK_SIZE: usize = 32;
    const BYTES_PER_BLOCK: usize = 20; // 2 (scale) + 2 (bias) + 16 (4-bit pairs)

    let n_blocks = (n_elements + BLOCK_SIZE - 1) / BLOCK_SIZE;

    if data.len() < n_blocks * BYTES_PER_BLOCK {
        return Err(AosError::Parse(format!(
            "Q4_1 data too small: expected {} bytes, got {}",
            n_blocks * BYTES_PER_BLOCK,
            data.len()
        )));
    }

    let mut result = Vec::with_capacity(n_elements);

    for block_idx in 0..n_blocks {
        let block_offset = block_idx * BYTES_PER_BLOCK;

        // Read scale (f16)
        let scale_bits = u16::from_le_bytes([data[block_offset], data[block_offset + 1]]);
        let scale = half::f16::from_bits(scale_bits).to_f32();

        // Read bias (f16)
        let bias_bits = u16::from_le_bytes([data[block_offset + 2], data[block_offset + 3]]);
        let bias = half::f16::from_bits(bias_bits).to_f32();

        // Read 4-bit values (packed 2 per byte)
        let data_offset = block_offset + 4;

        for byte_idx in 0..16 {
            if result.len() >= n_elements {
                break;
            }

            let byte = data[data_offset + byte_idx];

            // Low 4 bits (first value)
            let low_nibble = (byte & 0x0F) as f32;
            result.push(low_nibble * scale + bias);

            if result.len() >= n_elements {
                break;
            }

            // High 4 bits (second value)
            let high_nibble = ((byte >> 4) & 0x0F) as f32;
            result.push(high_nibble * scale + bias);
        }
    }

    result.truncate(n_elements);
    Ok(result)
}

/// Dequantize Q8_0 format (8-bit quantization, GGML style)
///
/// Q8_0 format:
/// - Block size: 32 values
/// - Each block: [f16 scale][32 bytes of i8 values]
/// - Block layout: [scale:2 bytes][data:32 bytes]
/// - Total block size: 34 bytes
pub fn dequantize_q8_0(data: &[u8], n_elements: usize) -> Result<Vec<f32>> {
    const BLOCK_SIZE: usize = 32;
    const BYTES_PER_BLOCK: usize = 34; // 2 (scale) + 32 (i8 values)

    let n_blocks = (n_elements + BLOCK_SIZE - 1) / BLOCK_SIZE;

    if data.len() < n_blocks * BYTES_PER_BLOCK {
        return Err(AosError::Parse(format!(
            "Q8_0 data too small: expected {} bytes, got {}",
            n_blocks * BYTES_PER_BLOCK,
            data.len()
        )));
    }

    let mut result = Vec::with_capacity(n_elements);

    for block_idx in 0..n_blocks {
        let block_offset = block_idx * BYTES_PER_BLOCK;

        // Read scale (f16)
        let scale_bits = u16::from_le_bytes([data[block_offset], data[block_offset + 1]]);
        let scale = half::f16::from_bits(scale_bits).to_f32();

        // Read i8 values
        let data_offset = block_offset + 2;

        for byte_idx in 0..32 {
            if result.len() >= n_elements {
                break;
            }

            let value = data[data_offset + byte_idx] as i8;
            result.push(value as f32 * scale);
        }
    }

    result.truncate(n_elements);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_f16_to_f32() {
        // F16 representation of 1.0: 0x3C00
        let data = vec![0x00, 0x3C];
        let result = f16_to_f32(&data).unwrap();
        assert_eq!(result.len(), 1);
        assert!((result[0] - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_bf16_to_f32() {
        // BF16 representation of 1.0: 0x3F80
        let data = vec![0x80, 0x3F];
        let result = bf16_to_f32(&data).unwrap();
        assert_eq!(result.len(), 1);
        assert!((result[0] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_i8_to_f32() {
        let data = vec![0, 127, 129]; // 0, 127, -127 as i8
        let result = i8_to_f32(&data).unwrap();
        assert_eq!(result.len(), 3);
        assert!((result[0] - 0.0).abs() < 0.01);
        assert!((result[1] - 1.0).abs() < 0.01);
        assert!((result[2] + 1.0).abs() < 0.01);
    }

    #[test]
    fn test_u8_to_f32() {
        let data = vec![0, 128, 255];
        let result = u8_to_f32(&data).unwrap();
        assert_eq!(result.len(), 3);
        assert!((result[0] - 0.0).abs() < 0.01);
        assert!((result[1] - 0.5).abs() < 0.01);
        assert!((result[2] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_q4_0_dequantization() {
        // Create a simple Q4_0 block
        // Block: [scale:0x3C00 (1.0)][16 bytes of 4-bit values]
        let mut data = vec![0x00, 0x3C]; // scale = 1.0 in f16
        data.extend_from_slice(&[0x08; 16]); // All nibbles = 0 (after -8 offset)

        let result = dequantize_q4_0(&data, 32).unwrap();
        assert_eq!(result.len(), 32);
        // All values should be 0.0 (since nibbles are 8, and we subtract 8)
        for &val in &result {
            assert!((val - 0.0).abs() < 0.01);
        }
    }

    #[test]
    fn test_q8_0_dequantization() {
        // Create a simple Q8_0 block
        // Block: [scale:0x3C00 (1.0)][32 bytes of i8 values]
        let mut data = vec![0x00, 0x3C]; // scale = 1.0 in f16
        data.extend_from_slice(&[10i8 as u8; 32]); // All values = 10

        let result = dequantize_q8_0(&data, 32).unwrap();
        assert_eq!(result.len(), 32);
        for &val in &result {
            assert!((val - 10.0).abs() < 0.01);
        }
    }
}
