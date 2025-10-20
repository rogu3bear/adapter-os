//! Trace validation utilities: parse, verify, and report stats

use std::path::Path;

use adapteros_core::{AosError, Result};

use crate::reader::TraceReader;

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
    let mut reader = TraceReader::new(path)?
        .with_verify_hash(opts.verify_hash)
        .with_tolerant_mode(!opts.strict);

    if let Some(n) = opts.max_events {
        reader = reader.with_max_events(n);
    }
    if let Some(b) = opts.max_bytes {
        reader = reader.with_max_bytes(b);
    }
    if let Some(l) = opts.max_line_len {
        reader = reader.with_max_line_len(l);
    }

    // Drain the reader; strict/tolerant behavior is enforced internally
    while let Some(_ev) = reader.read_next_event()? {}

    let stats = reader.stats().clone();
    Ok(TraceValidationReport {
        bytes_read: stats.bytes_read,
        events_read: stats.events_read,
        errors: stats.errors,
        skipped_lines: stats.skipped_lines,
        max_line_limit_hits: stats.max_line_limit_hits,
        verified_event_hashes: opts.verify_hash,
    })
}
