//! Trace reader for reading events from files

use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use adapteros_core::{AosError, Result};
use serde_json;

use crate::schema::{Event, TraceBundle};

/// Reader for trace bundles
pub struct TraceReader {
    reader: BufReader<File>,
}

impl TraceReader {
    /// Create a new trace reader
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path.as_ref())
            .map_err(|e| AosError::Telemetry(format!("Failed to open trace file: {}", e)))?;

        let reader = BufReader::new(file);

        Ok(Self { reader })
    }

    /// Read all events from the trace file
    pub fn read_all_events(self) -> Result<Vec<Event>> {
        let mut events = Vec::new();

        for (line_no, line) in self.reader.lines().enumerate() {
            let line = line.map_err(|e| {
                AosError::Telemetry(format!("Failed to read line {}: {}", line_no, e))
            })?;

            if line.trim().is_empty() {
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

        match self.reader.read_line(&mut line) {
            Ok(0) => Ok(None), // EOF
            Ok(_) => {
                if line.trim().is_empty() {
                    self.read_next_event() // Skip empty lines
                } else {
                    let event: Event = serde_json::from_str(&line).map_err(|e| {
                        AosError::Telemetry(format!("Failed to parse event: {}", e))
                    })?;
                    Ok(Some(event))
                }
            }
            Err(e) => Err(AosError::Telemetry(format!("Failed to read line: {}", e))),
        }
    }
}

/// Read a complete trace bundle from a file
pub fn read_trace_bundle<P: AsRef<Path>>(path: P) -> Result<TraceBundle> {
    let reader = TraceReader::new(path)?;
    let events = reader.read_all_events()?;

    if events.is_empty() {
        return Err(AosError::Telemetry(
            "No events found in trace file".to_string(),
        ));
    }

    // Extract metadata from first event
    let first_event = &events[0];
    let metadata = first_event.metadata.clone();

    let bundle = TraceBundle {
        bundle_id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)),
        version: 1,
        global_seed: metadata.global_seed,
        plan_id: metadata.plan_id,
        cpid: metadata.cpid,
        tenant_id: metadata.tenant_id,
        session_id: metadata.session_id,
        events,
        metadata: crate::schema::BundleMetadata {
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("System time before UNIX epoch")
                .as_nanos(),
            event_count: 0, // Will be set by add_event
            total_size_bytes: 0,
            compression: "none".to_string(),
            signature: None,
            custom: std::collections::HashMap::new(),
        },
        bundle_hash: adapteros_core::B3Hash::hash(b"empty"), // Will be updated
    };

    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::TraceBundle;
    use crate::write_trace_bundle;
    use tempfile::TempDir;

    #[test]
    fn test_trace_reader_creation() {
        let temp_dir = TempDir::new().unwrap();
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
        let temp_dir = TempDir::new().unwrap();
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

        let mut reader = TraceReader::new(&trace_path).unwrap();
        let events = reader.read_all_events().unwrap();

        assert_eq!(events.len(), 0); // Empty bundle
    }

    #[test]
    fn test_read_trace_bundle() {
        let temp_dir = TempDir::new().unwrap();
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
