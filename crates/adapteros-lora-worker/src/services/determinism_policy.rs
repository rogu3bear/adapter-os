use adapteros_core::{AosError, Result};
use ring::hkdf::{PRK, Salt, KDF};
use ring::rand::{SecureRandom, SystemRandom};
use blake3::Hasher;

pub fn seed_rng_hkdf(seed: &[u8]) -> Result<SystemRandom> {
    let rng = SystemRandom::new();
    let salt = Salt::new(ring::aead::AES_256_GCM, seed);
    let prk = PRK::generate(&rng, &salt).map_err(|e| AosError::Crypto(e.to_string()))?;
    // Use prk for deterministic expansion
    Ok(rng) // Placeholder; expand prk for seeded ops
}

pub fn validate_backend_attestation(backend: &str, output: &[u8]) -> Result<()> {
    let expected_hash = blake3::hash(output);
    let backend_hash = match backend {
        "metal" => Hasher::new().update(b"metal").finalize().as_bytes(),
        "mlx" => Hasher::new().update(b"mlx").finalize().as_bytes(),
        _ => return Err(AosError::DeterminismViolation("Unknown backend".to_string())),
    };
    if expected_hash.as_bytes() != backend_hash {
        return Err(AosError::DeterminismViolation("Attestation failed".to_string()));
    }
    Ok(())
}

// Policy enforcement
pub fn enforce_determinism_policy(input: &[u8], output: &[u8]) -> Result<()> {
    let seed = blake3::hash(input);
    seed_rng_hkdf(&seed[..])?;
    validate_backend_attestation("metal", output)?; // Default backend
    Ok(())
}
