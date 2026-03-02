use adapteros_core::training_receipt_digest::{
    canonical_training_receipt_json_string, compute_training_receipt_digest_v1,
    TrainingReceiptDigestInputV1,
};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn read(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn load_vector(name: &str) -> (TrainingReceiptDigestInputV1, String, String) {
    let dir = repo_root().join("docs/training_receipt_test_vectors/v1");
    let json_path = dir.join(format!("{name}.input.json"));
    let expected_path = dir.join(format!("{name}.expected_training_receipt_digest_hex.txt"));

    let json = read(&json_path);
    let expected_hex = read(&expected_path);

    let input: TrainingReceiptDigestInputV1 =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("parse {name}.input.json: {e}"));

    (input, json, expected_hex)
}

#[test]
fn vectors_v1_serialize_digest_match_expected() {
    for name in ["minimal", "typical", "reordered_phases", "multi_phase"] {
        let (input, json, expected_hex) = load_vector(name);

        let canonical = canonical_training_receipt_json_string(&input).expect("canonical json");
        assert_eq!(
            json,
            format!("{canonical}\n"),
            "{name}: canonical JSON must match fixture bytes"
        );

        let digest = compute_training_receipt_digest_v1(&input);
        assert_eq!(
            format!("{}\n", digest.to_hex()),
            expected_hex,
            "{name}: receipt digest mismatch"
        );
    }
}

#[test]
fn vectors_v1_any_single_field_mutation_changes_digest() {
    {
        let (mut input, _json, expected_hex) = load_vector("minimal");
        let expected_hex = expected_hex.trim();
        let d0 = compute_training_receipt_digest_v1(&input);
        assert_eq!(d0.to_hex(), expected_hex, "minimal: sanity");

        input.dataset_content_hash.push('x');
        let d1 = compute_training_receipt_digest_v1(&input);
        assert_ne!(d0, d1, "minimal: mutation must change digest");
    }

    {
        let (mut input, _json, expected_hex) = load_vector("typical");
        let expected_hex = expected_hex.trim();
        let d0 = compute_training_receipt_digest_v1(&input);
        assert_eq!(d0.to_hex(), expected_hex, "typical: sanity");

        input.phase_statuses[0].outputs_hash.push('x');
        let d1 = compute_training_receipt_digest_v1(&input);
        assert_ne!(d0, d1, "typical: mutation must change digest");
    }

    {
        let (mut input, _json, expected_hex) = load_vector("multi_phase");
        let expected_hex = expected_hex.trim();
        let d0 = compute_training_receipt_digest_v1(&input);
        assert_eq!(d0.to_hex(), expected_hex, "multi_phase: sanity");

        input.training_contract_version = Some("v2".to_string());
        let d1 = compute_training_receipt_digest_v1(&input);
        assert_ne!(d0, d1, "multi_phase: mutation must change digest");
    }
}
