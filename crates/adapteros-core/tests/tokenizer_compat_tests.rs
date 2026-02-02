use adapteros_core::tokenizer_config::SpecialTokenMap;
use std::path::Path;

#[test]
fn tokenizer_bpe_llama_compat_loads() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tokenizer_bpe_llama_compat.json");
    let meta = SpecialTokenMap::validate_tokenizer(&path, Some(3))
        .expect("Tokenizer compatibility fixture should parse");
    assert_eq!(meta.vocab_size, 3);
}
