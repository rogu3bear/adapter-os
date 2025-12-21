//! Trace validation utilities: parse, verify, and report stats

use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};

use adapteros_core::{AosError, Result};
use zstd::stream::read::Decoder;

use crate::schema::{Event, TraceBundleHeader};

/// Options to control validation behavior
#[derive(Debug, Clone, Default)]
pub struct TraceValidationOptions {
    pub strict: bool,
    pub verify_hash: bool,
    pub max_events: Option<usize>,
    pub max_bytes: Option<u64>,
    pub max_line_len: Option<usize>,
}

impl TraceValidationOptions {
    pub fn strict() -> Self {
        Self {
            strict: true,
            ..Default::default()
        }
    }

    pub fn tolerant() -> Self {
        Self {
            strict: false,
            ..Default::default()
        }
    }
}

/// Validation outcome and read statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct TraceValidationReport {
    pub bytes_read: u64,
    pub events_read: usize,
    pub errors: usize,
    pub skipped_lines: usize,
    pub max_line_limit_hits: usize,
    /// Whether event-hash verification was enabled
    pub verified_event_hashes: bool,
}

/// Validate a trace file with the provided options and return a report.
/// In strict mode, the first fatal error returns immediately.
pub fn validate_path(path: &Path, opts: &TraceValidationOptions) -> Result<TraceValidationReport> {
    let mut reader = open_trace_reader(path)?;

    let mut bytes_read = 0u64;
    let mut events_read = 0usize;
    let mut errors = 0usize;
    let mut skipped_lines = 0usize;
    let mut max_line_limit_hits = 0usize;

    let mut line = String::new();
    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .map_err(|e| AosError::Telemetry(format!("Failed to read trace line: {}", e)))?;
        if read == 0 {
            break;
        }

        bytes_read = bytes_read.saturating_add(read as u64);
        if let Some(max) = opts.max_bytes {
            if bytes_read > max {
                break;
            }
        }

        if line.trim().is_empty() {
            continue;
        }

        if is_header_line(&line) {
            continue;
        }

        if let Some(max_len) = opts.max_line_len {
            if line.len() > max_len {
                max_line_limit_hits += 1;
                errors += 1;
                skipped_lines += 1;
                if opts.strict {
                    return Err(AosError::Telemetry(format!(
                        "Trace line exceeds max_line_len ({} bytes): {}",
                        max_len,
                        path.display()
                    )));
                }
                continue;
            }
        }

        let event: Event = match serde_json::from_str(&line) {
            Ok(event) => event,
            Err(e) => {
                errors += 1;
                skipped_lines += 1;
                if opts.strict {
                    return Err(AosError::Telemetry(format!(
                        "Failed to parse event JSON: {}",
                        e
                    )));
                }
                continue;
            }
        };

        if opts.verify_hash && !event.verify_hash() {
            errors += 1;
            skipped_lines += 1;
            if opts.strict {
                return Err(AosError::Telemetry(
                    "Event hash verification failed".to_string(),
                ));
            }
            continue;
        }

        events_read += 1;
        if let Some(max) = opts.max_events {
            if events_read >= max {
                break;
            }
        }
    }

    Ok(TraceValidationReport {
        bytes_read,
        events_read,
        errors,
        skipped_lines,
        max_line_limit_hits,
        verified_event_hashes: opts.verify_hash,
    })
}

fn open_trace_reader(path: &Path) -> Result<BufReader<Box<dyn Read>>> {
    let file = File::open(path)
        .map_err(|e| AosError::Telemetry(format!("Failed to open trace file: {}", e)))?;

    let reader: Box<dyn Read> = match path.extension().and_then(|ext| ext.to_str()) {
        Some("zst") => {
            let decoder = Decoder::new(file).map_err(|e| {
                AosError::Telemetry(format!("Failed to open zstd decoder: {}", e))
            })?;
            Box::new(decoder)
        }
        _ => Box::new(file),
    };

    Ok(BufReader::new(reader))
}

fn is_header_line(line: &str) -> bool {
    if let Ok(header) = serde_json::from_str::<TraceBundleHeader>(line) {
        return header.is_header();
    }
    false
}
