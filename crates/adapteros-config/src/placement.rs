//! Energy/thermal-aware placement configuration.
//!
//! This module centralizes the knobs for per-token device placement across
//! CPU/GPU/ANE targets. Defaults favor a mixed objective (latency + thermal +
//! energy) to match the current adapterOS posture.

use crate::model::load_dotenv;
use serde::{Deserialize, Serialize};

/// Placement strategy for device selection.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum PlacementMode {
    /// Disable adaptive placement (stick to primary backend)
    Off,
    /// Balance latency, thermal headroom, and energy draw
    #[default]
    Balanced,
    /// Prioritize lowest latency, tolerate higher energy/thermal cost
    Latency,
    /// Prioritize energy efficiency/battery, accept more latency
    Energy,
    /// Prioritize thermal headroom to avoid throttling
    Thermal,
}

impl std::str::FromStr for PlacementMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.to_ascii_lowercase();
        match normalized.as_str() {
            "balanced" => Ok(PlacementMode::Balanced),
            "latency" => Ok(PlacementMode::Latency),
            "energy" => Ok(PlacementMode::Energy),
            "thermal" => Ok(PlacementMode::Thermal),
            "off" | "disabled" => Ok(PlacementMode::Off),
            other => Err(format!(
                "Invalid placement mode '{}'. Expected balanced|latency|energy|thermal|off",
                other
            )),
        }
    }
}

/// Weighting for the cost model.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PlacementWeights {
    pub latency: f32,
    pub energy: f32,
    pub thermal: f32,
}

impl Default for PlacementWeights {
    fn default() -> Self {
        // Mixed objective requested by default (latency + thermal + energy)
        Self {
            latency: 0.5,
            energy: 0.25,
            thermal: 0.25,
        }
    }
}

/// Configuration surface for placement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementConfig {
    pub mode: PlacementMode,
    pub weights: PlacementWeights,
    /// Thermal ceiling in Celsius where we start steering away from hot devices.
    pub thermal_ceiling_c: f32,
    /// Minimum cooldown steps before reconsidering a hot device.
    pub cooldown_steps: u32,
    /// Sampling cadence for telemetry (ms). Kept coarse to avoid overhead.
    pub sample_ms: u64,
}

impl Default for PlacementConfig {
    fn default() -> Self {
        Self {
            mode: PlacementMode::Balanced,
            weights: PlacementWeights::default(),
            thermal_ceiling_c: 84.0,
            cooldown_steps: 4,
            sample_ms: 250,
        }
    }
}

impl PlacementConfig {
    pub fn from_env() -> Self {
        load_dotenv();

        let mut cfg = PlacementConfig::default();

        if let Ok(mode) = std::env::var("AOS_PLACEMENT_MODE") {
            cfg.mode = mode.parse().unwrap_or(PlacementMode::Balanced);
        }

        if let Ok(val) = std::env::var("AOS_PLACEMENT_LATENCY_WEIGHT") {
            if let Ok(f) = val.parse::<f32>() {
                cfg.weights.latency = f;
            }
        }

        if let Ok(val) = std::env::var("AOS_PLACEMENT_ENERGY_WEIGHT") {
            if let Ok(f) = val.parse::<f32>() {
                cfg.weights.energy = f;
            }
        }

        if let Ok(val) = std::env::var("AOS_PLACEMENT_THERMAL_WEIGHT") {
            if let Ok(f) = val.parse::<f32>() {
                cfg.weights.thermal = f;
            }
        }

        if let Ok(val) = std::env::var("AOS_PLACEMENT_THERMAL_CEILING_C") {
            if let Ok(f) = val.parse::<f32>() {
                cfg.thermal_ceiling_c = f;
            }
        }

        if let Ok(val) = std::env::var("AOS_PLACEMENT_COOLDOWN_STEPS") {
            if let Ok(v) = val.parse::<u32>() {
                cfg.cooldown_steps = v.max(1);
            }
        }

        if let Ok(val) = std::env::var("AOS_PLACEMENT_SAMPLE_MS") {
            if let Ok(v) = val.parse::<u64>() {
                cfg.sample_ms = v.clamp(50, 5_000);
            }
        }

        cfg
    }
}
