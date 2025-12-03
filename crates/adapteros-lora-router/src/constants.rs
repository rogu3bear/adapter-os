//! Router configuration constants
//!
//! These constants define the default values for router configuration.
//! The k_sparse value is 4 (matching the schema default).

/// Default number of adapters to select per token (k-sparse selection)
///
/// This matches the AOS_ROUTER_K_SPARSE environment variable default.
pub const DEFAULT_K_SPARSE: usize = 4;

/// Default entropy floor for gate values
pub const DEFAULT_ENTROPY_FLOOR: f32 = 0.02;

/// Default gate quantization format (Q15 = 16-bit fixed-point)
pub const DEFAULT_GATE_QUANT_STR: &str = "q15";

/// Default sample tokens for full telemetry logging
/// Per Telemetry Ruleset #9
pub const DEFAULT_SAMPLE_TOKENS_FULL: usize = 128;

/// Default overhead budget percentage for router CPU
pub const DEFAULT_OVERHEAD_BUDGET_PCT: f32 = 8.0;

/// Maximum allowed k-sparse value
pub const MAX_K: usize = 8;

/// Default compression ratio for MPLoRA
pub const DEFAULT_COMPRESSION_RATIO: f32 = 0.8;

/// Boost value added to priors for pinned adapters (CHAT-PIN-02).
///
/// Creates preference without exclusivity - pinned adapters are more likely
/// to be selected but non-pinned can still win with higher feature scores.
/// This value is added to the prior score for each pinned adapter before
/// the router's scoring algorithm runs.
pub const PINNED_BOOST: f32 = 0.3;
