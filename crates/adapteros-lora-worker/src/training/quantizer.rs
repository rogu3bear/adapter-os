//! Q15 quantization for LoRA weights
//!
//! Converts f32 LoRA weights to i16 Q15 format for efficient storage and inference.

use super::trainer::LoRAWeights;
use serde::{Deserialize, Serialize};
use tracing::info;

/// LoRA quantizer for Q15 format
pub struct LoRAQuantizer;

/// Quantized LoRA weights in Q15 format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedLoRAWeights {
    /// Quantized lora_a matrices (i16 Q15 format)
    pub lora_a_q15: Vec<Vec<i16>>,
    /// Quantized lora_b matrices (i16 Q15 format)
    pub lora_b_q15: Vec<Vec<i16>>,
    /// Scaling factors for lora_a (per row)
    pub scale_a: Vec<f32>,
    /// Scaling factors for lora_b (per row)
    pub scale_b: Vec<f32>,
}

impl LoRAQuantizer {
    /// Quantize LoRA weights to Q15 format
    ///
    /// Q15 format: fixed-point representation with 15 fractional bits
    /// Range: [-1.0, 1.0) mapped to [-32768, 32767]
    pub fn quantize_to_q15(weights: &LoRAWeights) -> QuantizedLoRAWeights {
        info!(
            "Quantizing LoRA weights to Q15: lora_a={}x{}, lora_b={}x{}",
            weights.lora_a.len(),
            weights.lora_a.first().map(|r| r.len()).unwrap_or(0),
            weights.lora_b.len(),
            weights.lora_b.first().map(|r| r.len()).unwrap_or(0),
        );

        // Quantize lora_a
        let (lora_a_q15, scale_a) = Self::quantize_matrix(&weights.lora_a);

        // Quantize lora_b
        let (lora_b_q15, scale_b) = Self::quantize_matrix(&weights.lora_b);

        info!(
            "Quantization complete: avg_scale_a={:.6}, avg_scale_b={:.6}",
            scale_a.iter().sum::<f32>() / scale_a.len() as f32,
            scale_b.iter().sum::<f32>() / scale_b.len() as f32
        );

        QuantizedLoRAWeights {
            lora_a_q15,
            lora_b_q15,
            scale_a,
            scale_b,
        }
    }

    /// Dequantize Q15 weights back to f32
    pub fn dequantize_from_q15(weights: &QuantizedLoRAWeights) -> LoRAWeights {
        info!("Dequantizing Q15 weights to f32");

        // Dequantize lora_a
        let lora_a = Self::dequantize_matrix(&weights.lora_a_q15, &weights.scale_a);

        // Dequantize lora_b
        let lora_b = Self::dequantize_matrix(&weights.lora_b_q15, &weights.scale_b);

        LoRAWeights { lora_a, lora_b }
    }

    /// Quantize a 2D matrix with per-row scaling
    fn quantize_matrix(matrix: &[Vec<f32>]) -> (Vec<Vec<i16>>, Vec<f32>) {
        let mut quantized = Vec::with_capacity(matrix.len());
        let mut scales = Vec::with_capacity(matrix.len());

        for row in matrix {
            let (q_row, scale) = Self::quantize_row(row);
            quantized.push(q_row);
            scales.push(scale);
        }

        (quantized, scales)
    }

    /// Quantize a single row
    fn quantize_row(row: &[f32]) -> (Vec<i16>, f32) {
        if row.is_empty() {
            return (Vec::new(), 1.0);
        }

        // Find maximum absolute value for scaling
        let max_abs = row
            .iter()
            .map(|&v| v.abs())
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(1.0);

        // Compute scale to map max_abs to Q15 range
        let scale = if max_abs > 0.0 {
            max_abs / 32767.0
        } else {
            1.0
        };

        // Quantize each value
        let quantized: Vec<i16> = row
            .iter()
            .map(|&v| Self::quantize_value(v, scale))
            .collect();

        (quantized, scale)
    }

    /// Quantize a single f32 value to i16 Q15
    fn quantize_value(value: f32, scale: f32) -> i16 {
        let normalized = value / scale;
        let quantized = (normalized * 32768.0).clamp(-32768.0, 32767.0);
        quantized as i16
    }

    /// Dequantize a 2D matrix
    fn dequantize_matrix(matrix: &[Vec<i16>], scales: &[f32]) -> Vec<Vec<f32>> {
        matrix
            .iter()
            .zip(scales.iter())
            .map(|(row, &scale)| Self::dequantize_row(row, scale))
            .collect()
    }

