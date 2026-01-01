//! Explain error codes

use crate::error_codes;
use anyhow::{Context, Result};

/// Explain an error code or AosError name
pub async fn explain(code_or_name: &str) -> Result<()> {
    // Try as error code first (E3001)
    if let Some(error_code) = error_codes::REGISTRY.get(code_or_name) {
        println!("{}", error_code);
        return Ok(());
    }

    // Try as AosError variant name (runtime lookup with user input)
    #[allow(deprecated)]
    if let Some(error_code) = error_codes::find_by_aos_error(code_or_name) {
        println!(
            "📌 Mapped from AosError::{} to {}\n",
            code_or_name, error_code.code
        );
        println!("{}", error_code);
        return Ok(());
    }

    // Not found
    Err(anyhow::anyhow!(
        "Error code or AosError name not found: {}\n\n\
         Try:\n\
         - aosctl error-codes             (list all codes)\n\
         - aosctl explain E3001           (explain specific code)\n\
         - aosctl explain InvalidHash     (explain AosError variant)",
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
