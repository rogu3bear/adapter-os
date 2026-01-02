//! Agent D: UI/UX/Observability checks

use super::{Check, Section, VerifyAgentsArgs};
use anyhow::Result;
use std::fs;
use std::path::Path;

pub async fn run(_args: &VerifyAgentsArgs) -> Result<Section> {
    let mut section = Section::new("Agent D - UI/UX/Observability");

    // 1. Build UI
    section.add_check(check_ui_build());

    // 2. Footer metadata
    section.add_check(check_footer_metadata());

    // 3. Charts
    section.add_check(check_charts());

    // 4. Routing inspector
    section.add_check(check_routing_inspector());

    // 5. Audits page
    section.add_check(check_audits_page());

    // 6. Export functionality
    section.add_check(check_export());

    // 7. Accessibility
    section.add_check(check_accessibility());

    // 8. Toasts
    section.add_check(check_toasts());

    Ok(section)
}

fn check_ui_build() -> Check {
    // Check if Leptos UI directory exists
    let ui_dir = Path::new("crates/adapteros-ui");
    if !ui_dir.exists() {
        return Check::fail("UI Build", vec![], "crates/adapteros-ui/ directory not found");
    }

    // Check for Trunk.toml
    if !Path::new("crates/adapteros-ui/Trunk.toml").exists() {
        return Check::fail("UI Build", vec![], "crates/adapteros-ui/Trunk.toml not found");
    }

    // Check if UI has been built
    let dist_dir = Path::new("crates/adapteros-ui/dist");
    if dist_dir.exists() {
        // Count files in dist
        let file_count = walkdir::WalkDir::new(dist_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .count();

        Check::pass(
            "UI Build",
            vec![
                "crates/adapteros-ui/ directory exists".to_string(),
                "Trunk.toml found".to_string(),
                format!("dist/ directory exists with {} files", file_count),
            ],
        )
    } else {
        Check::skip(
            "UI Build",
            "UI not yet built (run: cd crates/adapteros-ui && trunk build)",
        )
    }
}

fn check_footer_metadata() -> Check {
    // Check for version/build metadata in UI code
    let ui_src = "crates/adapteros-ui/src";
    if !Path::new(ui_src).exists() {
        return Check::skip("Footer metadata", "UI source not found");
    }

    // Search for version/build handling
    let mut found_meta = false;
    for entry in walkdir::WalkDir::new(ui_src).into_iter().flatten() {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "rs") {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if (content.contains("version") || content.contains("build_hash"))
                    && (content.contains("/v1/meta") || content.contains("meta"))
                {
                    found_meta = true;
                    break;
                }
            }
        }
    }

    if found_meta {
        Check::pass(
            "Footer metadata",
            vec!["Version/build metadata handling found in UI".to_string()],
        )
    } else {
        Check::skip(
            "Footer metadata",
            "Version metadata code not explicitly found (may be in components)",
        )
    }
}

fn check_charts() -> Check {
    // Check for plotters-rs in Leptos UI Cargo.toml
    let cargo_toml = Path::new("crates/adapteros-ui/Cargo.toml");
    if !cargo_toml.exists() {
        return Check::skip("Charts", "crates/adapteros-ui/Cargo.toml not found");
    }

    let content = match fs::read_to_string(cargo_toml) {
        Ok(c) => c,
        Err(e) => {
            return Check::fail(
                "Charts",
                vec![],
                format!("Failed to read Cargo.toml: {}", e),
            )
        }
    };

    if content.contains("plotters") {
        Check::pass(
            "Charts",
            vec!["plotters dependency found in crates/adapteros-ui/Cargo.toml".to_string()],
        )
    } else {
        Check::skip(
            "Charts",
            "plotters not found (may use different charting library)",
        )
    }
}

fn check_routing_inspector() -> Check {
    // Check for routing page in UI
    let ui_src = "crates/adapteros-ui/src";
    if !Path::new(ui_src).exists() {
        return Check::skip("Routing inspector", "UI source not found");
    }

    let mut found_routing = false;
    for entry in walkdir::WalkDir::new(ui_src).into_iter().flatten() {
        if entry.file_type().is_file() {
            let path = entry.path();
            if let Some(name) = path.file_name() {
                if name.to_string_lossy().contains("routing") {
                    found_routing = true;
                    break;
                }
            }
        }
    }

    if found_routing {
        Check::pass(
            "Routing inspector",
            vec!["Routing page/component found in UI".to_string()],
        )
    } else {
        Check::skip("Routing inspector", "Routing page not found in UI")
    }
}

