use crate::output::OutputWriter;
use adapteros_trace::{validate_path, TraceValidationOptions};
use anyhow::Result;
use std::path::Path;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    path: &Path,
    strict: bool,
    verify_hash: bool,
    max_events: Option<usize>,
    max_bytes: Option<u64>,
    max_line_len: Option<usize>,
    output: &OutputWriter,
) -> Result<()> {
    output.info(format!("Validating trace: {}", path.display()));

    let mut opts = if strict {
        TraceValidationOptions::strict()
    } else {
        TraceValidationOptions::tolerant()
    };
    opts.verify_hash = verify_hash;
    opts.max_events = max_events;
    opts.max_bytes = max_bytes;
    opts.max_line_len = max_line_len;

    let report = validate_path(path, &opts)?;

    if output.is_json() {
        output.json(&report)?;
    } else {
        output.section("Trace Validation Report");
        output.kv("Bytes Read", &report.bytes_read.to_string());
        output.kv("Events Read", &report.events_read.to_string());
        output.kv("Errors", &report.errors.to_string());
        output.kv("Skipped Lines", &report.skipped_lines.to_string());
        output.kv(
            "Max Line Limit Hits",
            &report.max_line_limit_hits.to_string(),
        );
        output.kv(
            "Verified Event Hashes",
            if report.verified_event_hashes {
                "yes"
            } else {
                "no"
            },
        );
        if report.errors == 0 {
            output.success("Trace validation passed");
        } else {
            output.warning("Trace validation completed with errors (tolerant mode)");
        }
    }

    Ok(())
}
