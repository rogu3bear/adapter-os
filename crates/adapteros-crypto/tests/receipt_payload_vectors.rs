use adapteros_core::receipt_digest::RECEIPT_SCHEMA_V7;
use adapteros_crypto::{verify_receipt_payload_bytes, ReceiptVerifyReasonCode};
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
fn receipt_payload_vectors_v7_verify() {
    let dir = repo_root().join("docs/receipt_test_vectors/v7");
    for name in ["minimal", "typical", "citations_equipment"] {
        let payload = read_bytes(&dir.join(format!("{name}.input.json")));
        let expected = read_string(&dir.join(format!("{name}.expected_receipt_digest_hex.txt")));

        let res = verify_receipt_payload_bytes(&payload, expected.trim(), RECEIPT_SCHEMA_V7);
        assert!(
            res.pass,
            "{name}: expected pass, got reasons={:?} parse_error={:?}",
            res.reasons, res.parse_error
        );
        assert!(
            res.reasons.is_empty(),
            "{name}: expected no reasons, got {:?}",
            res.reasons
        );
        assert_eq!(
            res.computed_receipt_digest_hex.as_deref(),
            Some(res.expected_receipt_digest_hex.as_str()),
            "{name}: computed digest must equal expected"
        );
    }
}

#[test]
fn receipt_payload_vectors_v7_mutation_fails() {
    let dir = repo_root().join("docs/receipt_test_vectors/v7");
    let payload = read_bytes(&dir.join("minimal.input.json"));
    let expected = read_string(&dir.join("minimal.expected_receipt_digest_hex.txt"));

    // Mutate one byte in JSON. This should either:
    // - fail parsing (PayloadParseError), or
    // - parse successfully but change digest (ReceiptDigestMismatch), or
    // - be non-canonical (NonCanonicalPayload) while still mismatching digest.
    let mut mutated = payload.clone();
    if let Some(b) = mutated.iter_mut().find(|b| **b == b'7') {
        *b = b'8';
    } else {
        mutated.push(b' ');
    }

    let res = verify_receipt_payload_bytes(&mutated, expected.trim(), RECEIPT_SCHEMA_V7);
    assert!(
        !res.pass,
        "mutation should not verify (expected digest must not match)"
    );
    assert!(
        res.reasons
            .contains(&ReceiptVerifyReasonCode::PayloadParseError)
            || res
                .reasons
                .contains(&ReceiptVerifyReasonCode::ReceiptDigestMismatch),
        "expected parse error or digest mismatch, got {:?}",
        res.reasons
    );
}
