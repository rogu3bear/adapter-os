//! Apple Neural Engine Profiler
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//!
//! Provides comprehensive profiling for ANE utilization, power consumption,
//! and performance characteristics to identify optimization opportunities.

use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// ANE profiler for tracking utilization and performance
#[derive(Debug)]
pub struct ANEProfiler {
    /// Profiling sessions indexed by model ID
    sessions: Arc<Mutex<HashMap<String, ProfilingSession>>>,
    /// Global profiling metrics
    global_metrics: Arc<Mutex<GlobalMetrics>>,
    /// Profiling configuration
    config: ProfilerConfig,
}

/// Configuration for ANE profiler
#[derive(Debug, Clone)]
pub struct ProfilerConfig {
    /// Enable detailed operation-level profiling
    pub detailed_profiling: bool,
    /// Enable power consumption tracking
    pub track_power: bool,
    /// Enable thermal state tracking
    pub track_thermal: bool,
    /// Sampling interval for metrics (milliseconds)
    pub sampling_interval_ms: u64,
    /// Maximum profiling history entries per session
    pub max_history_entries: usize,
}

impl Default for ProfilerConfig {
    fn default() -> Self {
        Self {
            detailed_profiling: true,
            track_power: true,
            track_thermal: true,
            sampling_interval_ms: 100,
            max_history_entries: 1000,
        }
    }
}

/// Profiling session for a single model
#[derive(Debug)]
struct ProfilingSession {
    /// Model identifier
    model_id: String,
    /// Session start time
    start_time: Instant,
    /// Execution history
    executions: Vec<ExecutionProfile>,
    /// Operation fallback tracking
    fallback_ops: HashMap<String, FallbackStats>,
    /// Aggregated statistics
    stats: SessionStats,
}

/// Profile data for a single execution
#[derive(Debug, Clone)]
pub struct ExecutionProfile {
    /// Execution timestamp
    pub timestamp: Instant,
    /// Execution duration (microseconds)
    pub duration_us: u64,
    /// ANE was used for this execution
    pub used_ane: bool,
    /// Compute unit used (ANE, GPU, CPU)
    pub compute_unit: ComputeUnit,
    /// Power consumption estimate (milliwatts)
    pub power_mw: Option<f32>,
    /// Thermal state at execution
    pub thermal_state: ThermalState,
    /// Input tensor shape
    pub input_shape: Vec<usize>,
    /// Output tensor shape
    pub output_shape: Vec<usize>,
    /// Memory bandwidth utilized (GB/s)
    pub memory_bandwidth_gbps: Option<f32>,
}

/// Compute unit used for execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputeUnit {
    /// Apple Neural Engine
    ANE,
    /// GPU (Metal)
    GPU,
    /// CPU fallback
    CPU,
    /// Unknown/mixed
    Unknown,
}

/// Thermal state of device
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalState {
    /// Normal operating temperature
    Nominal,
    /// Elevated temperature (light throttling possible)
    Fair,
    /// High temperature (moderate throttling)
    Serious,
    /// Critical temperature (heavy throttling)
    Critical,
    /// Unknown state
    Unknown,
}

/// Fallback statistics for operations
#[derive(Debug, Clone)]
pub struct FallbackStats {
    /// Operation name
    pub op_name: String,
    /// Total executions
    pub total_executions: u64,
    /// ANE executions
    pub ane_executions: u64,
    /// GPU fallback executions
    pub gpu_fallbacks: u64,
    /// CPU fallback executions
    pub cpu_fallbacks: u64,
    /// Reasons for fallback
    pub fallback_reasons: Vec<FallbackReason>,
}

/// Reason for operation fallback
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackReason {
    /// Operation not supported on ANE
    UnsupportedOperation,
    /// Tensor shape incompatible with ANE
    IncompatibleShape,
    /// Data type not supported on ANE
    UnsupportedDataType,
    /// Model too large for ANE
    ModelTooLarge,
    /// ANE busy or unavailable
    ANEUnavailable,
    /// Thermal throttling
    ThermalThrottling,
    /// Unknown reason
    Unknown,
}

/// Aggregated session statistics
#[derive(Debug, Clone)]
pub struct SessionStats {
    /// Total executions
    pub total_executions: u64,
    /// ANE executions
    pub ane_executions: u64,
    /// GPU fallback executions
    pub gpu_fallbacks: u64,
    /// CPU fallback executions
    pub cpu_fallbacks: u64,
    /// Average execution time (microseconds)
    pub avg_execution_time_us: f32,
    /// Min execution time (microseconds)
    pub min_execution_time_us: u64,
    /// Max execution time (microseconds)
    pub max_execution_time_us: u64,
    /// Average power consumption (milliwatts)
    pub avg_power_mw: Option<f32>,
    /// Peak power consumption (milliwatts)
    pub peak_power_mw: Option<f32>,
    /// ANE utilization percentage
    pub ane_utilization_percent: f32,
    /// Average memory bandwidth (GB/s)
    pub avg_memory_bandwidth_gbps: Option<f32>,
    /// Tokens per second (throughput)
    pub tokens_per_second: f32,
}

