//! Trace writer for writing events to files

use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

use adapteros_core::{AosError, Result};
use serde_json;

use crate::schema::{Event, TraceBundle};

/// Writer for trace bundles
pub struct TraceWriter {
    writer: Box<dyn Write>,
    bundle: TraceBundle,
}

impl TraceWriter {
    /// Create a new trace writer
    pub fn new<P: AsRef<Path>>(path: P, bundle: TraceBundle) -> Result<Self> {
        let file = File::create(path.as_ref())
            .map_err(|e| AosError::Telemetry(format!("Failed to create trace file: {}", e)))?;
        let writer = BufWriter::new(file);
        Ok(Self {
            writer: Box::new(writer),
            bundle,
        })
    }

    /// Create a zstd-compressed writer (level 1-21 typical)
    pub fn new_zstd<P: AsRef<Path>>(path: P, bundle: TraceBundle, level: i32) -> Result<Self> {
        let file = File::create(path.as_ref())
            .map_err(|e| AosError::Telemetry(format!("Failed to create trace file: {}", e)))?;
        let encoder = zstd::Encoder::new(file, level)
            .map_err(|e| AosError::Telemetry(format!("Failed to create zstd encoder: {}", e)))?;
        let writer = BufWriter::new(encoder);
        Ok(Self {
            writer: Box::new(writer),
            bundle,
        })
    }

    /// Write an event to the trace
    pub fn write_event(&mut self, event: Event) -> Result<()> {
        // Add event to bundle
        self.bundle.add_event(event.clone());

        // Write event as NDJSON line
        let line = serde_json::to_string(&event)
            .map_err(|e| AosError::Telemetry(format!("Failed to serialize event: {}", e)))?;

        self.writer
            .write_all(line.as_bytes())
            .map_err(|e| AosError::Telemetry(format!("Failed to write event: {}", e)))?;

        self.writer
            .write_all(b"\n")
            .map_err(|e| AosError::Telemetry(format!("Failed to write newline: {}", e)))?;

        Ok(())
    }

    /// Flush the writer
    pub fn flush(&mut self) -> Result<()> {
        self.writer
            .flush()
            .map_err(|e| AosError::Telemetry(format!("Failed to flush writer: {}", e)))?;

        Ok(())
    }

    /// Get the current bundle
    pub fn bundle(&self) -> &TraceBundle {
        &self.bundle
    }

    /// Consume the writer and return the final bundle
    pub fn finalize(mut self) -> Result<TraceBundle> {
        self.flush()?;
        Ok(self.bundle)
    }
}

/// Write a complete trace bundle to a file
pub fn write_trace_bundle<P: AsRef<Path>>(path: P, bundle: TraceBundle) -> Result<()> {
    let mut writer = TraceWriter::new(
        path,
        TraceBundle::new(
            bundle.global_seed,
            bundle.plan_id.clone(),
            bundle.cpid.clone(),
            bundle.tenant_id.clone(),
            bundle.session_id.clone(),
        ),
    )?;

    // Write all events from the bundle
    for event in bundle.events {
        writer.write_event(event)?;
    }

    writer.finalize()?;
    Ok(())
}

/// Atomically write a trace bundle by writing to a temporary file and renaming
pub fn write_trace_bundle_atomic<P: AsRef<Path>>(path: P, bundle: TraceBundle) -> Result<()> {
    let path_ref = path.as_ref();
    // Simple tmp suffix
    use std::ffi::OsString;
    let tmp = {
        let mut s = OsString::from(path_ref.as_os_str());
        s.push(".tmp");
        s
    };
    let tmp_path = std::path::PathBuf::from(tmp);

    write_trace_bundle(&tmp_path, bundle)?;
    std::fs::rename(&tmp_path, path_ref).map_err(|e| {
        AosError::Telemetry(format!(
            "Failed to rename {} -> {}: {}",
            tmp_path.display(),
            path_ref.display(),
            e
        ))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logical_clock::LogicalClock;
    use crate::schema::TraceBundle;
    use tempfile::TempDir;

    #[test]
    fn test_trace_writer_creation() {
        let temp_dir = TempDir::new().unwrap();
        let trace_path = temp_dir.path().join("test_trace.ndjson");

        let bundle = TraceBundle::new(
            adapteros_core::B3Hash::hash(b"test_seed"),
            "test_plan".to_string(),
            "test_cpid".to_string(),
            "test_tenant".to_string(),
            "test_session".to_string(),
        );

        let writer = TraceWriter::new(&trace_path, bundle);
        assert!(writer.is_ok());
    }

    #[test]
    fn test_write_event() {
        let temp_dir = TempDir::new().unwrap();
        let trace_path = temp_dir.path().join("test_trace.ndjson");

        let bundle = TraceBundle::new(
            adapteros_core::B3Hash::hash(b"test_seed"),
            "test_plan".to_string(),
            "test_cpid".to_string(),
            "test_tenant".to_string(),
            "test_session".to_string(),
        );

        let mut writer = TraceWriter::new(&trace_path, bundle).unwrap();

        let clock = LogicalClock::new(adapteros_core::B3Hash::hash(b"test_seed"));
        let event = crate::events::inference_start_event(
            1,
            "test_plan".to_string(),
            "test_cpid".to_string(),
            "test_tenant".to_string(),
            "test_session".to_string(),
            adapteros_core::B3Hash::hash(b"test_seed"),
            &clock,
        )
        .unwrap();

        let result = writer.write_event(event);
        assert!(result.is_ok());

        let final_bundle = writer.finalize().unwrap();
        assert_eq!(final_bundle.events.len(), 1);
    }
}
