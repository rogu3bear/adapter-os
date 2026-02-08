use adapteros_core::receipt_digest::{
    canonical_json_string, compute_receipt_digest, ReceiptDigestInput, RECEIPT_SCHEMA_V7,
};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at crates/adapteros-core for this test crate.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn read(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn load_vector(name: &str) -> (ReceiptDigestInput, String, String) {
    let dir = repo_root().join("docs/receipt_test_vectors/v7");
    let json_path = dir.join(format!("{name}.input.json"));
    let expected_path = dir.join(format!("{name}.expected_receipt_digest_hex.txt"));

    let json = read(&json_path);
    let expected_hex = read(&expected_path);

    let input: ReceiptDigestInput =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("parse {name}.input.json: {e}"));

    (input, json, expected_hex)
}

#[test]
fn vectors_v7_serialize_digest_match_expected() {
    for name in ["minimal", "typical", "citations_equipment"] {
        let (input, json, expected_hex) = load_vector(name);

        // 1) deserialize -> canonical serialize stable (locks JSON ordering/format).
        let canonical = canonical_json_string(&input).expect("canonical json");
        assert_eq!(
            json,
            format!("{canonical}\n"),
            "{name}: canonical JSON must match fixture bytes"
        );

        // 2) digest matches expected (locks V7 framing and field inclusion).
        let digest = compute_receipt_digest(&input, RECEIPT_SCHEMA_V7).expect("digest");
        assert_eq!(
            format!("{}\n", digest.to_hex()),
            expected_hex,
            "{name}: receipt digest mismatch"
        );
    }
}

#[test]
fn vectors_v7_any_single_field_mutation_changes_digest() {
    // Minimal: bump logical_prompt_tokens
    {
        let (mut input, _json, expected_hex) = load_vector("minimal");
        let expected_hex = expected_hex.trim();
        let d0 = compute_receipt_digest(&input, RECEIPT_SCHEMA_V7).unwrap();
        assert_eq!(d0.to_hex(), expected_hex, "minimal: sanity");

        input.logical_prompt_tokens += 1;
        let d1 = compute_receipt_digest(&input, RECEIPT_SCHEMA_V7).unwrap();
        assert_ne!(d0, d1, "minimal: mutation must change digest");
    }

    // Typical: change stop_reason_token_index
    {
        let (mut input, _json, expected_hex) = load_vector("typical");
        let expected_hex = expected_hex.trim();
        let d0 = compute_receipt_digest(&input, RECEIPT_SCHEMA_V7).unwrap();
        assert_eq!(d0.to_hex(), expected_hex, "typical: sanity");

        input.stop_reason_token_index = Some(input.stop_reason_token_index.unwrap_or(0) + 1);
        let d1 = compute_receipt_digest(&input, RECEIPT_SCHEMA_V7).unwrap();
        assert_ne!(d0, d1, "typical: mutation must change digest");
    }

    // Citations/equipment: change citation_count
    {
        let (mut input, _json, expected_hex) = load_vector("citations_equipment");
        let expected_hex = expected_hex.trim();
        let d0 = compute_receipt_digest(&input, RECEIPT_SCHEMA_V7).unwrap();
        assert_eq!(d0.to_hex(), expected_hex, "citations_equipment: sanity");

        input.citation_count += 1;
        let d1 = compute_receipt_digest(&input, RECEIPT_SCHEMA_V7).unwrap();
        assert_ne!(d0, d1, "citations_equipment: mutation must change digest");
    }
}
