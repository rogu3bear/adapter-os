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

#[inline]
pub(crate) fn quantize_gate(gate: f32) -> i16 {
    let scaled = (gate * ROUTER_GATE_Q15_DENOM).round() as i32;
    scaled.clamp(0, ROUTER_GATE_Q15_MAX as i32) as i16
}
