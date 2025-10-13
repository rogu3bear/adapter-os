#![no_main]

use libfuzzer_sys::fuzz_target;
use adapteros_manifest::ManifestV3;

fuzz_target!(|data: &[u8]| {
    // Try parsing as YAML
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = serde_yaml::from_str::<ManifestV3>(text);
    }

    // Try parsing as JSON
    let _ = serde_json::from_slice::<ManifestV3>(data);
});
