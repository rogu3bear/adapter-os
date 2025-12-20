//! Tests for tenant isolation in handler code
//!
//! These tests ensure that handler code uses tenant-scoped database methods
//! instead of cross-tenant methods that could leak data.

use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Scans handler files for calls to deprecated tenant-unscoped adapter methods.
///
/// This test fails if any handler file contains calls to `db.get_adapter(`
/// instead of the tenant-scoped `db.get_adapter_for_tenant(`.
///
/// Allowed exceptions:
/// - Test files (contain "test" in path)
/// - Internal DB methods (in adapteros-db crate)
/// - System-level operations (explicitly documented)
#[test]
fn no_unscoped_adapter_queries_in_handlers() {
    let handlers_dir = Path::new("../adapteros-server-api/src/handlers");

    if !handlers_dir.exists() {
        // Skip if running from different directory
        println!(
            "Skipping test: handlers directory not found at {:?}",
            handlers_dir
        );
        return;
    }

    let mut violations: Vec<(String, usize, String)> = Vec::new();

    for entry in WalkDir::new(handlers_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        let path = entry.path();
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (line_num, line) in content.lines().enumerate() {
            // Check for direct get_adapter calls (not get_adapter_for_tenant)
            if line.contains(".get_adapter(") && !line.contains("get_adapter_for_tenant") {
                // Skip if it's a comment
                let trimmed = line.trim();
                if trimmed.starts_with("//") || trimmed.starts_with("*") {
                    continue;
                }

                violations.push((
                    path.display().to_string(),
                    line_num + 1,
                    line.trim().to_string(),
                ));
            }
        }
    }

    if !violations.is_empty() {
        let mut msg = String::from("\n\nTenant isolation violations found in handlers:\n");
        msg.push_str("=".repeat(60).as_str());
        msg.push('\n');

        for (file, line, context) in &violations {
            msg.push_str(&format!("\n{}:{}\n", file, line));
            msg.push_str(&format!("  {}\n", context));
            msg.push_str("  -> Use get_adapter_for_tenant() instead of get_adapter()\n");
        }

        msg.push_str("\n");
        msg.push_str("=".repeat(60).as_str());
        msg.push_str(
            "\n\nTo fix: Replace .get_adapter(id) with .get_adapter_for_tenant(tenant_id, id)\n",
        );

        panic!("{}", msg);
    }
}

/// Scans services for calls to deprecated tenant-unscoped adapter methods.
#[test]
fn no_unscoped_adapter_queries_in_services() {
    let services_dir = Path::new("../adapteros-server-api/src/services");

    if !services_dir.exists() {
        println!(
            "Skipping test: services directory not found at {:?}",
            services_dir
        );
        return;
    }

    let mut violations: Vec<(String, usize, String)> = Vec::new();

    // Files that are allowed to use unscoped methods (internal implementation)
    let allowed_files = ["adapter_service.rs"]; // Internal implementation wrapper

    for entry in WalkDir::new(services_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        let path = entry.path();
        let filename = path.file_name().unwrap_or_default().to_str().unwrap_or("");

        // Skip allowed files
        if allowed_files.contains(&filename) {
            continue;
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (line_num, line) in content.lines().enumerate() {
            if line.contains(".get_adapter(") && !line.contains("get_adapter_for_tenant") {
                let trimmed = line.trim();
                if trimmed.starts_with("//") || trimmed.starts_with("*") {
                    continue;
                }

                violations.push((
                    path.display().to_string(),
                    line_num + 1,
                    line.trim().to_string(),
                ));
            }
        }
    }

    if !violations.is_empty() {
        let mut msg = String::from("\n\nTenant isolation violations found in services:\n");
        msg.push_str("=".repeat(60).as_str());
        msg.push('\n');

        for (file, line, context) in &violations {
            msg.push_str(&format!("\n{}:{}\n", file, line));
            msg.push_str(&format!("  {}\n", context));
            msg.push_str("  -> Use get_adapter_for_tenant() instead of get_adapter()\n");
        }

        msg.push('\n');
        msg.push_str("=".repeat(60).as_str());
        msg.push_str(
            "\n\nTo fix: Replace .get_adapter(id) with .get_adapter_for_tenant(tenant_id, id)\n",
        );

        panic!("{}", msg);
    }
}
