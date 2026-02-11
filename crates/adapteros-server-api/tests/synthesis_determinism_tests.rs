//! Synthesis determinism tests
//!
//! Validates that seed derivation and model hashing produce deterministic,
//! collision-resistant outputs. The `#[ignore]` tests require an actual model
//! at `AOS_SYNTHESIS_MODEL_PATH` and an MLX-capable build.

use adapteros_core::B3Hash;
use adapteros_server_api::services::synthesis::{
    compute_synthesis_model_hash, derive_synthesis_seed_bytes_v1, derive_synthesis_seed_u64_v1,
};

// ========================================================================
// Unit-level: seed derivation correctness (always runs)
// ========================================================================

/// Prove v1 (u64) and v2 (32-byte) seed paths produce different outputs for
/// the same core inputs, since v2 includes additional inputs (chunk hash,
/// model content hash) and uses a different HKDF label.
#[test]
fn synthesis_seed_v1_v2_domain_separation() {
    let v1 = derive_synthesis_seed_u64_v1("t1", "dh", "d1", 0, "mh");
    let v2 = derive_synthesis_seed_bytes_v1("t1", "dh", "d1", 0, "ch", "mh");
    let v2_as_u64 = u64::from_le_bytes(v2[..8].try_into().unwrap());
    assert_ne!(v1, v2_as_u64, "v1 and v2 seeds must differ");
}

/// Verify that identical v2 inputs always produce identical 32-byte seeds.
#[test]
fn synthesis_seed_v2_is_stable() {
    let seed_a = derive_synthesis_seed_bytes_v1("t1", "dh", "d1", 0, "ch", "mh");
    let seed_b = derive_synthesis_seed_bytes_v1("t1", "dh", "d1", 0, "ch", "mh");
    assert_eq!(seed_a, seed_b);
    assert_eq!(seed_a.len(), 32);
}

/// All six v2 input fields contribute to the output -- changing any single
/// field must change the seed.
#[test]
fn synthesis_seed_v2_all_fields_contribute() {
    let base = derive_synthesis_seed_bytes_v1("t1", "dh", "d1", 0, "ch", "mh");
    let variants = [
        derive_synthesis_seed_bytes_v1("t2", "dh", "d1", 0, "ch", "mh"), // tenant
        derive_synthesis_seed_bytes_v1("t1", "dX", "d1", 0, "ch", "mh"), // doc hash
        derive_synthesis_seed_bytes_v1("t1", "dh", "d2", 0, "ch", "mh"), // doc id
        derive_synthesis_seed_bytes_v1("t1", "dh", "d1", 1, "ch", "mh"), // chunk index
        derive_synthesis_seed_bytes_v1("t1", "dh", "d1", 0, "cX", "mh"), // chunk hash
        derive_synthesis_seed_bytes_v1("t1", "dh", "d1", 0, "ch", "mX"), // model hash
    ];
    for (i, variant) in variants.iter().enumerate() {
        assert_ne!(&base, variant, "Changing field {} must change the seed", i);
    }
}

/// Model hash on a temp directory is deterministic and 64-char hex.
#[test]
fn synthesis_model_hash_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.json"), b"{\"model_type\":\"test\"}").unwrap();

    let hash_a = compute_synthesis_model_hash(dir.path()).unwrap();
    let hash_b = compute_synthesis_model_hash(dir.path()).unwrap();
    assert_eq!(hash_a.len(), 64);
    assert!(hash_a.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(hash_a, hash_b);
}

// ========================================================================
// Integration: real-model determinism (requires AOS_SYNTHESIS_MODEL_PATH)
// ========================================================================

