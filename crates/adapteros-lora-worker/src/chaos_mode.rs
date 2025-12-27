//! Chaos Mode: intentionally injects jitter into layer loading to verify synchronization.
//!
//! Enabled via `AOS_WORKER_CHAOS_MODE=1` (or `true/yes`). Optional
//! `AOS_CHAOS_SEED=<u64>` makes the jitter deterministic for tests.

use adapteros_core::{derive_seed, B3Hash};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tracing::debug;

#[derive(Debug, Clone, Copy)]
struct ChaosConfig {
    enabled: bool,
    min_delay_ms: u64,
    max_delay_ms: u64,
    seed: Option<u64>,
}

impl ChaosConfig {
    fn from_env() -> Self {
        let enabled = std::env::var("AOS_WORKER_CHAOS_MODE")
            .map(|v| {
                v.eq_ignore_ascii_case("1")
                    || v.eq_ignore_ascii_case("true")
                    || v.eq_ignore_ascii_case("yes")
            })
            .unwrap_or(false);

        let seed = std::env::var("AOS_CHAOS_SEED")
            .ok()
            .and_then(|s| s.parse::<u64>().ok());

        Self {
            enabled,
            min_delay_ms: 1,
            max_delay_ms: 50,
            seed,
        }
    }
}

fn chaos_config() -> &'static ChaosConfig {
    static CONFIG: OnceLock<ChaosConfig> = OnceLock::new();
    CONFIG.get_or_init(ChaosConfig::from_env)
}

fn determinism_debug_enabled() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| match std::env::var("AOS_DEBUG_DETERMINISM") {
        Ok(v) => {
            let normalized = v.to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes"
        }
        Err(_) => false,
    })
}

fn seeded_rng(seed: [u8; 32]) -> &'static Mutex<ChaCha8Rng> {
    static RNG: OnceLock<Mutex<ChaCha8Rng>> = OnceLock::new();
    RNG.get_or_init(|| Mutex::new(ChaCha8Rng::from_seed(seed)))
}

fn derived_seed_bytes(cfg: &ChaosConfig) -> [u8; 32] {
    static DERIVED_SEED: OnceLock<[u8; 32]> = OnceLock::new();

    *DERIVED_SEED.get_or_init(|| {
        let base_seed = if let Some(seed_override) = cfg.seed {
            B3Hash::hash(&seed_override.to_le_bytes())
        } else {
            B3Hash::hash(b"adapteros-lora-worker:chaos-mode")
        };

        let derived = derive_seed(&base_seed, "layer-jitter");

        if determinism_debug_enabled() {
            let base_hex = base_seed.to_hex();
            let derived_hex = B3Hash::hash(&derived).to_hex();
            tracing::info!(
                target: "determinism",
                chaos_seed_override = cfg.seed,
                base_seed_prefix = %base_hex.get(..16).unwrap_or(&base_hex),
                derived_prefix = %derived_hex.get(..16).unwrap_or(&derived_hex),
                "Chaos Mode derived seed via HKDF layer-jitter"
            );
        }

        derived
    })
}

fn sample_delay_ms(cfg: &ChaosConfig) -> u64 {
    if !cfg.enabled {
        return 0;
    }

    let seed = derived_seed_bytes(cfg);
    let rng = seeded_rng(seed);
    let mut guard = rng.lock().expect("chaos rng poisoned");
    guard.gen_range(cfg.min_delay_ms..=cfg.max_delay_ms)
}

/// Returns true if Chaos Mode is enabled.
pub fn chaos_mode_enabled() -> bool {
    chaos_config().enabled
}

/// If Chaos Mode is enabled, sleep for a random 1–50ms to mimic jittery layer loads.
pub fn maybe_delay_layer(layer_idx: usize) {
    let cfg = chaos_config();
    if !cfg.enabled {
        return;
    }

    let delay_ms = sample_delay_ms(cfg);
    if delay_ms == 0 {
        return;
    }

    debug!(
        layer = layer_idx,
        delay_ms, "Chaos Mode: injecting layer load jitter"
    );
    std::thread::sleep(Duration::from_millis(delay_ms));
}
