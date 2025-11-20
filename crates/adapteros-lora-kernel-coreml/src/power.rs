//! Power-efficient inference strategies for CoreML backend
//!
//! Implements adaptive power management with:
//! - Battery level monitoring
//! - Thermal state tracking
//! - Four power modes (Performance, Balanced, Efficiency, Low Power)
//! - Predictive thermal throttling
//! - Battery-aware scheduling
//! - Power consumption metrics
//!
//! Copyright: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

#[cfg(target_os = "macos")]
use crate::ffi::{get_battery_level, get_is_plugged_in, get_system_low_power_mode, get_thermal_state};

/// Power mode strategy for inference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerMode {
    /// Maximum speed, full ANE utilization, no throttling
    Performance,
    /// Adaptive ANE/GPU mix based on thermal state
    Balanced,
    /// Maximize battery life, prefer ANE, reduce precision
    Efficiency,
    /// Minimal consumption, defer non-critical tasks
    LowPower,
}

impl Default for PowerMode {
    fn default() -> Self {
        Self::Balanced
    }
}

impl PowerMode {
    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::Performance => "Maximum speed, full ANE",
            Self::Balanced => "Adaptive ANE/GPU mix",
            Self::Efficiency => "Maximize battery life",
            Self::LowPower => "Minimal consumption",
        }
    }

    /// Get maximum concurrent inference operations
    pub fn max_concurrent_operations(&self) -> usize {
        match self {
            Self::Performance => 8,
            Self::Balanced => 4,
            Self::Efficiency => 2,
            Self::LowPower => 1,
        }
    }

    /// Get batch timeout (defer operations when not plugged in)
    pub fn batch_timeout_ms(&self) -> Option<u64> {
        match self {
            Self::Performance => None, // No batching
            Self::Balanced => Some(50),
            Self::Efficiency => Some(100),
            Self::LowPower => Some(500),
        }
    }

    /// Check if reduced precision is allowed
    pub fn allow_reduced_precision(&self) -> bool {
        matches!(self, Self::Efficiency | Self::LowPower)
    }
}

/// Thermal state of the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThermalState {
    /// Normal operation
    Nominal,
    /// Temperature rising, no action needed
    Fair,
    /// Temperature high, consider throttling
    Serious,
    /// Critical temperature, must throttle
    Critical,
}

impl Default for ThermalState {
    fn default() -> Self {
        Self::Nominal
    }
}

impl ThermalState {
    /// Get throttle multiplier (0.0 = full throttle, 1.0 = no throttle)
    pub fn throttle_multiplier(&self) -> f32 {
        match self {
            Self::Nominal => 1.0,
            Self::Fair => 0.9,
            Self::Serious => 0.7,
            Self::Critical => 0.5,
        }
    }

    /// Check if throttling is required
    pub fn requires_throttling(&self) -> bool {
        matches!(self, Self::Serious | Self::Critical)
    }
}

/// Battery state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryState {
    /// Battery level percentage (0-100)
    pub level_percent: f32,
    /// Plugged into power source
    pub is_plugged_in: bool,
    /// System is in low power mode
    pub low_power_mode: bool,
    /// Timestamp of last update
    pub last_update: Instant,
}

impl Default for BatteryState {
    fn default() -> Self {
        Self {
            level_percent: 100.0,
            is_plugged_in: true,
            low_power_mode: false,
            last_update: Instant::now(),
        }
    }
}

impl BatteryState {
    /// Update battery state from system
    #[cfg(target_os = "macos")]
    pub fn update(&mut self) -> Result<()> {
        self.level_percent = unsafe { get_battery_level() };
        self.is_plugged_in = unsafe { get_is_plugged_in() != 0 };
        self.low_power_mode = unsafe { get_system_low_power_mode() != 0 };
        self.last_update = Instant::now();

        debug!(
            "Battery state updated: {:.1}%, plugged={}, low_power={}",
            self.level_percent, self.is_plugged_in, self.low_power_mode
        );

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn update(&mut self) -> Result<()> {
        // Stub for non-macOS platforms
        self.last_update = Instant::now();
        Ok(())
    }

    /// Check if battery is low (< 20%)
    pub fn is_low(&self) -> bool {
        !self.is_plugged_in && self.level_percent < 20.0
    }

    /// Check if battery is critical (< 10%)
    pub fn is_critical(&self) -> bool {
        !self.is_plugged_in && self.level_percent < 10.0
    }
}

/// Power consumption metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PowerMetrics {
    /// Total energy consumed (estimated mWh)
    pub total_energy_mwh: f64,
    /// Total tokens processed
    pub total_tokens: u64,
    /// Total inference operations
    pub total_operations: u64,
    /// Average watts per token
    pub avg_watts_per_token: f32,
    /// Average energy per inference (mWh)
    pub avg_energy_per_inference: f32,
    /// Estimated battery drain rate (% per hour)
    pub battery_drain_rate_pct_per_hour: f32,
    /// Thermal overhead events
    pub thermal_throttle_events: u64,
}

