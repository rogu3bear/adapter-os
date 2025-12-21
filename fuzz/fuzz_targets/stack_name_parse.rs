#![no_main]

use adapteros_core::StackName;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Try parsing raw bytes as UTF-8 string
    if let Ok(text) = std::str::from_utf8(data) {
        // Attempt to parse as stack name
        // Should not panic on any input
        let _ = StackName::parse(text);

        // Test component extraction if parsing succeeds
        if let Ok(name) = StackName::parse(text) {
            // These should not panic
            let _ = name.namespace();
            let _ = name.identifier();
            let _ = name.to_string();

            // Test validation
            let _ = name.validate();
        }
    }
});
