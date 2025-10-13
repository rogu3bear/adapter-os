#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Try parsing as JSON (full policy pack format)
    let _ = serde_json::from_slice::<serde_json::Value>(data);

    // Try parsing policy config from mplora-policy
    if let Ok(text) = std::str::from_utf8(data) {
        // Policy engine parsing
        let _ = serde_json::from_str::<serde_json::Value>(text);
    }
});
