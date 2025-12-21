//! Trace reader for reading events from files

use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};

use adapteros_core::{AosError, Result};
use serde_json;
use zstd::stream::read::Decoder;

use crate::schema::{Event, TraceBundle, TraceBundleHeader};

/// Reader for trace bundles
pub struct TraceReader {
    reader: BufReader<Box<dyn Read>>,
}

impl TraceReader {
    /// Create a new trace reader
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let reader = open_trace_reader(path.as_ref())?;
        Ok(Self { reader })
    }

    /// Read all events from the trace file
    pub fn read_all_events(mut self) -> Result<Vec<Event>> {
        let mut events = Vec::new();
        let mut line = String::new();
        let mut line_no: usize = 0;

        loop {
            line.clear();
            let read = self.reader.read_line(&mut line).map_err(|e| {
                AosError::Telemetry(format!("Failed to read line {}: {}", line_no, e))
            })?;
            if read == 0 {
                break;
            }
            line_no += 1;

            if line.trim().is_empty() {
                continue;
            }

            if is_header_line(&line) {
                continue;
            }

            let event: Event = serde_json::from_str(&line).map_err(|e| {
                AosError::Telemetry(format!("Failed to parse event at line {}: {}", line_no, e))
            })?;

            events.push(event);
        }

        Ok(events)
    }

    /// Read events one by one
    pub fn read_next_event(&mut self) -> Result<Option<Event>> {
        let mut line = String::new();

        loop {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) => return Ok(None), // EOF
                Ok(_) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if is_header_line(&line) {
                        continue;
                    }
                    let event: Event = serde_json::from_str(&line).map_err(|e| {
                        AosError::Telemetry(format!("Failed to parse event: {}", e))
                    })?;
                    return Ok(Some(event));
                }
                Err(e) => return Err(AosError::Telemetry(format!("Failed to read line: {}", e))),
            }
        }
    }
}

/// Read a complete trace bundle from a file
pub fn read_trace_bundle<P: AsRef<Path>>(path: P) -> Result<TraceBundle> {
    let mut reader = open_trace_reader(path.as_ref())?;
    let mut events = Vec::new();
    let mut header: Option<TraceBundleHeader> = None;
    let mut line = String::new();
    let mut line_no: usize = 0;

    loop {
        line.clear();
        let read = reader.read_line(&mut line).map_err(|e| {
            AosError::Telemetry(format!("Failed to read line {}: {}", line_no, e))
        })?;
        if read == 0 {
            break;
        }
        line_no += 1;

        if line.trim().is_empty() {
            continue;
        }

        if let Ok(parsed_header) = serde_json::from_str::<TraceBundleHeader>(&line) {
            if parsed_header.is_header() {
                header = Some(parsed_header);
                continue;
            }
        }

        let event: Event = serde_json::from_str(&line).map_err(|e| {
            AosError::Telemetry(format!("Failed to parse event at line {}: {}", line_no, e))
        })?;

        events.push(event);
    }

    if events.is_empty() && header.is_none() {
        return Err(AosError::Telemetry(
            "No events found in trace file".to_string(),
        ));
    }

    if let Some(header) = header {
        return Ok(TraceBundle {
            bundle_id: header.bundle_id,
            version: header.version,
            global_seed: header.global_seed,
            plan_id: header.plan_id,
            cpid: header.cpid,
            tenant_id: header.tenant_id,
            session_id: header.session_id,
            events,
            metadata: header.metadata,
            bundle_hash: header.bundle_hash,
        });
    }

    // Legacy format: derive bundle metadata from first event
    let first_event = events
        .first()
        .ok_or_else(|| AosError::Telemetry("No events found in trace file".to_string()))?;
    let mut bundle = TraceBundle::new(
        first_event.metadata.global_seed,
        first_event.metadata.plan_id.clone(),
        first_event.metadata.cpid.clone(),
        first_event.metadata.tenant_id.clone(),
        first_event.metadata.session_id.clone(),
    );

    for event in events {
        bundle.add_event(event);
    }

    Ok(bundle)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::TraceBundle;
    use crate::write_trace_bundle;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("tempdir")
    }

    #[test]
    fn test_trace_reader_creation() {
        let temp_dir = new_test_tempdir();
        let trace_path = temp_dir.path().join("test_trace.ndjson");

        // Create a test file
        let bundle = TraceBundle::new(
            adapteros_core::B3Hash::hash(b"test_seed"),
            "test_plan".to_string(),
            "test_cpid".to_string(),
            "test_tenant".to_string(),
            "test_session".to_string(),
        );

        write_trace_bundle(&trace_path, bundle).unwrap();

        let reader = TraceReader::new(&trace_path);
        assert!(reader.is_ok());
    }

    #[test]
    fn test_read_all_events() {
        let temp_dir = new_test_tempdir();
        let trace_path = temp_dir.path().join("test_trace.ndjson");

        // Create a test file with events
        let bundle = TraceBundle::new(
            adapteros_core::B3Hash::hash(b"test_seed"),
            "test_plan".to_string(),
            "test_cpid".to_string(),
            "test_tenant".to_string(),
            "test_session".to_string(),
        );

        write_trace_bundle(&trace_path, bundle).unwrap();

        let reader = TraceReader::new(&trace_path).unwrap();
        let events = reader.read_all_events().unwrap();

        assert_eq!(events.len(), 0); // Empty bundle
    }

    #[test]
    fn test_read_trace_bundle() {
        let temp_dir = new_test_tempdir();
        let trace_path = temp_dir.path().join("test_trace.ndjson");

        // Create a test file
        let bundle = TraceBundle::new(
            adapteros_core::B3Hash::hash(b"test_seed"),
            "test_plan".to_string(),
            "test_cpid".to_string(),
            "test_tenant".to_string(),
            "test_session".to_string(),
        );

        write_trace_bundle(&trace_path, bundle).unwrap();

        let read_bundle = read_trace_bundle(&trace_path).unwrap();
        assert_eq!(read_bundle.plan_id, "test_plan");
        assert_eq!(read_bundle.cpid, "test_cpid");
    }
}
