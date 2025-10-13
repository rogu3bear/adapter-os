#![no_main]

use adapteros_sbom::SpdxDocument;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Try parsing as JSON
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = SpdxDocument::from_json(text);
    }
});
