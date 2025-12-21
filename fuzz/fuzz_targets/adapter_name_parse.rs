#![no_main]

use adapteros_core::AdapterName;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Try parsing raw bytes as UTF-8 string
    if let Ok(text) = std::str::from_utf8(data) {
        // Attempt to parse as adapter name
        // Should not panic on any input
        let _ = AdapterName::parse(text);

        // Test component extraction if parsing succeeds
        if let Ok(name) = AdapterName::parse(text) {
            // These should not panic
            let _ = name.tenant();
            let _ = name.domain();
            let _ = name.purpose();
            let _ = name.revision();
            let _ = name.revision_number();
            let _ = name.base_path();
            let _ = name.display_name();
            let _ = name.to_string();

            // Test next revision generation
            let _ = name.next_revision();

            // Test lineage checking with self
            let _ = name.is_same_lineage(&name);
        }
    }
});
