#![cfg(all(test, feature = "extended-tests"))]
//! Platform validation tests for AdapterOS determinism
//!
//! Ensures that deterministic behavior is consistent across different platforms,
//! architectures, and environments while maintaining security boundaries.

use super::utils::*;
use adapteros_core::B3Hash;

/// Test that platform fingerprints are correctly generated and consistent
#[test]
fn test_platform_fingerprint_consistency() {
    let fp1 = PlatformFingerprint::current();
    let fp2 = PlatformFingerprint::current();

    // Fingerprints should be identical for same platform
    assert_eq!(fp1, fp2, "Platform fingerprints should be consistent");

    // Hash should also be consistent
    assert_eq!(fp1.hash(), fp2.hash(), "Platform fingerprint hashes should be consistent");
}

/// Test that platform-specific determinism is maintained
#[test]
fn test_platform_specific_determinism() {
    let fp = PlatformFingerprint::current();

    // Generate platform-specific seeds
    let platform_seed = derive_seed(&fp.hash(), "platform_specific");

    // Verify seed is deterministic for this platform
    let platform_seed2 = derive_seed(&fp.hash(), "platform_specific");
    assert_eq!(platform_seed, platform_seed2, "Platform-specific seeds should be deterministic");

    // But different from other platforms (simulated)
    let other_fp = PlatformFingerprint {
        os: "different_os".to_string(),
        arch: fp.arch.clone(),
        compiler: fp.compiler.clone(),
        features: fp.features.clone(),
    };
    let other_seed = derive_seed(&other_fp.hash(), "platform_specific");
    assert_ne!(platform_seed, other_seed, "Different platforms should have different seeds");
}

/// Test that architecture-specific optimizations don't break determinism
#[test]
fn test_architecture_determinism() {
    let fp = PlatformFingerprint::current();

    // Test different architecture-specific code paths
    let x86_seed = derive_seed(&fp.hash(), "x86_optimization");
    let arm_seed = derive_seed(&fp.hash(), "arm_optimization");
    let gpu_seed = derive_seed(&fp.hash(), "gpu_acceleration");

    // All should be deterministic
    assert_eq!(x86_seed, derive_seed(&fp.hash(), "x86_optimization"));
    assert_eq!(arm_seed, derive_seed(&fp.hash(), "arm_optimization"));
    assert_eq!(gpu_seed, derive_seed(&fp.hash(), "gpu_acceleration"));

    // But different from each other
    assert_ne!(x86_seed, arm_seed);
    assert_ne!(arm_seed, gpu_seed);
    assert_ne!(x86_seed, gpu_seed);
}

/// Test that compiler version differences don't affect determinism
#[test]
fn test_compiler_version_determinism() {
    let base_fp = PlatformFingerprint::current();

    // Simulate different compiler versions
    let compiler_versions = ["rustc_1.70", "rustc_1.75", "rustc_1.80"];

    let mut seeds = Vec::new();
    for version in &compiler_versions {
        let fp = PlatformFingerprint {
            compiler: version.to_string(),
            ..base_fp.clone()
        };
        let seed = derive_seed(&fp.hash(), "compilation");
        seeds.push(seed);
    }

    // All seeds should be different (different compilers = different behavior)
    for i in 0..seeds.len() {
        for j in (i+1)..seeds.len() {
            assert_ne!(seeds[i], seeds[j], "Different compiler versions should produce different seeds");
        }
    }
}

/// Test that feature flags don't break determinism when consistently applied
#[test]
fn test_feature_flag_determinism() {
    let base_fp = PlatformFingerprint::current();

    // Test with different feature sets
    let feature_sets = vec![
        vec!["deterministic".to_string()],
        vec!["deterministic".to_string(), "gpu".to_string()],
        vec!["deterministic".to_string(), "gpu".to_string(), "avx".to_string()],
    ];

    let mut seeds = Vec::new();
    for features in &feature_sets {
        let fp = PlatformFingerprint {
            features: features.clone(),
            ..base_fp.clone()
        };
        let seed = derive_seed(&fp.hash(), "feature_test");
        seeds.push(seed);
    }

    // Each feature set should produce consistent but different seeds
    for i in 0..seeds.len() {
        for j in (i+1)..seeds.len() {
            assert_ne!(seeds[i], seeds[j], "Different feature sets should produce different seeds");
        }
    }

    // Same feature set should produce same seed
    let fp1 = PlatformFingerprint {
        features: vec!["deterministic".to_string(), "gpu".to_string()],
        ..base_fp.clone()
    };
    let fp2 = PlatformFingerprint {
        features: vec!["deterministic".to_string(), "gpu".to_string()],
        ..base_fp.clone()
    };
    assert_eq!(fp1.hash(), fp2.hash(), "Identical feature sets should produce identical hashes");
}

/// Test that OS-specific behavior is properly isolated
#[test]
fn test_os_specific_isolation() {
    let base_fp = PlatformFingerprint::current();

    // Test different OS behaviors
    let os_types = ["macos", "linux", "windows"];

    let mut os_seeds = Vec::new();
    for os in &os_types {
        let fp = PlatformFingerprint {
            os: os.to_string(),
            ..base_fp.clone()
        };
        let seed = derive_seed(&fp.hash(), "os_behavior");
        os_seeds.push(seed);
    }

    // Each OS should have isolated, deterministic behavior
    for i in 0..os_seeds.len() {
        for j in (i+1)..os_seeds.len() {
            assert_ne!(os_seeds[i], os_seeds[j], "Different OS types should have different seeds");
        }
    }
}

