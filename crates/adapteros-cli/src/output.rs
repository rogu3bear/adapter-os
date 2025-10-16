//! Output formatting utilities with CI detection

use std::env;
use std::fmt;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use tokio::runtime::{Handle, Runtime};

type SharedWriter = Arc<Mutex<Box<dyn Write + Send + 'static>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Text,
    Json,
    Quiet,
}

impl OutputMode {
    #[allow(dead_code)]
    pub fn from_env() -> Self {
        if is_ci() {
            Self::Quiet
        } else {
            Self::Text
        }
    }
    pub fn from_flags(json: bool, quiet: bool) -> Self {
        if json {
            Self::Json
        } else if quiet {
            Self::Quiet
        } else {
            Self::Text
        }
    }
    pub fn is_verbose(&self) -> bool {
        matches!(self, Self::Text)
    }
    pub fn is_quiet(&self) -> bool {
        matches!(self, Self::Quiet)
    }
    pub fn is_json(&self) -> bool {
        matches!(self, Self::Json)
    }
}

pub struct OutputWriter {
    mode: OutputMode,
    verbose: bool,
    command: Option<String>,
    tenant: Option<String>,
    stdout: SharedWriter,
    stderr: SharedWriter,
}

impl fmt::Debug for OutputWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutputWriter")
            .field("mode", &self.mode)
            .field("verbose", &self.verbose)
            .field("command", &self.command)
            .field("tenant", &self.tenant)
            .finish()
    }
}

impl Clone for OutputWriter {
    fn clone(&self) -> Self {
        Self {
            mode: self.mode,
            verbose: self.verbose,
            command: self.command.clone(),
            tenant: self.tenant.clone(),
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
        }
    }
}

