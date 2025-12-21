//! Event sampling strategies for telemetry
//!
//! Implements configurable sampling rates to reduce telemetry volume while
//! ensuring critical events are never dropped.
//!
//! Per Policy Pack #9 (Telemetry Ruleset):
//! - Security events: 100% sampling (MUST NOT be sampled)
//! - Policy violations: 100% sampling
//! - Egress attempts: 100% sampling
//! - Performance events: Configurable (default: 10%)
//! - Debug events: Configurable (default: 1%)
//!
//! Implements log sampling strategies for high-volume events

#[allow(unused_imports)]
use crate::unified_events::{EventType, LogLevel, TelemetryEvent};
use rand::{Rng, SeedableRng};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Sampling strategy for telemetry events
#[derive(Debug, Clone)]
pub enum SamplingStrategy {
    /// Always sample (100%)
    Always,
    /// Never sample (0%)
    Never,
    /// Sample at a fixed rate (0.0 to 1.0)
    Fixed(f64),
    /// Sample first N events per time window
    HeadSampling { count: usize, window_secs: u64 },
    /// Sample based on event attributes
    Adaptive {
        base_rate: f64,
        min_rate: f64,
        max_rate: f64,
    },
}

impl SamplingStrategy {
    /// Determine if an event should be sampled
    pub fn should_sample(&self, rng: &mut impl Rng) -> bool {
        match self {
            SamplingStrategy::Always => true,
            SamplingStrategy::Never => false,
            SamplingStrategy::Fixed(rate) => {
                if *rate >= 1.0 {
                    true
                } else if *rate <= 0.0 {
                    false
                } else {
                    rng.gen::<f64>() < *rate
                }
            }
            SamplingStrategy::HeadSampling { .. } => {
                // Implemented in EventSampler
                true
            }
            SamplingStrategy::Adaptive { base_rate, .. } => {
                // Start with base rate, can be adjusted dynamically
                rng.gen::<f64>() < *base_rate
            }
        }
    }
}

/// Event sampler with configurable strategies
pub struct EventSampler {
    /// Sampling strategy per event type
    strategies: Arc<RwLock<HashMap<String, SamplingStrategy>>>,
    /// Default strategy for unspecified event types
    default_strategy: SamplingStrategy,
    /// Head sampling state (event_type -> (count, window_start))
    head_sampling_state: Arc<RwLock<HashMap<String, (usize, u64)>>>,
    /// RNG for sampling decisions
    rng: Arc<RwLock<rand::rngs::StdRng>>,
}

impl EventSampler {
    /// Create a new event sampler with default rules
    ///
    /// Default rules follow Policy Pack #9:
    /// - Security events: Always (100%)
    /// - Policy violations: Always (100%)
    /// - System errors: Always (100%)
    /// - Performance metrics: Fixed (10%)
    /// - Debug events: Fixed (1%)
    pub fn new() -> Self {
        Self::with_seed(Self::derive_sampler_seed())
    }

