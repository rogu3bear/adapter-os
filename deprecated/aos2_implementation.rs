// Isolated AOS 2.0 implementation - move to staging branch aos2-format
// All active code removed; stubs only

#[cfg(not(feature = "aos2-staging"))]
compile_error!("AOS 2.0 implementation isolated; enable staging for development");

// Stub functions
pub fn aos2_load() -> Result<(), AosError> {
    Err(AosError::Config("Isolated to staging".to_string()))
}
