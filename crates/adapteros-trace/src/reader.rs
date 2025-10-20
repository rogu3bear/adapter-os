//! Trace reader for reading events from files

use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    time::Duration,
};

use adapteros_core::{AosError, Result};
use serde_json;

use crate::schema::{Event, TraceBundle};

/// Reader for trace bundles
pub struct TraceReader {
    reader: Box<dyn BufRead>,
    path_hint: Option<PathBuf>,
    line_no: usize,
    max_line_len: Option<usize>,
    verify_hash: bool,
    tolerant: bool,
    max_events: Option<usize>,
    max_bytes: Option<u64>,
    stats: TraceReadStats,
}

impl TraceReader {
    /// Create a new trace reader from a file path
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        let file = File::open(path_ref).map_err(|e| {
            AosError::Telemetry(format!(
                "Failed to open trace file {}: {}",
                path_ref.display(),
                e
            ))
        })?;

        // If this looks like a zstd-compressed file, transparently decode
        let reader: Box<dyn Read> = match path_ref.extension().and_then(|s| s.to_str()) {
            Some("zst") | Some("zstd") => Box::new(zstd::Decoder::new(file).map_err(|e| {
                AosError::Telemetry(format!(
                    "Failed to open zstd decoder for {}: {}",
                    path_ref.display(),
                    e
                ))
            })?),
            _ => Box::new(file),
        };

        let reader = BufReader::new(reader);

        Ok(Self {
            reader: Box::new(reader),
            path_hint: Some(path_ref.to_path_buf()),
            line_no: 0,
            max_line_len: None,
            verify_hash: false,
            tolerant: false,
            max_events: None,
            max_bytes: None,
            stats: TraceReadStats::default(),
        })
    }

    /// Create a trace reader from any readable source (tests, in-memory buffers)
    pub fn from_reader<R: 'static + Read>(r: R) -> Self {
        Self {
            reader: Box::new(BufReader::new(r)),
            path_hint: None,
            line_no: 0,
            max_line_len: None,
            verify_hash: false,
            tolerant: false,
            max_events: None,
            max_bytes: None,
            stats: TraceReadStats::default(),
        }
    }

    /// Create a trace reader from any buffer reader
    pub fn from_bufread<R: 'static + BufRead>(r: R) -> Self {
        Self {
            reader: Box::new(r),
            path_hint: None,
            line_no: 0,
            max_line_len: None,
            verify_hash: false,
            tolerant: false,
            max_events: None,
            max_bytes: None,
            stats: TraceReadStats::default(),
        }
    }

    /// Set a maximum line length guard (in bytes). Returns self for chaining.
    pub fn with_max_line_len(mut self, limit: usize) -> Self {
        self.max_line_len = Some(limit);
        self
    }

    /// Enable or disable event hash verification
    pub fn with_verify_hash(mut self, verify: bool) -> Self {
        self.verify_hash = verify;
        self
    }

    /// Set tolerant mode (skip invalid lines/events and continue)
    pub fn with_tolerant_mode(mut self, tolerant: bool) -> Self {
        self.tolerant = tolerant;
        self
    }

    /// Cap maximum events to read (applies to read_all_events and iter usage)
    pub fn with_max_events(mut self, limit: usize) -> Self {
        self.max_events = Some(limit);
        self
    }

    /// Cap maximum bytes to read (line bytes accumulation)
    pub fn with_max_bytes(mut self, limit: u64) -> Self {
        self.max_bytes = Some(limit);
        self
    }

    /// Read all events from the trace file
    pub fn read_all_events(mut self) -> Result<Vec<Event>> {
        let mut events = Vec::new();
        while let Some(ev) = self.read_next_event()? {
            events.push(ev);
            if let Some(max) = self.max_events {
                if events.len() >= max {
                    break;
                }
            }
        }
        Ok(events)
    }

    /// Read events one by one
    pub fn read_next_event(&mut self) -> Result<Option<Event>> {
        let mut buf = Vec::with_capacity(4096);
        loop {
            buf.clear();
            match self.reader.read_until(b'\n', &mut buf) {
                Ok(0) => return Ok(None), // EOF
                Ok(n) => {
                    self.line_no += 1;
                    self.stats.bytes_read = self.stats.bytes_read.saturating_add(n as u64);

                    if let Some(limit) = self.max_bytes {
                        if self.stats.bytes_read > limit {
                            return Err(AosError::Telemetry(format!(
                                "Read exceeded max bytes limit: {} > {}",
                                self.stats.bytes_read, limit
                            )));
                        }
                    }

                    if let Some(limit) = self.max_line_len {
                        if buf.len() > limit {
                            self.stats.max_line_limit_hits += 1;
                            let path = self
                                .path_hint
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| "<stream>".to_string());
                            let msg = format!(
                                "Trace line too long at {}:{} ({} bytes > {} limit)",
                                path,
                                self.line_no,
                                buf.len(),
                                limit
                            );
                            if self.tolerant {
                                self.stats.errors += 1;
                                tracing::warn!("{}", msg);
                                continue;
                            } else {
                                return Err(AosError::Telemetry(msg));
                            }
                        }
                    }

                    // Trim trailing CR/LF
                    while matches!(buf.last(), Some(b'\n' | b'\r')) {
                        buf.pop();
                    }
                    if buf.is_empty() {
                        self.stats.skipped_lines += 1;
                        continue;
                    }

                    let event: Event = match serde_json::from_slice(&buf) {
                        Ok(ev) => ev,
                        Err(e) => {
                            let path = self
                                .path_hint
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| "<stream>".to_string());
                            let msg = format!(
                                "Failed to parse event at {}:{}: {}",
                                path, self.line_no, e
                            );
                            if self.tolerant {
                                self.stats.errors += 1;
                                tracing::warn!("{}", msg);
                                continue;
                            } else {
                                return Err(AosError::Telemetry(msg));
                            }
                        }
                    };

                    if self.verify_hash && !event.verify_hash() {
                        let msg =
                            format!("Event hash verification failed at line {}", self.line_no);
                        if self.tolerant {
                            self.stats.errors += 1;
                            tracing::warn!("{}", msg);
                            continue;
                        } else {
                            return Err(AosError::Telemetry(msg));
                        }
                    }

                    self.stats.events_read += 1;
                    return Ok(Some(event));
                }
                Err(e) => {
                    let path = self
                        .path_hint
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "<stream>".to_string());
                    return Err(AosError::Telemetry(format!(
                        "Failed to read line {} in {}: {}",
                        self.line_no + 1,
                        path,
                        e
                    )));
                }
            }
        }
    }

    /// Iterate over events without loading all into memory
    pub fn iter<'a>(&'a mut self) -> TraceIter<'a> {
        TraceIter { reader: self }
    }

    /// Read at most N events
    pub fn read_n_events(mut self, n: usize) -> Result<Vec<Event>> {
        self.max_events = Some(n);
        self.read_all_events()
    }

    /// Return accumulated read statistics
    pub fn stats(&self) -> &TraceReadStats {
        &self.stats
    }

    /// Follow (tail) the file, polling for new events. Stops when `should_stop` returns true.
    pub fn follow_until<F, S>(
        &mut self,
        poll: Duration,
        mut on_event: F,
        mut should_stop: S,
    ) -> Result<()>
    where
        F: FnMut(Event),
        S: FnMut() -> bool,
    {
        loop {
            match self.read_next_event()? {
                Some(ev) => on_event(ev),
                None => {
                    if should_stop() {
                        break;
                    }
                    std::thread::sleep(poll);
                }
            }
        }
        Ok(())
    }
}

