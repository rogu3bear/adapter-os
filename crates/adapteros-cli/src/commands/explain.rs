//! Explain error codes
//!
//! Use `aosctl explain <ERROR_CODE>` to get detailed information about any
//! AdapterOS error code. For compile-time checked error code mapping from
//! actual errors, use `AosError::ecode()` from `adapteros_core::errors`.

use crate::error_codes;
use anyhow::{Context, Result};

/// Explain an error code
///
/// Accepts error codes like E1001, E2002, etc. For programmatic mapping
/// from AosError instances, use the compile-time checked `AosError::ecode()` method.
pub async fn explain(code_or_name: &str) -> Result<()> {
    // Try as error code (E3001)
    if let Some(error_code) = error_codes::REGISTRY.get(code_or_name) {
        println!("{}", error_code);
        return Ok(());
    }

    // Try case-insensitive match on error code
    let upper = code_or_name.to_uppercase();
    if let Some(error_code) = error_codes::REGISTRY.get(upper.as_str()) {
        println!("{}", error_code);
        return Ok(());
    }

    // Not found - provide helpful guidance
    Err(anyhow::anyhow!(
        "Error code not found: {}\n\n\
         Usage:\n\
         - aosctl explain E3001           (explain specific error code)\n\
         - aosctl error-codes             (list all codes)\n\n\
         Note: AosError variant name lookup has been removed.\n\
         Use the typed AosError::ecode() method for compile-time checked mapping:\n\
         \n\
           use adapteros_core::errors::{{AosError, HasECode}};\n\
           let code = error.ecode();  // Returns ECode enum",
        code_or_name
    ))
}

/// List all error codes
pub async fn list_error_codes(json: bool) -> Result<()> {
    let codes: Vec<_> = error_codes::REGISTRY.values().collect();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&codes).context("Failed to serialize error codes")?
        );
    } else {
        println!("AdapterOS Error Code Registry");
        println!("════════════════════════════════════════════════════════════════\n");

        let mut by_category: std::collections::BTreeMap<&str, Vec<_>> =
            std::collections::BTreeMap::new();
        for code in &codes {
            by_category.entry(code.category).or_default().push(*code);
        }

        for (category, codes) in by_category {
            println!("📂 {}", category);
            println!("────────────────────────────────────────────────────────────────");
            for code in codes {
                println!("  {} - {}", code.code, code.title);
            }
            println!();
        }

        println!("Total: {} error codes\n", codes.len());
        println!("Usage: aosctl explain <CODE>");
    }

    Ok(())
}