impl Default for SessionStats {
    fn default() -> Self {
        Self {
            total_executions: 0,
            ane_executions: 0,
            gpu_fallbacks: 0,
            cpu_fallbacks: 0,
            avg_execution_time_us: 0.0,
            min_execution_time_us: u64::MAX,
            max_execution_time_us: 0,
            avg_power_mw: None,
            peak_power_mw: None,
            ane_utilization_percent: 0.0,
            avg_memory_bandwidth_gbps: None,
            tokens_per_second: 0.0,
        }
    }
}

/// Global profiling metrics across all sessions
#[derive(Debug, Clone, Default)]
pub struct GlobalMetrics {
    /// Total ANE time (microseconds)
    pub total_ane_time_us: u64,
    /// Total GPU fallback time (microseconds)
    pub total_gpu_time_us: u64,
    /// Total CPU fallback time (microseconds)
    pub total_cpu_time_us: u64,
    /// Total energy consumed (millijoules)
    pub total_energy_mj: f32,
    /// Current device thermal state
    pub current_thermal_state: ThermalState,
}

impl ANEProfiler {
    /// Create a new ANE profiler
    pub fn new(config: ProfilerConfig) -> Self {
        info!("ANE profiler initialized with config: {:?}", config);

        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            global_metrics: Arc::new(Mutex::new(GlobalMetrics::default())),
            config,
        }
    }

    /// Start profiling session for a model
    pub fn start_session(&self, model_id: String) -> Result<()> {
        let mut sessions = self.sessions.lock().map_err(|e| {
            AosError::CoreML(format!("Failed to acquire profiler lock: {}", e))
        })?;

        if sessions.contains_key(&model_id) {
            warn!("Profiling session already exists for model: {}", model_id);
            return Ok(());
        }

        sessions.insert(
            model_id.clone(),
            ProfilingSession {
                model_id,
                start_time: Instant::now(),
                executions: Vec::new(),
                fallback_ops: HashMap::new(),
                stats: SessionStats::default(),
            },
        );

        Ok(())
    }

    /// Record execution profile
    pub fn record_execution(
        &self,
        model_id: &str,
        profile: ExecutionProfile,
    ) -> Result<()> {
        let mut sessions = self.sessions.lock().map_err(|e| {
            AosError::CoreML(format!("Failed to acquire profiler lock: {}", e))
        })?;

        let session = sessions.get_mut(model_id).ok_or_else(|| {
            AosError::CoreML(format!("No profiling session for model: {}", model_id))
        })?;

        // Update session statistics
        session.stats.total_executions += 1;

        match profile.compute_unit {
            ComputeUnit::ANE => session.stats.ane_executions += 1,
            ComputeUnit::GPU => session.stats.gpu_fallbacks += 1,
            ComputeUnit::CPU => session.stats.cpu_fallbacks += 1,
            ComputeUnit::Unknown => {}
        }

        // Update timing stats
        if profile.duration_us < session.stats.min_execution_time_us {
            session.stats.min_execution_time_us = profile.duration_us;
        }
        if profile.duration_us > session.stats.max_execution_time_us {
            session.stats.max_execution_time_us = profile.duration_us;
        }

        // Update average execution time
        let total_time = session.stats.avg_execution_time_us
            * (session.stats.total_executions - 1) as f32
            + profile.duration_us as f32;
        session.stats.avg_execution_time_us =
            total_time / session.stats.total_executions as f32;

        // Update power stats
        if let Some(power_mw) = profile.power_mw {
            let current_avg = session.stats.avg_power_mw.unwrap_or(0.0);
            let new_avg = (current_avg * (session.stats.total_executions - 1) as f32
                + power_mw)
                / session.stats.total_executions as f32;
            session.stats.avg_power_mw = Some(new_avg);

            if let Some(peak) = session.stats.peak_power_mw {
                if power_mw > peak {
                    session.stats.peak_power_mw = Some(power_mw);
                }
            } else {
                session.stats.peak_power_mw = Some(power_mw);
            }
        }

        // Update memory bandwidth stats
        if let Some(bandwidth) = profile.memory_bandwidth_gbps {
            let current_avg = session.stats.avg_memory_bandwidth_gbps.unwrap_or(0.0);
            let new_avg = (current_avg * (session.stats.total_executions - 1) as f32
                + bandwidth)
                / session.stats.total_executions as f32;
            session.stats.avg_memory_bandwidth_gbps = Some(new_avg);
        }

        // Calculate ANE utilization
        session.stats.ane_utilization_percent = (session.stats.ane_executions as f32
            / session.stats.total_executions as f32)
            * 100.0;

        // Calculate tokens per second
        if session.stats.avg_execution_time_us > 0.0 {
            session.stats.tokens_per_second =
                1_000_000.0 / session.stats.avg_execution_time_us;
        }

        // Store execution profile
        session.executions.push(profile.clone());

        // Trim history if needed
        if session.executions.len() > self.config.max_history_entries {
            session.executions.remove(0);
        }

        // Update global metrics
        let mut global = self.global_metrics.lock().map_err(|e| {
            AosError::CoreML(format!("Failed to acquire global metrics lock: {}", e))
        })?;

        match profile.compute_unit {
            ComputeUnit::ANE => global.total_ane_time_us += profile.duration_us,
            ComputeUnit::GPU => global.total_gpu_time_us += profile.duration_us,
            ComputeUnit::CPU => global.total_cpu_time_us += profile.duration_us,
            ComputeUnit::Unknown => {}
        }

        if let Some(power_mw) = profile.power_mw {
            let energy_mj = (power_mw * profile.duration_us as f32) / 1000.0;
            global.total_energy_mj += energy_mj;
        }

        global.current_thermal_state = profile.thermal_state;

        debug!(
            "Recorded execution for {}: {:?}μs on {:?}",
            model_id, profile.duration_us, profile.compute_unit
        );

        Ok(())
    }

    /// Record operation fallback
    pub fn record_fallback(
        &self,
        model_id: &str,
        op_name: String,
        compute_unit: ComputeUnit,
        reason: FallbackReason,
    ) -> Result<()> {
        let mut sessions = self.sessions.lock().map_err(|e| {
            AosError::CoreML(format!("Failed to acquire profiler lock: {}", e))
        })?;

        let session = sessions.get_mut(model_id).ok_or_else(|| {
            AosError::CoreML(format!("No profiling session for model: {}", model_id))
        })?;

        let stats = session
            .fallback_ops
            .entry(op_name.clone())
            .or_insert_with(|| FallbackStats {
                op_name: op_name.clone(),
                total_executions: 0,
                ane_executions: 0,
                gpu_fallbacks: 0,
                cpu_fallbacks: 0,
                fallback_reasons: Vec::new(),
            });

        stats.total_executions += 1;

        match compute_unit {
            ComputeUnit::ANE => stats.ane_executions += 1,
            ComputeUnit::GPU => {
                stats.gpu_fallbacks += 1;
                if !stats.fallback_reasons.contains(&reason) {
                    stats.fallback_reasons.push(reason);
                }
            }
            ComputeUnit::CPU => {
                stats.cpu_fallbacks += 1;
                if !stats.fallback_reasons.contains(&reason) {
                    stats.fallback_reasons.push(reason);
                }
            }
            ComputeUnit::Unknown => {}
        }

        Ok(())
    }

    /// Get session statistics
    pub fn get_session_stats(&self, model_id: &str) -> Result<SessionStats> {
        let sessions = self.sessions.lock().map_err(|e| {
            AosError::CoreML(format!("Failed to acquire profiler lock: {}", e))
        })?;

        let session = sessions.get(model_id).ok_or_else(|| {
            AosError::CoreML(format!("No profiling session for model: {}", model_id))
        })?;

        Ok(session.stats.clone())
    }

    /// Get fallback operations report
    pub fn get_fallback_report(&self, model_id: &str) -> Result<Vec<FallbackStats>> {
        let sessions = self.sessions.lock().map_err(|e| {
            AosError::CoreML(format!("Failed to acquire profiler lock: {}", e))
        })?;

        let session = sessions.get(model_id).ok_or_else(|| {
            AosError::CoreML(format!("No profiling session for model: {}", model_id))
        })?;

        Ok(session.fallback_ops.values().cloned().collect())
    }

    /// Get global metrics
    pub fn get_global_metrics(&self) -> Result<GlobalMetrics> {
        let global = self.global_metrics.lock().map_err(|e| {
            AosError::CoreML(format!("Failed to acquire global metrics lock: {}", e))
        })?;

        Ok(global.clone())
    }

    /// Get current thermal state
    pub fn get_thermal_state(&self) -> Result<ThermalState> {
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;

            let output = Command::new("pmset")
                .arg("-g")
                .arg("therm")
                .output()
                .map_err(|e| AosError::CoreML(format!("Failed to get thermal state: {}", e)))?;

            let output_str = String::from_utf8_lossy(&output.stdout);

            // Parse thermal state from pmset output
            let state = if output_str.contains("CPU_Speed_Limit") {
                let limit_line = output_str
                    .lines()
                    .find(|line| line.contains("CPU_Speed_Limit"))
                    .unwrap_or("");

                if limit_line.contains("100") {
                    ThermalState::Nominal
                } else if limit_line.contains("75") {
                    ThermalState::Fair
                } else if limit_line.contains("50") {
                    ThermalState::Serious
                } else {
                    ThermalState::Critical
                }
            } else {
                ThermalState::Nominal
            };

            Ok(state)
        }

        #[cfg(not(target_os = "macos"))]
        {
            Ok(ThermalState::Unknown)
        }
    }

    /// Generate profiling report
    pub fn generate_report(&self, model_id: &str) -> Result<ProfilingReport> {
        let stats = self.get_session_stats(model_id)?;
        let fallbacks = self.get_fallback_report(model_id)?;
        let global = self.get_global_metrics()?;

        Ok(ProfilingReport {
            model_id: model_id.to_string(),
            session_stats: stats,
            fallback_operations: fallbacks,
            global_metrics: global,
            recommendations: self.generate_recommendations(model_id)?,
        })
    }

    /// Generate optimization recommendations
    fn generate_recommendations(&self, model_id: &str) -> Result<Vec<String>> {
        let stats = self.get_session_stats(model_id)?;
        let fallbacks = self.get_fallback_report(model_id)?;
        let mut recommendations = Vec::new();

        // ANE utilization recommendations
        if stats.ane_utilization_percent < 80.0 {
            recommendations.push(format!(
                "ANE utilization is only {:.1}%. Review model operations for ANE compatibility.",
                stats.ane_utilization_percent
            ));
        }

        // Fallback operations recommendations
        for fallback in fallbacks.iter() {
            if fallback.gpu_fallbacks + fallback.cpu_fallbacks > 0 {
                let fallback_rate = ((fallback.gpu_fallbacks + fallback.cpu_fallbacks) as f32
                    / fallback.total_executions as f32)
                    * 100.0;
                recommendations.push(format!(
                    "Operation '{}' has {:.1}% fallback rate. Reasons: {:?}",
                    fallback.op_name, fallback_rate, fallback.fallback_reasons
                ));
            }
        }

        // Performance recommendations
        if let Some(avg_power) = stats.avg_power_mw {
            if avg_power > 5000.0 {
                recommendations.push(format!(
                    "Average power consumption ({:.1}mW) is high. Consider quantization or model pruning.",
                    avg_power
                ));
            }
        }

        // Thermal recommendations
        if stats.ane_utilization_percent > 95.0 {
            recommendations.push(
                "Very high ANE utilization. Monitor thermal throttling.".to_string(),
            );
        }

        Ok(recommendations)
    }
}

