use adapteros_core::receipt_digest::RECEIPT_SCHEMA_V7;
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
fn server_uses_shared_receipt_payload_verifier_vectors_v7() {
    let dir = repo_root().join("docs/receipt_test_vectors/v7");
    for name in ["minimal", "typical", "citations_equipment"] {
        let payload = read_bytes(&dir.join(format!("{name}.input.json")));
        let expected = read_string(&dir.join(format!("{name}.expected_receipt_digest_hex.txt")));

        let res = adapteros_crypto::verify_receipt_payload_bytes(
            &payload,
            expected.trim(),
            RECEIPT_SCHEMA_V7,
        );
        assert!(res.pass, "{name}: expected pass, got {:?}", res.reasons);
        assert!(res.reasons.is_empty(), "{name}: reasons must be empty");
    }
}