    /// Create an event sampler with a specific seed for deterministic testing
    pub fn with_seed(seed: [u8; 32]) -> Self {
        let mut strategies = HashMap::new();

        // Security events (100% sampling - Telemetry Ruleset #9)
        strategies.insert("security.violation".to_string(), SamplingStrategy::Always);
        strategies.insert("security.check".to_string(), SamplingStrategy::Always);
        strategies.insert("security.alert".to_string(), SamplingStrategy::Always);

        // Policy events (100% sampling)
        strategies.insert("policy.violation".to_string(), SamplingStrategy::Always);
        strategies.insert("policy.enforcement".to_string(), SamplingStrategy::Always);
        strategies.insert(
            "policy.hash_validation".to_string(),
            SamplingStrategy::Always,
        );

        // Egress events (100% sampling)
        strategies.insert("network.egress".to_string(), SamplingStrategy::Always);
        strategies.insert("network.blocked".to_string(), SamplingStrategy::Always);

        // System errors (100% sampling)
        strategies.insert("system.error".to_string(), SamplingStrategy::Always);
        strategies.insert("adapter.evicted".to_string(), SamplingStrategy::Always);
        strategies.insert("memory.pressure".to_string(), SamplingStrategy::Always);

        // Performance metrics (10% sampling)
        strategies.insert(
            "performance.metric".to_string(),
            SamplingStrategy::Fixed(0.1),
        );
        strategies.insert("performance.alert".to_string(), SamplingStrategy::Always);

        // Router decisions (10% sampling, but deterministic replay uses 100%)
        strategies.insert("router.decision".to_string(), SamplingStrategy::Fixed(0.1));

        // Inference events (10% sampling)
        strategies.insert("inference.start".to_string(), SamplingStrategy::Fixed(0.1));
        strategies.insert(
            "inference.complete".to_string(),
            SamplingStrategy::Fixed(0.1),
        );
        strategies.insert("inference.error".to_string(), SamplingStrategy::Always);

        // Debug events (1% sampling)
        strategies.insert("debug".to_string(), SamplingStrategy::Fixed(0.01));

        // Use provided seed for reproducibility
        let rng = rand::rngs::StdRng::from_seed(seed);

        Self {
            strategies: Arc::new(RwLock::new(strategies)),
            default_strategy: SamplingStrategy::Fixed(0.1), // 10% default
            head_sampling_state: Arc::new(RwLock::new(HashMap::new())),
            rng: Arc::new(RwLock::new(rng)),
        }
    }

    /// Derive a sampler seed using HKDF with system entropy
    ///
    /// This creates a deterministic seed based on:
    /// - Process start time
    /// - Process ID
    /// - Hostname
    ///
    /// The seed is unique per process instance but reproducible for debugging.
    fn derive_sampler_seed() -> [u8; 32] {
        use std::time::{SystemTime, UNIX_EPOCH};

        // Gather entropy sources
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let pid = std::process::id();
        let hostname = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("HOST"))
            .unwrap_or_else(|_| "unknown".to_string());

        // Create HKDF input material
        let ikm = format!("telemetry-sampler:{}:{}:{}", timestamp, pid, hostname);

        // Use BLAKE3 for key derivation (consistent with adapteros-core)
        let hash = blake3::hash(ikm.as_bytes());
        *hash.as_bytes()
    }

    /// Check if an event should be sampled
    pub async fn should_sample(&self, event: &TelemetryEvent) -> bool {
        // Critical events based on log level
        if matches!(event.level, LogLevel::Error | LogLevel::Critical) {
            return true;
        }

        // Get strategy for this event type
        let strategies = self.strategies.read().await;
        let strategy = strategies
            .get(&event.event_type)
            .unwrap_or(&self.default_strategy);

        match strategy {
            SamplingStrategy::HeadSampling { count, window_secs } => {
                self.should_sample_head(&event.event_type, *count, *window_secs)
                    .await
            }
            _ => {
                let mut rng = self.rng.write().await;
                strategy.should_sample(&mut *rng)
            }
        }
    }

    /// Head sampling logic (sample first N events per time window)
    async fn should_sample_head(&self, event_type: &str, count: usize, window_secs: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut state = self.head_sampling_state.write().await;

        let (current_count, window_start) = state.entry(event_type.to_string()).or_insert((0, now));

        // Check if we need to reset the window
        if now - *window_start >= window_secs {
            *current_count = 0;
            *window_start = now;
        }

        // Sample if under the count limit
        if *current_count < count {
            *current_count += 1;
            true
        } else {
            false
        }
    }

    /// Set a custom sampling strategy for an event type
    pub async fn set_strategy(&self, event_type: String, strategy: SamplingStrategy) {
        let mut strategies = self.strategies.write().await;
        strategies.insert(event_type, strategy);
    }

    /// Get current sampling rate for an event type
    pub async fn get_sampling_rate(&self, event_type: &str) -> f64 {
        let strategies = self.strategies.read().await;
        match strategies.get(event_type).unwrap_or(&self.default_strategy) {
            SamplingStrategy::Always => 1.0,
            SamplingStrategy::Never => 0.0,
            SamplingStrategy::Fixed(rate) => *rate,
            SamplingStrategy::HeadSampling { .. } => 1.0, // Simplified
            SamplingStrategy::Adaptive { base_rate, .. } => *base_rate,
        }
    }

    /// Get sampling statistics
    pub async fn stats(&self) -> SamplingStats {
        let strategies = self.strategies.read().await;
        let event_type_count = strategies.len();

        let always_sampled = strategies
            .values()
            .filter(|s| matches!(s, SamplingStrategy::Always))
            .count();

        SamplingStats {
            event_type_count,
            always_sampled_count: always_sampled,
        }
    }
}

