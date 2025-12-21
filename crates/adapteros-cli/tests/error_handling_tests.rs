//! Tests for CLI error handling
//!
//! Tests that the CLI properly handles and reports errors:
//! - Error code registry
//! - Error formatting and display
//! - Error explanations
//! - Error categorization

use adapteros_cli::error_codes::{all_error_codes, get_error_code, ErrorCode};

#[cfg(test)]
mod error_code_tests {
    use super::*;

    #[test]
    fn test_all_error_codes_not_empty() {
        let codes = all_error_codes();
        assert!(
            !codes.is_empty(),
            "error codes registry should not be empty"
        );
    }

    #[test]
    fn test_error_codes_have_required_fields() {
        let codes = all_error_codes();

        for code in codes {
            assert!(!code.code.is_empty(), "code should not be empty");
            assert!(!code.category.is_empty(), "category should not be empty");
            assert!(!code.title.is_empty(), "title should not be empty");
            assert!(!code.cause.is_empty(), "cause should not be empty");
            assert!(!code.fix.is_empty(), "fix should not be empty");

            // Code should start with 'E' and be followed by digits
            assert!(
                code.code.starts_with('E'),
                "code {} should start with 'E'",
                code.code
            );
            assert!(
                code.code[1..].chars().all(|c| c.is_ascii_digit()),
                "code {} should have digits after 'E'",
                code.code
            );
        }
    }

    #[test]
    fn test_error_code_categories() {
        let codes = all_error_codes();
        let mut categories = std::collections::HashSet::new();

        for code in codes {
            categories.insert(code.category);
        }

        // Verify expected categories exist
        let expected_categories = vec![
            "Crypto/Signing",
            "Policy/Determinism",
            "Kernels/Build",
            "Telemetry/Chain",
            "Artifacts/CAS",
            "Adapters/DIR",
            "Node/Cluster",
            "CLI/Config",
            "OS/Environment",
        ];

        for expected in expected_categories {
            assert!(
                categories.contains(expected),
                "category {} should exist",
                expected
            );
        }
    }

    #[test]
    fn test_error_code_ranges() {
        let codes = all_error_codes();

        for code in codes {
            let num: u32 = code.code[1..].parse().expect("should be valid number");

            // Test category ranges based on documentation
            match code.category {
                "Crypto/Signing" => {
                    assert!(
                        (1000..2000).contains(&num),
                        "Crypto/Signing code {} should be E1xxx",
                        code.code
                    );
                }
                "Policy/Determinism" => {
                    assert!(
                        (2000..3000).contains(&num),
                        "Policy/Determinism code {} should be E2xxx",
                        code.code
                    );
                }
                "Kernels/Build" => {
                    assert!(
                        (3000..4000).contains(&num),
                        "Kernels/Build code {} should be E3xxx",
                        code.code
                    );
                }
                "Telemetry/Chain" => {
                    assert!(
                        (4000..5000).contains(&num),
                        "Telemetry/Chain code {} should be E4xxx",
                        code.code
                    );
                }
                "Artifacts/CAS" => {
                    assert!(
                        (5000..6000).contains(&num),
                        "Artifacts/CAS code {} should be E5xxx",
                        code.code
                    );
                }
                "Adapters/DIR" => {
                    assert!(
                        (6000..7000).contains(&num),
                        "Adapters/DIR code {} should be E6xxx",
                        code.code
                    );
                }
                "Node/Cluster" => {
                    assert!(
                        (7000..8000).contains(&num),
                        "Node/Cluster code {} should be E7xxx",
                        code.code
                    );
                }
                "CLI/Config" => {
                    assert!(
                        (8000..9000).contains(&num),
                        "CLI/Config code {} should be E8xxx",
                        code.code
                    );
                }
                "OS/Environment" => {
                    assert!(
                        (9000..10000).contains(&num),
                        "OS/Environment code {} should be E9xxx",
                        code.code
                    );
                }
                _ => {
                    panic!("unexpected category: {}", code.category);
                }
            }
        }
    }

    #[test]
    fn test_error_code_uniqueness() {
        let codes = all_error_codes();
        let mut seen = std::collections::HashSet::new();

        for code in codes {
            assert!(
                seen.insert(code.code),
                "duplicate error code: {}",
                code.code
            );
        }
    }

    #[test]
    fn test_get_error_code_existing() {
        // Test that we can retrieve known error codes
        if let Some(code) = get_error_code("E1001") {
            assert_eq!(code.code, "E1001");
            assert_eq!(code.category, "Crypto/Signing");
        } else {
            panic!("E1001 should exist");
        }
    }

    #[test]
    fn test_get_error_code_nonexistent() {
        let result = get_error_code("E9999");
        assert!(result.is_none(), "E9999 should not exist");
    }

    #[test]
    fn test_error_code_display() {
        let code = ErrorCode {
            code: "E1001",
            category: "Crypto/Signing",
            title: "Invalid Signature",
            cause: "The signature verification failed",
            fix: "1. Re-sign the bundle\n2. Verify the signature",
            related_docs: &["docs/ARCHITECTURE.md"],
        };

        let display = format!("{}", code);

        // Verify output contains expected elements
        assert!(display.contains("E1001"));
        assert!(display.contains("Crypto/Signing"));
        assert!(display.contains("Invalid Signature"));
        assert!(display.contains("signature verification failed"));
        assert!(display.contains("Re-sign the bundle"));
        assert!(display.contains("docs/ARCHITECTURE.md"));
    }

