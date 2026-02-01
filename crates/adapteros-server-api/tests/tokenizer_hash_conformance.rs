use adapteros_core::B3Hash;
use std::fs;
use std::path::Path;

#[test]
fn fixture_tokenizer_hash_matches_manifest() {
    let tokenizer_path = Path::new("tests/fixtures/models/tiny-test/tokenizer.json");
    let manifest_path =
        Path::new("crates/adapteros-server-api/tests/fixtures/tokenizer_manifest.json");

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(manifest_path).expect("manifest fixture should be readable"),
    )
    .expect("manifest fixture should be valid JSON");

    let expected = manifest["base"]["tokenizer_hash"]
        .as_str()
        .expect("manifest.base.tokenizer_hash must be a string");

    let computed = B3Hash::hash_file(tokenizer_path)
        .expect("hashing tokenizer fixture should succeed")
        .to_hex();

    assert_eq!(
        expected, computed,
        "Fixture tokenizer hash drifted from manifest expectation"
    );
}
