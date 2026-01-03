use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

/// Q15 gate constants for router outputs (deterministic, non-negative gates)
///
/// # Why 32767 and not 32768?
///
/// Q15 fixed-point format represents values in [-1.0, 1.0) using signed 16-bit integers.
/// The maximum positive value is 32767 (0x7FFF), not 32768, because:
///
/// 1. **i16 range**: -32768 to 32767. Using 32768 would overflow.
/// 2. **Precision**: 32767.0 gives exact representation of 1.0 when gate=32767.
///    Using 32768.0 would make max gate = 0.99997, losing the ability to express "full weight".
/// 3. **Determinism**: Consistent denominator ensures identical f32→Q15→f32 round-trips.
///
/// # Usage
/// - Encode: `gate_q15 = (gate_f32 * 32767.0).round() as i16`
/// - Decode: `gate_f32 = gate_q15 as f32 / 32767.0`
///
/// # Critical Invariant
/// **DO NOT CHANGE TO 32768** - This would break determinism proofs and replay verification.
pub const ROUTER_GATE_Q15_DENOM: f32 = 32767.0;
pub const ROUTER_GATE_Q15_MAX: i16 = 32767;

/// String identifier for Q15 format in metadata
pub const Q15_FORMAT_NAME: &str = "Q15";

const _: () = {
    if ROUTER_GATE_Q15_DENOM != 32767.0 {
        panic!("ROUTER_GATE_Q15_DENOM must remain 32767.0 for determinism");
    }
    if ROUTER_GATE_Q15_MAX != i16::MAX {
        panic!("ROUTER_GATE_Q15_MAX must remain i16::MAX for determinism");
    }
};

// =============================================================================
// GateQuantFormat - Quantization metadata for stored gates
// =============================================================================

/// Gate quantization format metadata for deterministic storage and replay.
///
/// Stores the quantization scheme alongside gate values to prevent silent
/// divergence when constants change. On read, the format is validated to
/// ensure gates were encoded with the expected denominator.
///
/// # Invariants
///
/// 1. **Format must match**: The `q_format` field must be "Q15" for current schema.
/// 2. **Denominator must match**: The `denom` field must equal `ROUTER_GATE_Q15_DENOM`.
/// 3. **Fail closed**: Mismatches cause errors, not silent degradation.
///
/// # Example
///
/// ```ignore
/// use adapteros_lora_router::quantization::{GateQuantFormat, ROUTER_GATE_Q15_DENOM};
///
/// // Store with metadata
/// let format = GateQuantFormat::q15();
/// let stored = serde_json::json!({
///     "gate_format": format,
///     "gates_q15": [16384, 8192, 8191]
/// });
///
/// // Validate on read
/// let loaded_format: GateQuantFormat = serde_json::from_value(stored["gate_format"].clone())?;
/// loaded_format.validate()?;
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateQuantFormat {
    /// Quantization format identifier (e.g., "Q15")
    pub q_format: String,
    /// Denominator used for quantization (e.g., 32767.0 for Q15)
    pub denom: f32,
}

impl GateQuantFormat {
    /// Create a new Q15 format with the canonical denominator.
    ///
    /// This is the standard format for all router gate storage.
    pub fn q15() -> Self {
        Self {
            q_format: Q15_FORMAT_NAME.to_string(),
            denom: ROUTER_GATE_Q15_DENOM,
        }
    }

    /// Create a Q15 format with a specific denominator (for testing/migration).
    #[cfg(test)]
    pub fn with_denom(denom: f32) -> Self {
        Self {
            q_format: Q15_FORMAT_NAME.to_string(),
            denom,
        }
    }

    /// Validate that this format matches the expected Q15 schema.
    ///
    /// # Returns
    /// `Ok(())` if format is valid, `Err` with details otherwise.
    ///
    /// # Errors
    /// Returns `AosError::DeterminismViolation` if:
    /// - `q_format` is not "Q15"
    /// - `denom` does not match `ROUTER_GATE_Q15_DENOM`
    pub fn validate(&self) -> Result<()> {
        if self.q_format != Q15_FORMAT_NAME {
            return Err(AosError::DeterminismViolation(format!(
                "GateQuantFormat: unsupported format '{}', expected '{}'",
                self.q_format, Q15_FORMAT_NAME
            )));
        }

        // Use ulps comparison for f32 since we're comparing known constants
        if (self.denom - ROUTER_GATE_Q15_DENOM).abs() > f32::EPSILON {
            return Err(AosError::DeterminismViolation(format!(
                "GateQuantFormat: denom mismatch {} != expected {}. \
                 Gate values may have been encoded with incompatible scaling.",
                self.denom, ROUTER_GATE_Q15_DENOM
            )));
        }

        Ok(())
    }