impl PowerMetrics {
    /// Record inference operation
    pub fn record_inference(&mut self, tokens: u64, energy_mwh: f64) {
        self.total_tokens += tokens;
        self.total_operations += 1;
        self.total_energy_mwh += energy_mwh;

        // Update averages
        if self.total_tokens > 0 {
            self.avg_watts_per_token = (self.total_energy_mwh / self.total_tokens as f64) as f32;
        }
        if self.total_operations > 0 {
            self.avg_energy_per_inference =
                (self.total_energy_mwh / self.total_operations as f64) as f32;
        }
    }

    /// Record thermal throttle event
    pub fn record_throttle_event(&mut self) {
        self.thermal_throttle_events += 1;
    }

    /// Update battery drain rate
    pub fn update_drain_rate(&mut self, battery_delta_pct: f32, elapsed_hours: f32) {
        if elapsed_hours > 0.0 {
            self.battery_drain_rate_pct_per_hour = battery_delta_pct / elapsed_hours;
        }
    }
}

/// Thermal history for predictive throttling
#[derive(Debug)]
struct ThermalHistory {
    states: VecDeque<(Instant, ThermalState)>,
    max_size: usize,
}

impl ThermalHistory {
    fn new(max_size: usize) -> Self {
        Self {
            states: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    fn push(&mut self, state: ThermalState) {
        let now = Instant::now();
        if self.states.len() >= self.max_size {
            self.states.pop_front();
        }
        self.states.push_back((now, state));
    }

    /// Predict if thermal throttling is imminent
    fn predict_throttle_needed(&self, window_secs: u64) -> bool {
        if self.states.len() < 2 {
            return false;
        }

        let now = Instant::now();
        let window = Duration::from_secs(window_secs);

        // Count recent "hot" states
        let hot_count = self
            .states
            .iter()
            .rev()
            .take_while(|(ts, _)| now.duration_since(*ts) < window)
            .filter(|(_, state)| matches!(state, ThermalState::Serious | ThermalState::Critical))
            .count();

        // If >50% of recent samples are hot, predict throttling needed
        let recent_count = self
            .states
            .iter()
            .rev()
            .take_while(|(ts, _)| now.duration_since(*ts) < window)
            .count();

        hot_count > recent_count / 2
    }
}

/// Power manager for CoreML backend
pub struct PowerManager {
    /// Current power mode
    mode: Arc<Mutex<PowerMode>>,
    /// Battery state
    battery: Arc<Mutex<BatteryState>>,
    /// Current thermal state
    thermal_state: Arc<Mutex<ThermalState>>,
    /// Thermal history for prediction
    thermal_history: Arc<Mutex<ThermalHistory>>,
    /// Power consumption metrics
    metrics: Arc<Mutex<PowerMetrics>>,
    /// Shutdown signal
    shutdown: Arc<AtomicBool>,
    /// Last monitoring update
    last_monitor_update: Arc<Mutex<Instant>>,
    /// Monitoring interval (seconds)
    monitor_interval_secs: u64,
    /// Inference operation counter
    operation_counter: Arc<AtomicU64>,
}

impl PowerManager {
    /// Create a new power manager
    pub fn new(mode: PowerMode, monitor_interval_secs: u64) -> Self {
        Self {
            mode: Arc::new(Mutex::new(mode)),
            battery: Arc::new(Mutex::new(BatteryState::default())),
            thermal_state: Arc::new(Mutex::new(ThermalState::default())),
            thermal_history: Arc::new(Mutex::new(ThermalHistory::new(60))), // 60 samples
            metrics: Arc::new(Mutex::new(PowerMetrics::default())),
            shutdown: Arc::new(AtomicBool::new(false)),
            last_monitor_update: Arc::new(Mutex::new(Instant::now())),
            monitor_interval_secs,
            operation_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get current power mode
    pub fn get_mode(&self) -> PowerMode {
        *self.mode.lock().unwrap()
    }

    /// Set power mode
    pub fn set_mode(&self, mode: PowerMode) {
        let mut current = self.mode.lock().unwrap();
        if *current != mode {
            info!("Power mode changed: {:?} -> {:?}", *current, mode);
            *current = mode;
        }
    }

    /// Get battery state
    pub fn get_battery_state(&self) -> BatteryState {
        self.battery.lock().unwrap().clone()
    }

    /// Get thermal state
    pub fn get_thermal_state(&self) -> ThermalState {
        *self.thermal_state.lock().unwrap()
    }

    /// Get power metrics
    pub fn get_metrics(&self) -> PowerMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// Update system state (battery, thermal)
    pub fn update_system_state(&self) -> Result<()> {
        // Update battery
        {
            let mut battery = self.battery.lock().unwrap();
            battery.update()?;

            // Auto-switch to low power mode if battery critical
            if battery.is_critical() || battery.low_power_mode {
                let mut mode = self.mode.lock().unwrap();
                if *mode != PowerMode::LowPower {
                    warn!("Battery critical or low power mode enabled, switching to LowPower mode");
                    *mode = PowerMode::LowPower;
                }
            }
        }

        // Update thermal state
        #[cfg(target_os = "macos")]
        {
            let thermal_value = unsafe { get_thermal_state() };
            let new_state = match thermal_value {
                0 => ThermalState::Nominal,
                1 => ThermalState::Fair,
                2 => ThermalState::Serious,
                _ => ThermalState::Critical,
            };

            let mut thermal = self.thermal_state.lock().unwrap();
            if *thermal != new_state {
                info!("Thermal state changed: {:?} -> {:?}", *thermal, new_state);
                *thermal = new_state;
            }

            // Update thermal history
            let mut history = self.thermal_history.lock().unwrap();
            history.push(new_state);

            // Record throttle event if needed
            if new_state.requires_throttling() {
                let mut metrics = self.metrics.lock().unwrap();
                metrics.record_throttle_event();
            }
        }

        Ok(())
    }

    /// Check if monitoring update is needed
    pub fn needs_update(&self) -> bool {
        let last_update = self.last_monitor_update.lock().unwrap();
        last_update.elapsed().as_secs() >= self.monitor_interval_secs
    }

    /// Perform periodic monitoring update
    pub fn periodic_update(&self) -> Result<()> {
        if !self.needs_update() {
            return Ok(());
        }

        self.update_system_state()?;

        // Update timestamp
        *self.last_monitor_update.lock().unwrap() = Instant::now();

        Ok(())
    }

    /// Check if operation should be deferred (battery-aware scheduling)
    pub fn should_defer_operation(&self, is_critical: bool) -> bool {
        if is_critical {
            return false; // Never defer critical operations
        }

        let mode = self.get_mode();
        let battery = self.get_battery_state();

        match mode {
            PowerMode::Performance => false,
            PowerMode::Balanced => {
                // Defer if battery low and not plugged in
                battery.is_low()
            }
            PowerMode::Efficiency | PowerMode::LowPower => {
                // Defer if not plugged in
                !battery.is_plugged_in
            }
        }
    }

    /// Get throttle multiplier based on thermal state
    pub fn get_throttle_multiplier(&self) -> f32 {
        let thermal = self.get_thermal_state();
        thermal.throttle_multiplier()
    }

    /// Predict if thermal throttling is imminent
    pub fn predict_throttle_needed(&self) -> bool {
        let history = self.thermal_history.lock().unwrap();
        history.predict_throttle_needed(30) // 30-second window
    }

    /// Record inference operation for metrics
    pub fn record_inference(&self, tokens: u64, duration: Duration) {
        // Estimate energy consumption (mWh)
        // Rough estimate: ANE ~10W, GPU ~15W, duration in hours
        let mode = self.get_mode();
        let thermal = self.get_thermal_state();

        let base_watts = match mode {
            PowerMode::Performance => 15.0,
            PowerMode::Balanced => 12.0,
            PowerMode::Efficiency => 10.0,
            PowerMode::LowPower => 8.0,
        };

        let thermal_multiplier = thermal.throttle_multiplier();
        let effective_watts = base_watts * thermal_multiplier;

        let duration_hours = duration.as_secs_f64() / 3600.0;
        let energy_mwh = effective_watts * duration_hours * 1000.0; // Convert to mWh

        let mut metrics = self.metrics.lock().unwrap();
        metrics.record_inference(tokens, energy_mwh);

        self.operation_counter.fetch_add(1, Ordering::Relaxed);

        debug!(
            "Inference recorded: {} tokens, {:.3}ms, {:.3}mWh",
            tokens,
            duration.as_millis(),
            energy_mwh
        );
    }

    /// Get adaptive batch size based on power mode
    pub fn get_adaptive_batch_size(&self) -> usize {
        let mode = self.get_mode();
        let thermal = self.get_thermal_state();

        let base_batch = match mode {
            PowerMode::Performance => 8,
            PowerMode::Balanced => 4,
            PowerMode::Efficiency => 2,
            PowerMode::LowPower => 1,
        };

        // Reduce batch size if thermal throttling
        if thermal.requires_throttling() {
            (base_batch / 2).max(1)
        } else {
            base_batch
        }
    }

    /// Check if ANE should be preferred over GPU
    pub fn prefer_ane(&self) -> bool {
        let mode = self.get_mode();
        let battery = self.get_battery_state();

        match mode {
            PowerMode::Performance => false, // Use GPU for max speed
            PowerMode::Balanced => !battery.is_plugged_in, // ANE when on battery
            PowerMode::Efficiency | PowerMode::LowPower => true, // Always prefer ANE
        }
    }

    /// Shutdown monitoring
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    /// Check if shutdown requested
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }
}

impl Default for PowerManager {
    fn default() -> Self {
        Self::new(PowerMode::Balanced, 5) // 5-second monitoring interval
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_mode_defaults() {
        assert_eq!(PowerMode::default(), PowerMode::Balanced);
        assert_eq!(PowerMode::Performance.max_concurrent_operations(), 8);
        assert_eq!(PowerMode::LowPower.max_concurrent_operations(), 1);
    }

    #[test]
    fn test_thermal_state_throttle() {
        assert_eq!(ThermalState::Nominal.throttle_multiplier(), 1.0);
        assert_eq!(ThermalState::Critical.throttle_multiplier(), 0.5);
        assert!(!ThermalState::Nominal.requires_throttling());
        assert!(ThermalState::Critical.requires_throttling());
    }

    #[test]
    fn test_battery_state() {
        let mut battery = BatteryState::default();
        assert!(!battery.is_low());

        battery.level_percent = 15.0;
        battery.is_plugged_in = false;
        assert!(battery.is_low());
        assert!(!battery.is_critical());

        battery.level_percent = 5.0;
        assert!(battery.is_critical());
    }

    #[test]
    fn test_power_metrics() {
        let mut metrics = PowerMetrics::default();
        metrics.record_inference(100, 10.0);
        metrics.record_inference(200, 20.0);

        assert_eq!(metrics.total_tokens, 300);
        assert_eq!(metrics.total_operations, 2);
        assert_eq!(metrics.total_energy_mwh, 30.0);
    }

    #[test]
    fn test_thermal_history_prediction() {
        let mut history = ThermalHistory::new(10);

        // Add nominal states
        for _ in 0..5 {
            history.push(ThermalState::Nominal);
        }
        assert!(!history.predict_throttle_needed(30));

        // Add serious states
        for _ in 0..6 {
            history.push(ThermalState::Serious);
        }
        assert!(history.predict_throttle_needed(30));
    }

    #[test]
    fn test_power_manager() {
        let pm = PowerManager::new(PowerMode::Balanced, 5);

        assert_eq!(pm.get_mode(), PowerMode::Balanced);
        assert!(!pm.should_defer_operation(true)); // Never defer critical

        pm.set_mode(PowerMode::LowPower);
        assert_eq!(pm.get_mode(), PowerMode::LowPower);
    }
}
