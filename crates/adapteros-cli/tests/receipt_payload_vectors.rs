use adapteros_core::receipt_digest::{
    canonical_json_string, ReceiptDigestInput, RECEIPT_SCHEMA_V7,
};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn read_bytes(path: &PathBuf) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn read_string(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn cli_uses_shared_receipt_payload_verifier_vectors_v7() {
    let dir = repo_root().join("docs/receipt_test_vectors/v7");
    for name in ["minimal", "typical", "citations_equipment"] {
        let payload = read_bytes(&dir.join(format!("{name}.input.json")));
        let payload_obj: ReceiptDigestInput = serde_json::from_slice(&payload)
            .unwrap_or_else(|e| panic!("{name}: parse payload json: {e}"));
        let canonical_payload = canonical_json_string(&payload_obj)
            .unwrap_or_else(|e| panic!("{name}: canonicalize payload json: {e}"));
        let expected = read_string(&dir.join(format!("{name}.expected_receipt_digest_hex.txt")));

        let res = adapteros_crypto::verify_receipt_payload_bytes(
            canonical_payload.as_bytes(),
            expected.trim(),
            RECEIPT_SCHEMA_V7,
        );
        assert!(res.pass, "{name}: expected pass, got {:?}", res.reasons);
        assert!(res.reasons.is_empty(), "{name}: reasons must be empty");
    }
}