    /// Check if this format is the canonical Q15 format.
    pub fn is_q15(&self) -> bool {
        self.q_format == Q15_FORMAT_NAME
            && (self.denom - ROUTER_GATE_Q15_DENOM).abs() <= f32::EPSILON
    }

    /// Decode a Q15 gate value to f32 using this format's denominator.
    ///
    /// # Arguments
    /// * `gate_q15` - The quantized gate value
    ///
    /// # Returns
    /// The decoded f32 gate value in [0.0, 1.0]
    #[inline]
    pub fn decode(&self, gate_q15: i16) -> f32 {
        gate_q15 as f32 / self.denom
    }

    /// Encode a f32 gate value to Q15 using this format's denominator.
    ///
    /// # Arguments
    /// * `gate` - The f32 gate value, expected in [0.0, 1.0]
    ///
    /// # Returns
    /// The quantized i16 gate value
    #[inline]
    pub fn encode(&self, gate: f32) -> i16 {
        let scaled = (gate * self.denom).round() as i32;
        scaled.clamp(0, ROUTER_GATE_Q15_MAX as i32) as i16
    }
}

impl Default for GateQuantFormat {
    fn default() -> Self {
        Self::q15()
    }
}

impl std::fmt::Display for GateQuantFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}(denom={})", self.q_format, self.denom)
    }
}

#[inline]
pub(crate) fn quantize_gate(gate: f32) -> i16 {
    let scaled = (gate * ROUTER_GATE_Q15_DENOM).round() as i32;
    scaled.clamp(0, ROUTER_GATE_Q15_MAX as i32) as i16
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_quant_format_q15_default() {
        let format = GateQuantFormat::q15();
        assert_eq!(format.q_format, Q15_FORMAT_NAME);
        assert_eq!(format.denom, ROUTER_GATE_Q15_DENOM);
        assert!(format.is_q15());
    }

    #[test]
    fn gate_quant_format_validate_passes() {
        let format = GateQuantFormat::q15();
        assert!(format.validate().is_ok());
    }

    #[test]
    fn gate_quant_format_validate_wrong_format() {
        let format = GateQuantFormat {
            q_format: "Q8".to_string(),
            denom: 127.0,
        };
        assert!(format.validate().is_err());
    }

    #[test]
    fn gate_quant_format_validate_wrong_denom() {
        let format = GateQuantFormat::with_denom(32768.0);
        assert!(format.validate().is_err());
    }

    #[test]
    fn gate_quant_format_encode_decode_roundtrip() {
        let format = GateQuantFormat::q15();
        for gate in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            let encoded = format.encode(gate);
            let decoded = format.decode(encoded);
            // Allow small rounding error
            assert!(
                (gate - decoded).abs() < 0.0001,
                "Roundtrip failed for {}",
                gate
            );
        }
    }

    #[test]
    fn gate_quant_format_serialization_roundtrip() {
        let format = GateQuantFormat::q15();
        let json = serde_json::to_string(&format).expect("serialize");
        let deserialized: GateQuantFormat = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(format, deserialized);
        assert!(deserialized.validate().is_ok());
    }

    #[test]
    fn quantize_gate_clamps_negative() {
        assert_eq!(quantize_gate(-0.5), 0);
    }

    #[test]
    fn quantize_gate_clamps_overflow() {
        assert_eq!(quantize_gate(1.5), ROUTER_GATE_Q15_MAX);
    }

    #[test]
    fn quantize_gate_standard_values() {
        assert_eq!(quantize_gate(0.0), 0);
        assert_eq!(quantize_gate(0.5), 16384); // 32767 / 2 rounded
        assert_eq!(quantize_gate(1.0), 32767);
    }
}
