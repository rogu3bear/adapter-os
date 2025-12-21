//! Generate HTML report from telemetry bundle

use crate::output::OutputWriter;
use adapteros_telemetry::generate_html_report;
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct ReportResult {
    output_path: String,
}

pub async fn run(bundle: &Path, output_path: &Path, output: &OutputWriter) -> Result<()> {
    output.info(format!(
        "Generating report from bundle: {}",
        bundle.display()
    ));

    generate_html_report(bundle, output_path)?;

    output.success(format!("Report generated: {}", output_path.display()));
    output.blank();
    output.info("Open in browser:");
    output.print(format!("  open {}", output_path.display()));

    if output.is_json() {
        let result = ReportResult {
            output_path: output_path.display().to_string(),
        };
        output.json(&result)?;
    }

    Ok(())
}
