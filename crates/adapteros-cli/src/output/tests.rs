use super::*;
use std::{
    fmt,
    io::{self, Write},
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
struct TestBuffer(Arc<Mutex<Vec<u8>>>);

impl TestBuffer {
    fn new() -> (Self, Arc<Mutex<Vec<u8>>>) {
        let inner = Arc::new(Mutex::new(Vec::new()));
        (Self(Arc::clone(&inner)), inner)
    }

    fn contents(buf: &Arc<Mutex<Vec<u8>>>) -> String {
        let data = buf.lock().unwrap();
        String::from_utf8_lossy(&data).trim_end().to_string()
    }
}

impl Write for TestBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut data = self.0.lock().unwrap();
        data.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn test_ci_detection() {
    let _ = is_ci();
}

#[test]
fn test_output_modes() {
    let text = OutputMode::Text;
    assert!(text.is_verbose());
    assert!(!text.is_quiet());
    assert!(!text.is_json());

    let quiet = OutputMode::Quiet;
    assert!(!quiet.is_verbose());
    assert!(quiet.is_quiet());
    assert!(!quiet.is_json());

    let json = OutputMode::Json;
    assert!(!json.is_verbose());
    assert!(!json.is_quiet());
    assert!(json.is_json());
}

#[test]
fn test_table_text_output() {
    let (stdout_writer, stdout_buf) = TestBuffer::new();
    let (stderr_writer, _) = TestBuffer::new();
    let output = OutputWriter::with_streams(OutputMode::Text, false, stdout_writer, stderr_writer);

    struct DummyTable;
    impl fmt::Display for DummyTable {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            writeln!(f, "| a | b |")?;
            write!(f, "| 1 | 2 |")
        }
    }

    output.table(&DummyTable, None).unwrap();
    let contents = TestBuffer::contents(&stdout_buf);
    assert!(contents.contains("| a | b |"));
    assert!(contents.contains("| 1 | 2 |"));
}

#[test]
fn test_table_json_output() {
    let (stdout_writer, stdout_buf) = TestBuffer::new();
    let (stderr_writer, _) = TestBuffer::new();
    let output = OutputWriter::with_streams(OutputMode::Json, false, stdout_writer, stderr_writer);

    #[derive(serde::Serialize)]
    struct Row {
        id: u32,
    }

    output
        .table(&"ignored", Some(&vec![Row { id: 1 }]))
        .unwrap();
    let contents = TestBuffer::contents(&stdout_buf);
    assert!(contents.contains("\"id\""));
}

#[test]
fn test_status_helpers() {
    let (stdout_writer, stdout_buf) = TestBuffer::new();
    let (stderr_writer, _) = TestBuffer::new();
    let output = OutputWriter::with_streams(OutputMode::Text, true, stdout_writer, stderr_writer);

    output.status("doing work", StatusLine::InProgress);
    output.progress_with_status(2, 4, "halfway");
    output.status("done", StatusLine::Success);

    let contents = TestBuffer::contents(&stdout_buf);
    assert!(contents.contains("… doing work"));
    assert!(contents.contains("[ 50%] halfway"));
    assert!(contents.contains("✓ done"));
}

#[test]
fn test_bullet_list() {
    let (stdout_writer, stdout_buf) = TestBuffer::new();
    let (stderr_writer, _) = TestBuffer::new();
    let output = OutputWriter::with_streams(OutputMode::Text, false, stdout_writer, stderr_writer);

    output.bullet_list(["alpha", "beta"]);
    let contents = TestBuffer::contents(&stdout_buf);
    assert!(contents.contains("• alpha"));
    assert!(contents.contains("• beta"));
}

#[test]
fn test_warning_and_error_routes() {
    let (stdout_writer, _) = TestBuffer::new();
    let (stderr_writer, stderr_buf) = TestBuffer::new();
    let output = OutputWriter::with_streams(OutputMode::Text, false, stdout_writer, stderr_writer);

    output.warning("careful");
    output.error("boom");
    let contents = TestBuffer::contents(&stderr_buf);
    assert!(contents.contains("⚠️  careful"));
    assert!(contents.contains("❌ boom"));
}

#[test]
fn test_emit_cli_error_generates_event() {
    let (stdout_writer, _) = TestBuffer::new();
    let (stderr_writer, _) = TestBuffer::new();
    let mut output =
        OutputWriter::with_streams(OutputMode::Text, false, stdout_writer, stderr_writer);
    output.set_command("test");
    output.set_tenant(Some("tenant"));

    let event_id = output.emit_cli_error("E1000", "unit test failure");
    assert!(!event_id.is_empty());
}