    #[test]
    fn test_error_code_display_no_docs() {
        let code = ErrorCode {
            code: "E8001",
            category: "CLI/Config",
            title: "Config Error",
            cause: "Configuration is invalid",
            fix: "Fix the configuration",
            related_docs: &[],
        };

        let display = format!("{}", code);

        // Should not contain documentation section
        assert!(!display.contains("Related Documentation"));
    }

    #[test]
    fn test_error_code_multiline_fix() {
        let code = ErrorCode {
            code: "E2001",
            category: "Policy/Determinism",
            title: "Determinism Violation",
            cause: "Non-deterministic behavior detected",
            fix: "1. Check seed derivation\n2. Verify router sorting\n3. Run determinism checks",
            related_docs: &[],
        };

        let display = format!("{}", code);

        // All fix steps should be present
        assert!(display.contains("Check seed derivation"));
        assert!(display.contains("Verify router sorting"));
        assert!(display.contains("Run determinism checks"));
    }

    #[test]
    fn test_error_codes_have_actionable_fixes() {
        let codes = all_error_codes();

        for code in codes {
            // Fix should contain actionable words
            let fix_lower = code.fix.to_lowercase();
            let has_action = fix_lower.contains("run")
                || fix_lower.contains("check")
                || fix_lower.contains("verify")
                || fix_lower.contains("install")
                || fix_lower.contains("update")
                || fix_lower.contains("set")
                || fix_lower.contains("enable")
                || fix_lower.contains("disable")
                || fix_lower.contains("ensure")
                || fix_lower.contains("confirm")
                || fix_lower.contains("contact")
                || fix_lower.contains("review")
                || fix_lower.contains("remove")
                || fix_lower.contains("add")
                || fix_lower.contains("create")
                || fix_lower.contains("delete");

            assert!(
                has_action,
                "code {} should have actionable fix instructions",
                code.code
            );
        }
    }

    #[test]
    fn test_error_code_serialization() {
        let code = ErrorCode {
            code: "E1001",
            category: "Crypto/Signing",
            title: "Invalid Signature",
            cause: "The signature verification failed",
            fix: "Re-sign the bundle",
            related_docs: &["docs/ARCHITECTURE.md"],
        };

        // Test that error code can be serialized to JSON
        let json = serde_json::to_string(&code).expect("should serialize");
        assert!(json.contains("E1001"));
        assert!(json.contains("Crypto/Signing"));

        // Test deserialization
        let deserialized: ErrorCode =
            serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(deserialized.code, "E1001");
        assert_eq!(deserialized.category, "Crypto/Signing");
    }

    #[test]
    fn test_crypto_signing_errors() {
        let codes = all_error_codes();
        let crypto_codes: Vec<_> = codes
            .into_iter()
            .filter(|c| c.category == "Crypto/Signing")
            .collect();

        assert!(!crypto_codes.is_empty(), "should have crypto/signing errors");

        // Verify common crypto errors exist
        let code_nums: Vec<&str> = crypto_codes.iter().map(|c| c.code).collect();
        assert!(
            code_nums.contains(&"E1001"),
            "should have E1001 (Invalid Signature)"
        );
    }

    #[test]
    fn test_cli_config_errors() {
        let codes = all_error_codes();
        let cli_codes: Vec<_> = codes
            .into_iter()
            .filter(|c| c.category == "CLI/Config")
            .collect();

        assert!(!cli_codes.is_empty(), "should have CLI/Config errors");
    }
}

#[cfg(test)]
mod error_handling_behavior {
    use super::*;

    #[test]
    fn test_error_code_lookup_case_sensitivity() {
        // Error codes should be case-sensitive
        assert!(get_error_code("E1001").is_some());
        assert!(get_error_code("e1001").is_none());
        assert!(get_error_code("E1001").is_some());
    }

    #[test]
    fn test_error_code_formatting_consistency() {
        let codes = all_error_codes();

        for code in codes {
            // All codes should be exactly 5 characters (E + 4 digits)
            assert_eq!(
                code.code.len(),
                5,
                "code {} should be 5 characters",
                code.code
            );

            // Category should be properly capitalized
            assert!(
                code.category.chars().next().unwrap().is_uppercase(),
                "category {} should start with uppercase",
                code.category
            );

            // Title should be properly capitalized
            assert!(
                code.title.chars().next().unwrap().is_uppercase(),
                "title {} should start with uppercase",
                code.title
            );
        }
    }

    #[test]
    fn test_error_messages_are_helpful() {
        let codes = all_error_codes();

        for code in codes {
            // Cause should explain the problem
            assert!(
                code.cause.len() > 10,
                "code {} cause should be descriptive",
                code.code
            );

            // Fix should provide solution
            assert!(
                code.fix.len() > 10,
                "code {} fix should be descriptive",
                code.code
            );
        }
    }
}