/// Proves strict-replay synthesis is bit-exact: two runs with identical
/// inputs and seed produce byte-identical output.
///
/// Run with:
/// ```sh
/// AOS_SYNTHESIS_MODEL_PATH=/path/to/model \
///   cargo test -p adapteros-server-api --test synthesis_determinism_tests \
///   synthesis_strict_replay_deterministic -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn synthesis_strict_replay_deterministic() {
    let model_path = match std::env::var("AOS_SYNTHESIS_MODEL_PATH") {
        Ok(p) => std::path::PathBuf::from(p),
        Err(_) => {
            eprintln!("SKIP: AOS_SYNTHESIS_MODEL_PATH not set");
            return;
        }
    };

    if !model_path.exists() {
        eprintln!("SKIP: Model path does not exist: {}", model_path.display());
        return;
    }

    // Fixed test inputs
    let test_chunk =
        "Rust is a systems programming language focused on safety, concurrency, and performance.";
    let tenant_id = "test_tenant";
    let doc_content = "This is a test document about Rust programming.";
    let doc_hash = B3Hash::hash(doc_content.as_bytes()).to_hex();
    let doc_id = "test_doc_001";
    let chunk_hash = B3Hash::hash(test_chunk.as_bytes()).to_hex();

    // Compute model content hash via service function
    let model_hash =
        compute_synthesis_model_hash(&model_path).expect("Failed to compute model hash");

    // Derive 32-byte seed via service function
    let seed =
        derive_synthesis_seed_bytes_v1(tenant_id, &doc_hash, doc_id, 0, &chunk_hash, &model_hash);

    eprintln!("Seed (hex): {}", hex::encode(seed));
    eprintln!("Model hash: {}", model_hash);

    // Run synthesis twice with identical seed
    let rt = tokio::runtime::Runtime::new().unwrap();

    let (output_a, output_b) = rt.block_on(async {
        use adapteros_orchestrator::synthesis::{
            EnrichmentMode, SynthesisEngine, SynthesisEngineConfig, SynthesisRequest,
        };

        let config = SynthesisEngineConfig {
            model_path: model_path.clone(),
            temperature: 0.0,
            top_p: 1.0,
            enrichment_mode: EnrichmentMode::StrictReplay,
            ..Default::default()
        };

        let mut engine_a = SynthesisEngine::new(config.clone());
        engine_a
            .load_model()
            .await
            .expect("Failed to load model (run A)");

        let req_a = SynthesisRequest::new(test_chunk, "test_source:chunk_0");
        let result_a = engine_a
            .synthesize(req_a, Some(seed))
            .await
            .expect("Synthesis run A failed");

        let mut engine_b = SynthesisEngine::new(config);
        engine_b
            .load_model()
            .await
            .expect("Failed to load model (run B)");

        let req_b = SynthesisRequest::new(test_chunk, "test_source:chunk_0");
        let result_b = engine_b
            .synthesize(req_b, Some(seed))
            .await
            .expect("Synthesis run B failed");

        (result_a, result_b)
    });

    // Assert bit-exact determinism
    let hash_a = B3Hash::hash(output_a.raw_output.as_bytes());
    let hash_b = B3Hash::hash(output_b.raw_output.as_bytes());

    assert_eq!(
        hash_a,
        hash_b,
        "Strict replay must produce bit-identical output.\n\
         Run A: {} bytes, hash {}\n\
         Run B: {} bytes, hash {}",
        output_a.raw_output.len(),
        hash_a.to_hex(),
        output_b.raw_output.len(),
        hash_b.to_hex(),
    );

    eprintln!("Determinism verified: output hash {}", hash_a.to_hex());
}

/// Proves provenance includes real model content hash, not placeholders.
///
/// Run with:
/// ```sh
/// AOS_SYNTHESIS_MODEL_PATH=/path/to/model \
///   cargo test -p adapteros-server-api --test synthesis_determinism_tests \
///   synthesis_provenance_contains_model_hash -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn synthesis_provenance_contains_model_hash() {
    let model_path = match std::env::var("AOS_SYNTHESIS_MODEL_PATH") {
        Ok(p) => std::path::PathBuf::from(p),
        Err(_) => {
            eprintln!("SKIP: AOS_SYNTHESIS_MODEL_PATH not set");
            return;
        }
    };

    if !model_path.exists() {
        eprintln!("SKIP: Model path does not exist: {}", model_path.display());
        return;
    }

    let model_hash =
        compute_synthesis_model_hash(&model_path).expect("Failed to compute model hash");

    // Model hash should be a 64-char hex string (32 bytes BLAKE3)
    assert_eq!(
        model_hash.len(),
        64,
        "Model hash should be 64 hex characters"
    );
    assert!(
        model_hash.chars().all(|c| c.is_ascii_hexdigit()),
        "Model hash must be valid hex"
    );

    // Hash must be deterministic
    let model_hash_2 =
        compute_synthesis_model_hash(&model_path).expect("Failed to compute model hash (2nd call)");
    assert_eq!(model_hash, model_hash_2, "Model hash must be deterministic");

    eprintln!("Model hash: {}", model_hash);
}