/// Complete profiling report
#[derive(Debug, Clone)]
pub struct ProfilingReport {
    /// Model identifier
    pub model_id: String,
    /// Session statistics
    pub session_stats: SessionStats,
    /// Fallback operations
    pub fallback_operations: Vec<FallbackStats>,
    /// Global metrics
    pub global_metrics: GlobalMetrics,
    /// Optimization recommendations
    pub recommendations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profiler_creation() {
        let profiler = ANEProfiler::new(ProfilerConfig::default());
        assert!(profiler.start_session("test_model".to_string()).is_ok());
    }

    #[test]
    fn test_execution_recording() {
        let profiler = ANEProfiler::new(ProfilerConfig::default());
        profiler.start_session("test_model".to_string()).unwrap();

        let profile = ExecutionProfile {
            timestamp: Instant::now(),
            duration_us: 1000,
            used_ane: true,
            compute_unit: ComputeUnit::ANE,
            power_mw: Some(2000.0),
            thermal_state: ThermalState::Nominal,
            input_shape: vec![1, 128],
            output_shape: vec![1, 152064],
            memory_bandwidth_gbps: Some(100.0),
        };

        assert!(profiler.record_execution("test_model", profile).is_ok());

        let stats = profiler.get_session_stats("test_model").unwrap();
        assert_eq!(stats.total_executions, 1);
        assert_eq!(stats.ane_executions, 1);
        assert_eq!(stats.ane_utilization_percent, 100.0);
    }

    #[test]
    fn test_fallback_tracking() {
        let profiler = ANEProfiler::new(ProfilerConfig::default());
        profiler.start_session("test_model".to_string()).unwrap();

        profiler
            .record_fallback(
                "test_model",
                "custom_op".to_string(),
                ComputeUnit::GPU,
                FallbackReason::UnsupportedOperation,
            )
            .unwrap();

        let fallbacks = profiler.get_fallback_report("test_model").unwrap();
        assert_eq!(fallbacks.len(), 1);
        assert_eq!(fallbacks[0].op_name, "custom_op");
        assert_eq!(fallbacks[0].gpu_fallbacks, 1);
    }
}
