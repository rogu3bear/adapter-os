use blake3;
use adapteros_core::AosError;
use tracing::warn;

pub fn verify_hash(weights: &[u8], expected: &[u8]) -> Result<(), AosError> {
    let computed = blake3::hash(weights).as_bytes();
    if computed != expected {
        warn!(expected = %hex::encode(expected), computed = %hex::encode(computed), "Hash verification failed");
        Err(AosError::Kernel("Verification failed: Hash mismatch".to_string()))
    } else {
        Ok(())
    }
}
