#![no_main]

use libfuzzer_sys::fuzz_target;
use adapteros_sbom::SpdxDocument;

fuzz_target!(|data: &[u8]| {
    // Try parsing as JSON
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = SpdxDocument::from_json(text);
    }
});