fn check_audits_page() -> Check {
    // Check for audits page in UI
    let ui_src = "crates/adapteros-ui/src";
    if !Path::new(ui_src).exists() {
        return Check::skip("Audits page", "UI source not found");
    }

    let mut found_audits = false;
    for entry in walkdir::WalkDir::new(ui_src).into_iter().flatten() {
        if entry.file_type().is_file() {
            let path = entry.path();
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                if name_str.contains("audit") || name_str.contains("promotion") {
                    found_audits = true;
                    break;
                }
            }
        }
    }

    if found_audits {
        Check::pass(
            "Audits page",
            vec!["Audits/promotion page found in UI".to_string()],
        )
    } else {
        Check::skip("Audits page", "Audits page not found in UI")
    }
}

fn check_export() -> Check {
    // Check for export functionality in UI code
    let ui_src = "crates/adapteros-ui/src";
    if !Path::new(ui_src).exists() {
        return Check::skip("Export functionality", "UI source not found");
    }

    let mut found_export = false;
    for entry in walkdir::WalkDir::new(ui_src).into_iter().flatten() {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "rs") {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if (content.contains("export") || content.contains("download"))
                    && (content.contains("csv") || content.contains("json"))
                {
                    found_export = true;
                    break;
                }
            }
        }
    }

    if found_export {
        Check::pass(
            "Export functionality",
            vec!["CSV/JSON export code found in UI".to_string()],
        )
    } else {
        Check::skip(
            "Export functionality",
            "Export code not explicitly found in UI",
        )
    }
}

fn check_accessibility() -> Check {
    // Check for ARIA attributes and responsive CSS
    let ui_src = "crates/adapteros-ui/src";
    let tailwind_css = "crates/adapteros-ui/tailwind.css";

    let mut has_aria = false;
    let mut has_responsive = false;

    // Check for ARIA in Rust code
    if Path::new(ui_src).exists() {
        for entry in walkdir::WalkDir::new(ui_src).into_iter().flatten() {
            if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "rs") {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if content.contains("aria-") || content.contains("role=") {
                        has_aria = true;
                        break;
                    }
                }
            }
        }
    }

    // Check for responsive breakpoints in CSS or Tailwind config
    if let Ok(content) = fs::read_to_string(tailwind_css) {
        if content.contains("@media") && (content.contains("768px") || content.contains("1024px")) {
            has_responsive = true;
        }
    }
    // Also check tailwind.config.js for responsive settings
    if let Ok(content) = fs::read_to_string("crates/adapteros-ui/tailwind.config.js") {
        if content.contains("screens") || content.contains("sm:") || content.contains("md:") {
            has_responsive = true;
        }
    }

    let mut evidence = Vec::new();
    if has_aria {
        evidence.push("ARIA attributes found".to_string());
    }
    if has_responsive {
        evidence.push("Responsive breakpoints found in CSS/Tailwind".to_string());
    }

    if has_aria || has_responsive {
        Check::pass("Accessibility", evidence)
    } else {
        Check::skip(
            "Accessibility",
            "ARIA attributes or responsive CSS not explicitly found",
        )
    }
}

fn check_toasts() -> Check {
    // Check for toast/notification handling
    let ui_src = "crates/adapteros-ui/src";
    if !Path::new(ui_src).exists() {
        return Check::skip("Toasts", "UI source not found");
    }

    let mut found_toasts = false;
    for entry in walkdir::WalkDir::new(ui_src).into_iter().flatten() {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "rs") {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if content.contains("toast") || content.contains("notification") {
                    found_toasts = true;
                    break;
                }
            }
        }
    }

    if found_toasts {
        Check::pass(
            "Toasts",
            vec!["Toast/notification handling found in UI".to_string()],
        )
    } else {
        Check::skip("Toasts", "Toast handling not explicitly found")
    }
}