#[rustfmt::skip]
impl OutputWriter {
    pub fn new(mode: OutputMode, verbose: bool) -> Self {
        Self { mode, verbose, command: None, tenant: None, stdout: Arc::new(Mutex::new(Box::new(io::stdout()))), stderr: Arc::new(Mutex::new(Box::new(io::stderr()))) }
    }
    pub fn with_streams(mut self, stdout: Box<dyn Write + Send + 'static>, stderr: Box<dyn Write + Send + 'static>) -> Self {
        self.stdout = Arc::new(Mutex::new(stdout));
        self.stderr = Arc::new(Mutex::new(stderr));
        self
    }
    pub fn with_context(mut self, command: impl Into<String>, tenant: Option<impl Into<String>>) -> Self {
        self.command = Some(command.into());
        self.tenant = tenant.map(|t| t.into());
        self
    }
    pub fn mode(&self) -> OutputMode { self.mode }
    pub fn is_verbose(&self) -> bool { self.verbose || self.mode.is_verbose() }
    pub fn is_quiet(&self) -> bool { self.mode.is_quiet() }
    pub fn is_json(&self) -> bool { self.mode.is_json() }
    fn text_enabled(&self) -> bool { !self.mode.is_quiet() && !self.mode.is_json() }
    pub fn progress(&self, msg: impl AsRef<str>) { if self.is_verbose() && !self.mode.is_json() { self.stdout_line(&format!("  ⏳ {}", msg.as_ref())); } }
    pub fn progress_done(&self, success: bool) { if self.is_verbose() && !self.mode.is_json() { self.stdout_line(if success {"  ✅ Done"} else {"  ❌ Failed"}); } }
    pub fn progress_step(&self, current: usize, total: Option<usize>, msg: impl AsRef<str>) {
        if !self.is_verbose() || self.mode.is_json() { return; }
        let msg = msg.as_ref();
        self.stdout_line(&match total { Some(t) => format!("  ▶️  [{} / {}] {}", current, t, msg), None => format!("  ▶️  [{}] {}", current, msg) });
    }
    pub fn verbose(&self, msg: impl AsRef<str>) { if self.is_verbose() && !self.mode.is_json() { self.stdout_line(&format!("  {}", msg.as_ref())); } }
    pub fn blank(&self) { if self.text_enabled() { self.stdout_line(""); } }
    pub fn success(&self, msg: impl AsRef<str>) { if self.text_enabled() { self.stdout_line(&format!("✓ {}", msg.as_ref())); } }
    pub fn result(&self, msg: impl AsRef<str>) { if !self.mode.is_json() { self.stdout_line(msg.as_ref()); } }
    pub fn error(&self, msg: impl AsRef<str>) { self.stderr_line(&format!("❌ {}", msg.as_ref())); }
    pub fn warning(&self, msg: impl AsRef<str>) { if !self.mode.is_quiet() { self.stderr_line(&format!("⚠️  {}", msg.as_ref())); } }
    pub fn fatal_with_code(&mut self, code: &str, msg: &str) -> ! {
        let event_id = self.emit_cli_error(code, msg);
        self.error(&format!("{} – see: aosctl explain {} (event: {})", msg, code, event_id));
        std::process::exit(20);
    }
    pub fn section(&self, title: impl AsRef<str>) {
        if self.text_enabled() {
            let title = title.as_ref();
            self.stdout_line("");
            self.stdout_line(&format!("🔧 {}", title));
            self.stdout_line(&"─".repeat(title.len() + 3));
        }
    }
    pub fn info(&self, msg: impl AsRef<str>) { if self.text_enabled() { self.stdout_line(&format!("ℹ️  {}", msg.as_ref())); } }
    pub fn kv(&self, key: &str, value: &str) { if self.text_enabled() { self.stdout_line(&format!("  {}: {}", key, value)); } }
    pub fn json<T: serde::Serialize>(&self, data: &T) -> Result<(), serde_json::Error> {
        if self.mode.is_json() { self.stdout_line(&serde_json::to_string_pretty(data)?); }
        Ok(())
    }
    pub fn print(&self, msg: impl AsRef<str>) { if self.text_enabled() { self.stdout_line(msg.as_ref()); } }
    pub fn status(&self, label: impl AsRef<str>, state: Status) {
        if self.text_enabled() {
            let label = label.as_ref();
            let line = match state { Status::Pending => format!("🕓 {}", label), Status::Running => format!("⏳ {}", label), Status::Success => format!("✅ {}", label), Status::Failure => format!("❌ {}", label), Status::Skipped => format!("⏭️  {}", label) };
            self.stdout_line(&line);
        }
    }
    pub fn status_update(&self, label: impl AsRef<str>, detail: impl AsRef<str>) { if self.text_enabled() { self.stdout_line(&format!("  ↪ {}: {}", label.as_ref(), detail.as_ref())); } }
    pub fn table<T: serde::Serialize>(&self, table: &dyn fmt::Display, json_data: Option<&T>) -> Result<(), serde_json::Error> {
        if self.mode.is_json() { if let Some(data) = json_data { self.json(data)?; } return Ok(()); }
        if self.mode.is_quiet() { return Ok(()); }
        self.stdout_block(&table.to_string());
        Ok(())
    }
    fn emit_cli_error(&self, code: &str, msg: &str) -> String {
        let fut = {
            let code = code.to_string();
            let message = msg.to_string();
            let command = self.command.clone().unwrap_or_else(|| "unknown".to_string());
            let tenant = self.tenant.clone();
            async move {
                crate::cli_telemetry::emit_cli_error(Some(code.as_str()), command.as_str(), tenant.as_deref(), message.as_str())
                    .await
                    .unwrap_or_else(|_| "-".to_string())
            }
        };
        if let Ok(handle) = Handle::try_current() { handle.block_on(fut) } else if let Ok(rt) = Runtime::new() { rt.block_on(fut) } else { "-".to_string() }
    }
    fn stdout_line(&self, line: &str) {
        if let Ok(mut writer) = self.stdout.lock() {
            if line.is_empty() { let _ = writeln!(writer); } else { let _ = writeln!(writer, "{}", line); }
            let _ = writer.flush();
        }
    }
    fn stdout_block(&self, block: &str) {
        if let Ok(mut writer) = self.stdout.lock() {
            if block.ends_with('\n') { let _ = write!(writer, "{}", block); } else { let _ = writeln!(writer, "{}", block); }
            let _ = writer.flush();
        }
    }
    fn stderr_line(&self, line: &str) {
        if let Ok(mut writer) = self.stderr.lock() {
            let _ = writeln!(writer, "{}", line);
            let _ = writer.flush();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Running,
    Success,
    Failure,
    Skipped,
}

#[allow(dead_code)]
pub fn is_ci() -> bool {
    env::var("CI")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
        || env::var("GITHUB_ACTIONS").is_ok()
        || env::var("JENKINS_URL").is_ok()
        || env::var("CIRCLECI").is_ok()
        || env::var("TRAVIS").is_ok()
        || env::var("GITLAB_CI").is_ok()
        || env::var("BUILDKITE").is_ok()
}

pub fn command_header(mode: &OutputMode, title: &str) {
    if !mode.is_quiet() && !mode.is_json() {
        println!("\n🔧 {}", title);
        println!("{}", "─".repeat(title.len() + 3));
    }
}

pub fn progress(mode: &OutputMode, msg: &str) {
    if mode.is_verbose() && !mode.is_json() {
        println!("  {}", msg);
    }
}

#[rustfmt::skip]
#[cfg(test)]
mod tests {
    use super::*; use std::sync::Arc; use tempfile::tempdir;

    #[derive(Clone)] struct BufferWriter { inner: Arc<Mutex<Vec<u8>>> }
    impl BufferWriter { fn new(inner: Arc<Mutex<Vec<u8>>>) -> Self { Self { inner } } }
    impl Write for BufferWriter { fn write(&mut self, buf: &[u8]) -> io::Result<usize> { self.inner.lock().unwrap().extend_from_slice(buf); Ok(buf.len()) } fn flush(&mut self) -> io::Result<()> { Ok(()) } }

    fn harness(mode: OutputMode, verbose: bool) -> (OutputWriter, Arc<Mutex<Vec<u8>>>, Arc<Mutex<Vec<u8>>>) {
        let out = Arc::new(Mutex::new(Vec::new())); let err = Arc::new(Mutex::new(Vec::new()));
        let writer = OutputWriter::new(mode, verbose).with_streams(Box::new(BufferWriter::new(out.clone())), Box::new(BufferWriter::new(err.clone())));
        (writer, out, err)
    }

    #[test]
    fn modes_and_ci_detection() { assert!(OutputMode::Text.is_verbose()); assert!(OutputMode::Quiet.is_quiet()); assert!(OutputMode::Json.is_json()); let _ = is_ci(); }

    #[test]
    fn table_and_json_rendering() {
        struct T; impl fmt::Display for T { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { writeln!(f, "H")?; write!(f, "row") } }
        let (writer, out, _) = harness(OutputMode::Text, false); writer.table(&T, None).unwrap();
        let text = String::from_utf8(out.lock().unwrap().clone()).unwrap(); assert!(text.contains("H"));
        #[derive(serde::Serialize)] struct Row { id: u8 }
        let (writer, out, _) = harness(OutputMode::Json, false); writer.table(&"irrelevant".to_string(), Some(&vec![Row { id: 7 }])).unwrap();
        assert!(String::from_utf8(out.lock().unwrap().clone()).unwrap().contains("\"id\""));
    }

    #[test]
    fn status_and_progress_helpers() {
        let (writer, out, err) = harness(OutputMode::Text, true);
        writer.status("Download", Status::Running); writer.status_update("bytes", "42"); writer.progress("step");
        writer.progress_step(1, Some(3), "stage"); writer.progress_done(true); writer.warning("careful");
        let stdout = String::from_utf8(out.lock().unwrap().clone()).unwrap(); let stderr = String::from_utf8(err.lock().unwrap().clone()).unwrap();
        assert!(stdout.contains("⏳ Download")); assert!(stdout.contains("[1 / 3] stage")); assert!(stdout.contains("✅")); assert!(stderr.contains("⚠"));
    }

    #[test]
    fn telemetry_integration_emits_event() {
        let dir = tempdir().unwrap(); std::env::set_var("AOS_TELEMETRY_DIR", dir.path());
        let mut writer = OutputWriter::new(OutputMode::Text, false).with_context("unit-test", Some("tenant"));
        let id = writer.emit_cli_error("E1234", "boom"); assert_ne!(id, "-");
        let log = std::fs::read_to_string(dir.path().join("cli_errors.jsonl")).unwrap(); assert!(log.contains(&id));
        std::env::remove_var("AOS_TELEMETRY_DIR");
    }
}
