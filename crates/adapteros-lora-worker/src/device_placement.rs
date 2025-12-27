//! Deterministic device placement with thermal/energy awareness.
//!
//! This module provides:
//! - Telemetry snapshotting (CPU/GPU/ANE best-effort on macOS)
//! - Deterministic, quantized cost model for lane selection
//! - Placement trace helpers for audit/replay

use adapteros_config::{PlacementConfig, PlacementMode};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[cfg(feature = "telemetry-sysinfo")]
use sysinfo::{CpuRefreshKind, RefreshKind, System};

use crate::BackendLane;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceKind {
    Cpu,
    Gpu,
    Ane,
}

#[derive(Debug, Clone, Copy)]
pub struct DeviceSample {
    pub utilization: f32,
    pub temperature_c: Option<f32>,
    pub power_w: Option<f32>,
}

impl Default for DeviceSample {
    fn default() -> Self {
        Self {
            utilization: 0.0,
            temperature_c: None,
            power_w: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TelemetrySnapshot {
    pub cpu: DeviceSample,
    pub gpu: DeviceSample,
    pub ane: DeviceSample,
    pub captured_at: Instant,
}

/// Best-effort telemetry collector (macOS-focused).
pub struct TelemetryCollector {
    #[cfg(feature = "telemetry-sysinfo")]
    sys: System,
    last: TelemetrySnapshot,
    sample_interval: Duration,
}

impl TelemetryCollector {
    pub fn new(sample_ms: u64) -> Self {
        let snapshot = TelemetrySnapshot {
            cpu: DeviceSample::default(),
            gpu: DeviceSample::default(),
            ane: DeviceSample::default(),
            captured_at: Instant::now(),
        };

        #[cfg(feature = "telemetry-sysinfo")]
        {
            let mut sys = System::new_with_specifics(
                RefreshKind::new().with_cpu(CpuRefreshKind::everything()),
            );
            sys.refresh_cpu();
            return Self {
                sys,
                last: snapshot.clone(),
                sample_interval: Duration::from_millis(sample_ms.clamp(50, 5_000)),
            };
        }

        #[cfg(not(feature = "telemetry-sysinfo"))]
        Self {
            last: snapshot.clone(),
            sample_interval: Duration::from_millis(sample_ms.clamp(50, 5_000)),
        }
    }

    pub fn snapshot(&mut self) -> TelemetrySnapshot {
        if self.last.captured_at.elapsed() < self.sample_interval {
            return self.last.clone();
        }

        self.last = self.collect();
        self.last.clone()
    }

    #[cfg(feature = "telemetry-sysinfo")]
    fn collect(&mut self) -> TelemetrySnapshot {
        self.sys.refresh_cpu();
        let cpu_util = self.sys.global_cpu_info().cpu_usage().clamp(0.0, 100.0) / 100.0;
        let cpu_sample = DeviceSample {
            utilization: quantize(cpu_util),
            temperature_c: Some(estimate_temp(cpu_util)),
            power_w: None,
        };

        let gpu_sample = collect_gpu_sample();
        let ane_sample = collect_ane_sample();

        TelemetrySnapshot {
            cpu: cpu_sample,
            gpu: gpu_sample,
            ane: ane_sample,
            captured_at: Instant::now(),
        }
    }

    #[cfg(not(feature = "telemetry-sysinfo"))]
    fn collect(&mut self) -> TelemetrySnapshot {
        TelemetrySnapshot {
            cpu: DeviceSample::default(),
            gpu: collect_gpu_sample(),
            ane: collect_ane_sample(),
            captured_at: Instant::now(),
        }
    }
}

fn collect_ane_sample() -> DeviceSample {
    // Without public ANE counters, approximate with a low-util placeholder.
    DeviceSample {
        utilization: quantize(0.05),
        temperature_c: None,
        power_w: None,
    }
}

#[cfg(target_os = "macos")]
fn collect_gpu_sample() -> DeviceSample {
    use metal::Device;

    if let Some(device) = Device::system_default() {
        let alloc = device.current_allocated_size() as f32;
        let budget = device.recommended_max_working_set_size() as f32;
        let util = if budget > 0.0 {
            (alloc / budget).clamp(0.0, 1.5)
        } else {
            0.0
        };
        return DeviceSample {
            utilization: quantize(util),
            temperature_c: None,
            power_w: None,
        };
    }

    DeviceSample::default()
}

#[cfg(not(target_os = "macos"))]
fn collect_gpu_sample() -> DeviceSample {
    DeviceSample::default()
}

fn quantize(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

#[allow(dead_code)] // reserved for future placement heuristics
fn estimate_temp(util: f32) -> f32 {
    let approx = 35.0 + util * 45.0;
    approx.clamp(30.0, 100.0)
}

#[derive(Debug, Clone)]
pub struct LaneDescriptor {
    pub lane: BackendLane,
    pub kind: DeviceKind,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct PlacementDecision {
    pub lane: BackendLane,
    pub lane_name: String,
    pub score: f32,
    pub utilization: f32,
    pub temperature_c: Option<f32>,
}

/// Deterministic, quantized cost model for device placement.
pub struct PlacementEngine {
    cfg: PlacementConfig,
    cooldowns: HashMap<DeviceKind, u32>,
}

impl PlacementEngine {
    pub fn new(cfg: PlacementConfig) -> Self {
        Self {
            cfg,
            cooldowns: HashMap::new(),
        }
    }

    pub fn mode(&self) -> PlacementMode {
        self.cfg.mode
    }

    /// Choose the best lane, returning None when placement is disabled or only
    /// one lane is available.
    pub fn choose_lane(
        &mut self,
        lanes: &[LaneDescriptor],
        snap: &TelemetrySnapshot,
    ) -> Option<PlacementDecision> {
        // When telemetry is not available (feature off), avoid steering to keep behavior stable.
        // Allow tests to execute deterministically even without the feature.
        if !cfg!(feature = "telemetry-sysinfo") && !cfg!(test) {
            return None;
        }
        if telemetry_empty(snap) {
            return None;
        }
        if self.cfg.mode == PlacementMode::Off || lanes.len() <= 1 {
            return None;
        }

        self.tick_cooldowns();

        let mut best: Option<(PlacementDecision, f32)> = None;
        for lane in lanes {
            let sample = sample_for_kind(lane.kind, snap);
            if let Some(temp) = sample.temperature_c {
                if temp > self.cfg.thermal_ceiling_c {
                    continue; // Skip lanes over ceiling if we have alternatives
                }
            }
            let score = self.score_lane(lane.kind, sample);

            if let Some((_, best_score)) = &best {
                if score < *best_score
                    || (float_eq(score, *best_score)
                        && lane
                            .name
                            .to_ascii_lowercase()
                            .cmp(&best.as_ref().unwrap().0.lane_name.to_ascii_lowercase())
                            .is_lt())
                {
                    best = Some((
                        PlacementDecision {
                            lane: lane.lane,
                            lane_name: lane.name.clone(),
                            score,
                            utilization: sample.utilization,
                            temperature_c: sample.temperature_c,
                        },
                        score,
                    ));
                }
            } else {
                best = Some((
                    PlacementDecision {
                        lane: lane.lane,
                        lane_name: lane.name.clone(),
                        score,
                        utilization: sample.utilization,
                        temperature_c: sample.temperature_c,
                    },
                    score,
                ));
            }
        }

        best.map(|(decision, _)| decision)
    }

    fn tick_cooldowns(&mut self) {
        self.cooldowns.retain(|_, v| {
            if *v > 0 {
                *v -= 1;
            }
            *v > 0
        });
    }

    fn score_lane(&mut self, kind: DeviceKind, sample: DeviceSample) -> f32 {
        let weights = self.cfg.weights;
        let (lat_base, energy_base, thermal_base) = lane_constants(kind);

        let util = sample.utilization.max(0.0);
        let temp_penalty = sample
            .temperature_c
            .filter(|t| *t > self.cfg.thermal_ceiling_c)
            .map(|t| {
                self.cooldowns.insert(kind, self.cfg.cooldown_steps);
                (t - self.cfg.thermal_ceiling_c) * 0.05
            })
            .unwrap_or(0.0);

        let cooldown_penalty = self
            .cooldowns
            .get(&kind)
            .map(|steps| 5.0 + (*steps as f32 * 0.5))
            .unwrap_or(0.0);

        let energy_penalty = sample.power_w.map(|w| w * 0.02).unwrap_or(util * 0.1);

        let lat_term = weights.latency * quantize(lat_base + util);
        let energy_term = weights.energy * quantize(energy_base + energy_penalty);
        let thermal_term = weights.thermal * quantize(thermal_base + temp_penalty);

        quantize(lat_term + energy_term + thermal_term + cooldown_penalty)
    }
}

fn float_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

fn sample_for_kind(kind: DeviceKind, snap: &TelemetrySnapshot) -> DeviceSample {
    match kind {
        DeviceKind::Cpu => snap.cpu,
        DeviceKind::Gpu => snap.gpu,
        DeviceKind::Ane => snap.ane,
    }
}

fn lane_constants(kind: DeviceKind) -> (f32, f32, f32) {
    match kind {
        DeviceKind::Cpu => (1.8, 1.2, 1.1),
        DeviceKind::Gpu => (1.0, 1.0, 1.0),
        DeviceKind::Ane => (0.85, 0.7, 0.9),
    }
}

fn telemetry_empty(snap: &TelemetrySnapshot) -> bool {
    snap.cpu.utilization == 0.0
        && snap.gpu.utilization == 0.0
        && snap.ane.utilization == 0.0
        && snap.cpu.temperature_c.is_none()
        && snap.gpu.temperature_c.is_none()
        && snap.ane.temperature_c.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_lower_cost_lane_deterministically() {
        let cfg = PlacementConfig::default();
        let mut engine = PlacementEngine::new(cfg);

        let lanes = vec![
            LaneDescriptor {
                lane: BackendLane::Primary,
                kind: DeviceKind::Gpu,
                name: "metal".to_string(),
            },
            LaneDescriptor {
                lane: BackendLane::Fallback,
                kind: DeviceKind::Ane,
                name: "coreml-ane".to_string(),
            },
        ];

        let snap = TelemetrySnapshot {
            cpu: DeviceSample::default(),
            gpu: DeviceSample {
                utilization: 0.9,
                temperature_c: Some(80.0),
                power_w: None,
            },
            ane: DeviceSample {
                utilization: 0.1,
                temperature_c: Some(55.0),
                power_w: None,
            },
            captured_at: Instant::now(),
        };

        let decision = engine.choose_lane(&lanes, &snap).expect("decision");
        assert_eq!(decision.lane, BackendLane::Fallback);
        assert_eq!(decision.lane_name, "coreml-ane".to_string());
    }
}