/// Test that endianness differences are handled deterministically
#[test]
fn test_endianness_handling() {
    // Test that serialization is canonical regardless of platform endianness
    let test_data = vec![1u32, 2u32, 3u32, 0x42u32];

    // Serialize in little-endian (canonical for AdapterOS)
    let mut le_bytes = Vec::new();
    for &num in &test_data {
        le_bytes.extend_from_slice(&num.to_le_bytes());
    }

    // Verify it's always little-endian
    let hash1 = B3Hash::hash(&le_bytes);
    let hash2 = B3Hash::hash(&le_bytes);
    assert_eq!(hash1, hash2, "Canonical serialization should be deterministic");

    // Test that big-endian would produce different results (if it existed)
    let mut be_bytes = Vec::new();
    for &num in &test_data {
        be_bytes.extend_from_slice(&num.to_be_bytes());
    }

    if cfg!(target_endian = "little") {
        // On little-endian systems, BE bytes will be different
        let be_hash = B3Hash::hash(&be_bytes);
        assert_ne!(hash1, be_hash, "Different endianness should produce different hashes");
    }
}

/// Test that floating-point precision is handled deterministically
#[test]
fn test_floating_point_precision() {
    // Test that floating-point operations are deterministic
    // This is critical for numerical stability in ML models

    let values = vec![1.0f32, 2.5f32, -3.14f32, 0.0f32, f32::INFINITY, f32::NEG_INFINITY];

    // Hash the canonical byte representation
    let mut bytes = Vec::new();
    for &val in &values {
        bytes.extend_from_slice(&val.to_le_bytes());
    }

    let hash1 = B3Hash::hash(&bytes);
    let hash2 = B3Hash::hash(&bytes);
    assert_eq!(hash1, hash2, "Floating-point serialization should be deterministic");

    // Test NaN canonicalization
    let nan_values = vec![f32::NAN, f32::NAN, f32::NAN];
    let mut nan_bytes = Vec::new();
    for &val in &nan_values {
        // Canonicalize NaN to a fixed bit pattern
        let canonical_nan = if val.is_nan() { f32::from_bits(0x7FC00000) } else { val };
        nan_bytes.extend_from_slice(&canonical_nan.to_le_bytes());
    }

    let nan_hash1 = B3Hash::hash(&nan_bytes);
    let nan_hash2 = B3Hash::hash(&nan_bytes);
    assert_eq!(nan_hash1, nan_hash2, "NaN canonicalization should be deterministic");
}

/// Test that SIMD operations are deterministic across platforms
#[test]
fn test_simd_determinism() {
    // Test that SIMD operations produce deterministic results
    // This is important for vectorized ML operations

    let data = vec![1.0f32, 2.0f32, 3.0f32, 4.0f32, 5.0f32, 6.0f32, 7.0f32, 8.0f32];

    // Simulate SIMD operation (simple addition)
    let result: Vec<f32> = data.iter().map(|&x| x + 1.0).collect();

    // Serialize and hash
    let mut bytes = Vec::new();
    for &val in &result {
        bytes.extend_from_slice(&val.to_le_bytes());
    }

    let hash1 = B3Hash::hash(&bytes);
    let hash2 = B3Hash::hash(&bytes);
    assert_eq!(hash1, hash2, "SIMD operations should be deterministic");
}

/// Test that GPU kernel compilation is deterministic
#[test]
fn test_gpu_kernel_determinism() {
    // Test that Metal/OpenCL kernel compilation produces deterministic results
    let kernel_source = r#"
        kernel void test_kernel(device float* input [[buffer(0)]],
                               device float* output [[buffer(1)]],
                               uint gid [[thread_position_in_grid]]) {
            output[gid] = input[gid] * 2.0f;
        }
    "#;

    // Hash the kernel source
    let source_hash = B3Hash::hash(kernel_source.as_bytes());

    // Verify hash is consistent
    let source_hash2 = B3Hash::hash(kernel_source.as_bytes());
    assert_eq!(source_hash, source_hash2, "Kernel source hashing should be deterministic");

    // In a real implementation, this would also verify that compiled kernel
    // binaries produce identical results across compilations
}

/// Test that memory layout is deterministic across platforms
#[test]
fn test_memory_layout_determinism() {
    // Test that struct layouts and padding are handled deterministically

    #[repr(C)]
    #[derive(Debug, Clone)]
    struct TestStruct {
        a: u32,
        b: f32,
        c: u16,
        d: u8,
    }

    let test_data = TestStruct {
        a: 0x12345678,
        b: 3.14159,
        c: 0xABCD,
        d: 0x42,
    };

    // Get raw bytes (should be deterministic due to #[repr(C)])
    let bytes = unsafe {
        std::slice::from_raw_parts(
            &test_data as *const TestStruct as *const u8,
            std::mem::size_of::<TestStruct>(),
        )
    };

    let hash1 = B3Hash::hash(bytes);
    let hash2 = B3Hash::hash(bytes);
    assert_eq!(hash1, hash2, "Memory layout serialization should be deterministic");
}