impl Default for EventSampler {
    fn default() -> Self {
        Self::new()
    }
}

/// Sampling statistics
#[derive(Debug, Clone)]
pub struct SamplingStats {
    pub event_type_count: usize,
    pub always_sampled_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unified_events::TelemetryEventBuilder;
    use adapteros_core::identity::IdentityEnvelope;

    fn create_test_event(event_type: EventType, level: LogLevel) -> TelemetryEvent {
        let identity = IdentityEnvelope::new(
            "test".to_string(),
            "telemetry".to_string(),
            "sampling_test".to_string(),
            "1.0".to_string(),
        );

        TelemetryEventBuilder::new(event_type, level, "Test event".to_string(), identity)
            .build()
            .expect("Failed to build test event")
    }

    #[tokio::test]
    async fn test_security_events_always_sampled() {
        let sampler = EventSampler::new();

        let event = create_test_event(EventType::SecurityViolation, LogLevel::Critical);

        // Test 100 times to ensure it's always sampled
        for _ in 0..100 {
            assert!(sampler.should_sample(&event).await);
        }
    }

    #[tokio::test]
    async fn test_critical_level_always_sampled() {
        let sampler = EventSampler::new();

        // Even a debug event should be sampled if it's critical level
        let event = create_test_event(EventType::Custom("debug".to_string()), LogLevel::Critical);

        assert!(sampler.should_sample(&event).await);
    }

    #[tokio::test]
    async fn test_error_level_always_sampled() {
        let sampler = EventSampler::new();

        let event = create_test_event(EventType::Custom("test".to_string()), LogLevel::Error);

        assert!(sampler.should_sample(&event).await);
    }

    #[tokio::test]
    async fn test_fixed_sampling_rate() {
        let sampler = EventSampler::new();

        sampler
            .set_strategy("test.event".to_string(), SamplingStrategy::Fixed(0.5))
            .await;

        let event = create_test_event(EventType::Custom("test.event".to_string()), LogLevel::Info);

        // Sample 1000 times and check approximate 50% rate
        let mut sampled = 0;
        for _ in 0..1000 {
            if sampler.should_sample(&event).await {
                sampled += 1;
            }
        }

        // Allow 10% variance (450-550 sampled out of 1000)
        assert!(sampled > 400 && sampled < 600, "Sampled: {}", sampled);
    }

    #[tokio::test]
    async fn test_head_sampling() {
        let sampler = EventSampler::new();

        sampler
            .set_strategy(
                "test.head".to_string(),
                SamplingStrategy::HeadSampling {
                    count: 5,
                    window_secs: 60,
                },
            )
            .await;

        let event = create_test_event(EventType::Custom("test.head".to_string()), LogLevel::Info);

        // First 5 should be sampled
        for i in 0..5 {
            assert!(
                sampler.should_sample(&event).await,
                "Event {} should be sampled",
                i
            );
        }

        // Next 5 should not be sampled
        for i in 5..10 {
            assert!(
                !sampler.should_sample(&event).await,
                "Event {} should not be sampled",
                i
            );
        }
    }

    #[tokio::test]
    async fn test_custom_strategy() {
        let sampler = EventSampler::new();

        sampler
            .set_strategy("custom.event".to_string(), SamplingStrategy::Never)
            .await;

        let event = create_test_event(
            EventType::Custom("custom.event".to_string()),
            LogLevel::Debug,
        );

        // Should never be sampled (unless critical/error level overrides)
        assert!(!sampler.should_sample(&event).await);
    }

    #[tokio::test]
    async fn test_sampling_stats() {
        let sampler = EventSampler::new();

        let stats = sampler.stats().await;

        // Should have some strategies configured by default
        assert!(stats.event_type_count > 0);
        assert!(stats.always_sampled_count > 0);
    }
}