    /// Dequantize a single row
    fn dequantize_row(row: &[i16], scale: f32) -> Vec<f32> {
        row.iter()
            .map(|&v| Self::dequantize_value(v, scale))
            .collect()
    }

    /// Dequantize a single i16 Q15 value to f32
    fn dequantize_value(value: i16, scale: f32) -> f32 {
        let normalized = value as f32 / 32768.0;
        normalized * scale
    }

    /// Calculate quantization error (MSE)
    pub fn calculate_error(original: &LoRAWeights, quantized: &QuantizedLoRAWeights) -> f32 {
        let dequantized = Self::dequantize_from_q15(quantized);

        let mut total_error = 0.0;
        let mut count = 0;

        // Compare lora_a
        for (orig_row, deq_row) in original.lora_a.iter().zip(dequantized.lora_a.iter()) {
            for (&orig_val, &deq_val) in orig_row.iter().zip(deq_row.iter()) {
                let diff = orig_val - deq_val;
                total_error += diff * diff;
                count += 1;
            }
        }

        // Compare lora_b
        for (orig_row, deq_row) in original.lora_b.iter().zip(dequantized.lora_b.iter()) {
            for (&orig_val, &deq_val) in orig_row.iter().zip(deq_row.iter()) {
                let diff = orig_val - deq_val;
                total_error += diff * diff;
                count += 1;
            }
        }

        if count > 0 {
            total_error / count as f32
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantize_value() {
        let scale = 1.0;

        // Test typical values
        assert_eq!(LoRAQuantizer::quantize_value(0.0, scale), 0);
        assert_eq!(LoRAQuantizer::quantize_value(1.0, scale), 32767);
        assert_eq!(LoRAQuantizer::quantize_value(-1.0, scale), -32768);

        // Test clamping
        assert_eq!(LoRAQuantizer::quantize_value(2.0, scale), 32767);
        assert_eq!(LoRAQuantizer::quantize_value(-2.0, scale), -32768);
    }

    #[test]
    fn test_dequantize_value() {
        let scale = 1.0;

        assert!((LoRAQuantizer::dequantize_value(0, scale) - 0.0).abs() < 1e-6);
        assert!((LoRAQuantizer::dequantize_value(32767, scale) - 1.0).abs() < 0.01);
        assert!((LoRAQuantizer::dequantize_value(-32768, scale) - (-1.0)).abs() < 0.01);
    }

    #[test]
    fn test_quantize_row() {
        let row = vec![0.0, 0.5, -0.5, 1.0, -1.0];
        let (quantized, scale) = LoRAQuantizer::quantize_row(&row);

        assert_eq!(quantized.len(), 5);
        assert!(scale > 0.0);

        // Verify clamping
        assert_eq!(quantized[3], 32767); // 1.0 maps to max
        assert_eq!(quantized[4], -32768); // -1.0 maps to min
    }

    #[test]
    fn test_round_trip_quantization() {
        let original = LoRAWeights {
            lora_a: vec![vec![0.1, -0.2, 0.3], vec![-0.1, 0.2, -0.3]],
            lora_b: vec![vec![0.5, -0.5], vec![0.4, -0.4], vec![0.3, -0.3]],
        };

        // Quantize
        let quantized = LoRAQuantizer::quantize_to_q15(&original);

        // Verify structure
        assert_eq!(quantized.lora_a_q15.len(), 2);
        assert_eq!(quantized.lora_b_q15.len(), 3);
        assert_eq!(quantized.scale_a.len(), 2);
        assert_eq!(quantized.scale_b.len(), 3);

        // Dequantize
        let dequantized = LoRAQuantizer::dequantize_from_q15(&quantized);

        // Check structure matches
        assert_eq!(dequantized.lora_a.len(), original.lora_a.len());
        assert_eq!(dequantized.lora_b.len(), original.lora_b.len());

        // Calculate error
        let error = LoRAQuantizer::calculate_error(&original, &quantized);
        assert!(error < 0.01, "Quantization error too high: {}", error);
    }

    #[test]
    fn test_quantize_empty() {
        let empty_row: Vec<f32> = vec![];
        let (quantized, scale) = LoRAQuantizer::quantize_row(&empty_row);

        assert!(quantized.is_empty());
        assert_eq!(scale, 1.0);
    }

    #[test]
    fn test_quantize_zeros() {
        let zeros = vec![0.0; 10];
        let (quantized, scale) = LoRAQuantizer::quantize_row(&zeros);

        assert_eq!(quantized.len(), 10);
        assert!(quantized.iter().all(|&v| v == 0));
    }
}