/// Iterator over events for a TraceReader
pub struct TraceIter<'a> {
    reader: &'a mut TraceReader,
}

impl<'a> Iterator for TraceIter<'a> {
    type Item = Result<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.reader.read_next_event() {
            Ok(Some(ev)) => Some(Ok(ev)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

/// Read a complete trace bundle from a file
pub fn read_trace_bundle<P: AsRef<Path>>(path: P) -> Result<TraceBundle> {
    let path_ref = path.as_ref();
    let reader = TraceReader::new(path_ref)?;
    let events = reader.read_all_events()?;

    if events.is_empty() {
        return Err(AosError::Telemetry(
            "No events found in trace file".to_string(),
        ));
    }

    // Extract metadata from first event
    let first_event = &events[0];
    let metadata = first_event.metadata.clone();

    // Build bundle and add events to compute counts and hash
    let mut bundle = TraceBundle::new(
        metadata.global_seed,
        metadata.plan_id,
        metadata.cpid,
        metadata.tenant_id,
        metadata.session_id,
    );
    for ev in events {
        bundle.add_event(ev);
    }

    // Populate metadata fields
    bundle.metadata.created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_nanos();
    if let Ok(meta) = std::fs::metadata(path_ref) {
        bundle.metadata.total_size_bytes = meta.len();
    }

    Ok(bundle)
}

/// Read only the first event's metadata from a file
pub fn read_trace_header<P: AsRef<Path>>(path: P) -> Result<crate::schema::EventMetadata> {
    let mut reader = TraceReader::new(&path)?;
    while let Some(ev) = reader.read_next_event()? {
        return Ok(ev.metadata);
    }
    Err(AosError::Telemetry(
        "No events found in trace file".to_string(),
    ))
}

/// Reader statistics for observability and guard enforcement
#[derive(Debug, Clone, Default)]
pub struct TraceReadStats {
    pub bytes_read: u64,
    pub events_read: usize,
    pub errors: usize,
    pub skipped_lines: usize,
    pub max_line_limit_hits: usize,
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

        let reader = TraceReader::new(&trace_path).unwrap();
        let events = reader.read_all_events().unwrap();

        assert_eq!(events.len(), 0); // Empty bundle
    }

    #[test]
    fn test_read_trace_bundle() {
        let temp_dir = TempDir::new().unwrap();
        let trace_path = temp_dir.path().join("test_trace.ndjson");

        // Create a test file with at least one event
        use crate::logical_clock::LogicalTimestamp;
        use std::collections::HashMap;

        let mut bundle = TraceBundle::new(
            adapteros_core::B3Hash::hash(b"test_seed"),
            "test_plan".to_string(),
            "test_cpid".to_string(),
            "test_tenant".to_string(),
            "test_session".to_string(),
        );

        // Add a test event
        let metadata = crate::schema::EventMetadata {
            global_seed: adapteros_core::B3Hash::hash(b"test_seed"),
            plan_id: "test_plan".to_string(),
            cpid: "test_cpid".to_string(),
            tenant_id: "test_tenant".to_string(),
            session_id: "test_session".to_string(),
            adapter_ids: Vec::new(),
            memory_usage_mb: 0,
            gpu_utilization_pct: 0.0,
            custom: HashMap::new(),
        };

        let logical_timestamp =
            LogicalTimestamp::new(0, 0, None, adapteros_core::B3Hash::hash(b"test"));
        let event = crate::schema::Event::new(
            1,
            "test_op".to_string(),
            "test_event".to_string(),
            HashMap::new(),
            HashMap::new(),
            metadata,
            logical_timestamp,
        );

        bundle.add_event(event);

        write_trace_bundle(&trace_path, bundle).unwrap();

        let read_bundle = read_trace_bundle(&trace_path).unwrap();
        assert_eq!(read_bundle.plan_id, "test_plan");
        assert_eq!(read_bundle.cpid, "test_cpid");
        assert_eq!(read_bundle.events.len(), 1);
    }

    #[test]
    fn test_iter_over_events_and_skip_blanks() {
        let temp_dir = TempDir::new().unwrap();
        let trace_path = temp_dir.path().join("iter_test.ndjson");

        // Create a test file with one event
        let mut bundle = TraceBundle::new(
            adapteros_core::B3Hash::hash(b"seed"),
            "plan".to_string(),
            "cpid".to_string(),
            "tenant".to_string(),
            "session".to_string(),
        );

        use crate::logical_clock::LogicalTimestamp;
        let metadata = crate::schema::EventMetadata {
            global_seed: adapteros_core::B3Hash::hash(b"seed"),
            plan_id: "plan".to_string(),
            cpid: "cpid".to_string(),
            tenant_id: "tenant".to_string(),
            session_id: "session".to_string(),
            adapter_ids: Vec::new(),
            memory_usage_mb: 0,
            gpu_utilization_pct: 0.0,
            custom: std::collections::HashMap::new(),
        };
        let ts = LogicalTimestamp::new(0, 0, None, adapteros_core::B3Hash::hash(b"ts"));
        let ev = crate::schema::Event::new(
            1,
            "op".to_string(),
            "etype".to_string(),
            std::collections::HashMap::new(),
            std::collections::HashMap::new(),
            metadata,
            ts,
        );
        bundle.add_event(ev);
        write_trace_bundle(&trace_path, bundle).unwrap();

        let mut reader = TraceReader::new(&trace_path).unwrap();
        let mut it = reader.iter();
        let first = it.next().unwrap().unwrap();
        assert_eq!(first.op_id, "op");
        assert!(it.next().is_none());
    }

    #[test]
    fn test_error_includes_path_and_line() {
        use std::io::Write;
        // Create an invalid trace: valid JSON line then invalid line
        let mut buf: Vec<u8> = Vec::new();
        buf.write_all(
            br#"{"a":1}
{"#,
        )
        .unwrap();
        let mut reader = TraceReader::from_reader(&buf[..]);
        // First event (ignore result)
        let _ = reader.read_next_event();
        // Second should error with path and line number
        let err = reader.read_next_event().unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("<stream>"));
        assert!(msg.contains(":2"));
    }

    #[test]
    fn test_max_line_len_guard() {
        // Create a valid but very long JSON line exceeding the guard
        let long_val = "a".repeat(1024);
        let json_line = format!("{{\"k\":\"{}\"}}\n", long_val);
        let mut reader = TraceReader::from_reader(json_line.as_bytes()).with_max_line_len(100);
        let err = reader.read_next_event().unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("too long"));
        assert!(msg.contains(":1"));
    }
}
