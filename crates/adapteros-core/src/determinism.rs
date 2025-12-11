use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use blake3::Hasher;
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::{B3Hash, SeedMode};

/// Origin of the determinism seed material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeterminismSource {
    /// Seed provided as a 32-byte hex string.
    RequestSeedHex,
    /// Legacy 64-bit seed expanded via HKDF.
    SeedU64Expanded,
    /// Seed derived from live request context.
    DerivedFromRequest,
}

/// Canonical determinism context shared across inference, routing, and replay.
///
/// Carries the request-scoped seed, derived router seed, determinism mode hints,
/// and the routing determinism mode so the router can enforce deterministic
/// tie-breaking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeterminismContext {
    request_seed: [u8; 32],
    request_seed_low64: u64,
    router_seed_hex: String,
    seed_mode: SeedMode,
    routing_mode: RoutingDeterminismMode,
    source: DeterminismSource,
}

impl DeterminismContext {
    /// Build from a request seed, deriving the router seed from manifest hash.
    pub fn new(
        request_seed: [u8; 32],
        manifest_hash: Option<&B3Hash>,
        seed_mode: SeedMode,
        routing_mode: RoutingDeterminismMode,
        source: DeterminismSource,
    ) -> Self {
        let router_seed_hex = derive_router_seed(&request_seed, manifest_hash);
        Self::with_router_seed(
            request_seed,
            router_seed_hex,
            seed_mode,
            routing_mode,
            source,
        )
    }

    /// Build from a request seed with an explicit router seed override.
    pub fn new_with_router_seed(
        request_seed: [u8; 32],
        router_seed_hex: Option<String>,
        manifest_hash: Option<&B3Hash>,
        seed_mode: SeedMode,
        routing_mode: RoutingDeterminismMode,
        source: DeterminismSource,
    ) -> Self {
        let router_seed =
            router_seed_hex.unwrap_or_else(|| derive_router_seed(&request_seed, manifest_hash));
        Self::with_router_seed(request_seed, router_seed, seed_mode, routing_mode, source)
    }

    fn with_router_seed(
        request_seed: [u8; 32],
        router_seed_hex: String,
        seed_mode: SeedMode,
        routing_mode: RoutingDeterminismMode,
        source: DeterminismSource,
    ) -> Self {
        let request_seed_low64 = u64::from_le_bytes(request_seed[..8].try_into().unwrap());
        Self {
            request_seed,
            request_seed_low64,
            router_seed_hex,
            seed_mode,
            routing_mode,
            source,
        }
    }

    /// Get master request seed bytes.
    pub fn request_seed(&self) -> [u8; 32] {
        self.request_seed
    }

    /// Lower 64 bits of the master seed (for API compatibility).
    pub fn request_seed_low64(&self) -> u64 {
        self.request_seed_low64
    }

    /// Router seed as hex string.
    pub fn router_seed_hex(&self) -> &str {
        &self.router_seed_hex
    }

    /// Router seed bytes (zeroed if parsing fails).
    pub fn router_seed_bytes(&self) -> [u8; 32] {
        B3Hash::from_hex(&self.router_seed_hex)
            .map(|h| *h.as_bytes())
            .unwrap_or([0u8; 32])
    }

    /// Routing determinism mode (deterministic/adaptive).
    pub fn routing_mode(&self) -> RoutingDeterminismMode {
        self.routing_mode
    }

    /// Seed mode used when deriving the request seed.
    pub fn seed_mode(&self) -> SeedMode {
        self.seed_mode
    }

    /// Source of the seed material.
    pub fn source(&self) -> &DeterminismSource {
        &self.source
    }

    /// Per-step sampler seed derived from the request seed.
    pub fn sampler_seed(&self, step: u64) -> [u8; 32] {
        derive_sampler_seed(&self.request_seed, step)
    }

    /// Deterministic tie-breaker seed for adaptive routing.
    pub fn router_tiebreak_seed(&self) -> [u8; 32] {
        derive_router_tiebreak_seed(&self.router_seed_hex)
    }
}

/// Derive router seed from request seed and optional manifest hash.
pub fn derive_router_seed(request_seed: &[u8; 32], manifest_hash: Option<&B3Hash>) -> String {
    let mut hasher = Hasher::new();
    hasher.update(b"router");
    hasher.update(request_seed);
    if let Some(hash) = manifest_hash {
        hasher.update(hash.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// Derive sampler seed for a generation step.
pub fn derive_sampler_seed(request_seed: &[u8; 32], step: u64) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(b"sample");
    hasher.update(request_seed);
    hasher.update(&step.to_le_bytes());
    hasher.finalize().as_bytes().to_owned().try_into().unwrap()
}

/// Expand a legacy u64 seed into 32 bytes using HKDF.
pub fn expand_u64_seed(seed: u64) -> [u8; 32] {
    let mut seed_bytes = [0u8; 32];
    seed_bytes[..8].copy_from_slice(&seed.to_le_bytes());
    let hk = Hkdf::<Sha256>::new(None, &seed_bytes[..8]);
    hk.expand(b"replay-seed-expand", &mut seed_bytes[8..])
        .expect("HKDF expand failed");
    seed_bytes
}

/// Derive a deterministic seed for router tie-breaking.
pub fn derive_router_tiebreak_seed(router_seed_hex: &str) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(b"router-tie");
    hasher.update(router_seed_hex.as_bytes());
    hasher.finalize().as_bytes().to_owned().try_into().unwrap()
}
