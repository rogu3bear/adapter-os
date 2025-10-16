use crate::cli_telemetry;
use std::{
    env, fmt,
    io::{self, Write},
    sync::{Arc, Mutex},
};
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

    pub fn is_verbose(self) -> bool {
        matches!(self, Self::Text)
    }
    pub fn is_quiet(self) -> bool {
        matches!(self, Self::Quiet)
    }
    pub fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLine {
    InProgress,
    Success,
    Failure,
    Skipped,
}

impl StatusLine {
    fn icon(self) -> &'static str {
        match self {
            Self::InProgress => "…",
            Self::Success => "✓",
            Self::Failure => "✗",
            Self::Skipped => "⏭",
        }
    }
}

pub struct OutputWriter {
    mode: OutputMode,
    verbose: bool,
    command: Option<String>,
    tenant: Option<String>,
    stdout: Arc<Mutex<Box<dyn Write + Send>>>,
    stderr: Arc<Mutex<Box<dyn Write + Send>>>,
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
            stdout: Arc::clone(&self.stdout),
            stderr: Arc::clone(&self.stderr),
        }
    }
}

impl OutputWriter {
    pub fn new(mode: OutputMode, verbose: bool) -> Self {
        Self::with_streams(mode, verbose, io::stdout(), io::stderr())
    }
    pub fn with_streams<O, E>(mode: OutputMode, verbose: bool, stdout: O, stderr: E) -> Self
    where
        O: Write + Send + 'static,
        E: Write + Send + 'static,
    {
        Self {
            mode,
            verbose,
            command: None,
            tenant: None,
            stdout: Arc::new(Mutex::new(Box::new(stdout))),
            stderr: Arc::new(Mutex::new(Box::new(stderr))),
        }
    }
    pub fn set_command(&mut self, command: impl Into<String>) {
        self.command = Some(command.into());
    }
    pub fn set_tenant(&mut self, tenant: Option<impl Into<String>>) {
        self.tenant = tenant.map(Into::into);
    }
    pub fn mode(&self) -> OutputMode {
        self.mode
    }
    pub fn is_verbose(&self) -> bool {
        self.verbose || self.mode.is_verbose()
    }
    pub fn is_quiet(&self) -> bool {
        self.mode.is_quiet()
    }
    pub fn is_json(&self) -> bool {
        self.mode.is_json()
    }
    pub fn progress(&self, msg: impl AsRef<str>) {
        self.emit_verbose(format!("  {}", msg.as_ref()));
    }

    pub fn progress_done(&self, success: bool) {
        self.emit_verbose(if success {
            "  ✓ Done"
        } else {
            "  ✗ Failed"
        });
    }

    pub fn verbose(&self, msg: impl AsRef<str>) {
        self.emit_verbose(format!("  {}", msg.as_ref()));
    }

    pub fn blank(&self) {
        self.emit_text("");
    }

    pub fn success(&self, msg: impl AsRef<str>) {
        self.emit_text(format!("✓ {}", msg.as_ref()));
    }
    pub fn result(&self, msg: impl AsRef<str>) {
        if !self.is_json() {
            self.write_stdout(msg);
        }
    }

    pub fn error(&self, msg: impl AsRef<str>) {
        self.write_stderr(format!("❌ {}", msg.as_ref()));
    }

    pub fn warning(&self, msg: impl AsRef<str>) {
        if !self.is_quiet() {
            self.write_stderr(format!("⚠️  {}", msg.as_ref()));
        }
    }
    pub fn fatal_with_code(&mut self, code: &str, msg: &str) -> ! {
        let event_id = self.emit_cli_error(code, msg);
        self.error(format!(
            "{} – see: aosctl explain {} (event: {})",
            msg, code, event_id
        ));
        std::process::exit(20);
    }

    fn emit_cli_error(&self, code: &str, msg: &str) -> String {
        let command = self.command.as_deref().unwrap_or("unknown");
        let tenant = self.tenant.as_deref();
        let fut = cli_telemetry::emit_cli_error(Some(code), command, tenant, msg);

        let handle_result = |res: anyhow::Result<String>| {
            res.unwrap_or_else(|err| {
                tracing::error!(error = %err, "failed to emit telemetry");
                "-".to_string()
            })
        };

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle.block_on(async { handle_result(fut.await) }),
            Err(_) => tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map(|rt| rt.block_on(async { handle_result(fut.await) }))
                .unwrap_or_else(|err| {
                    tracing::error!(error = %err, "failed to create telemetry runtime");
                    "-".to_string()
                }),
        }
    }

    pub fn section(&self, title: impl AsRef<str>) {
        if !self.mode.is_quiet() && !self.mode.is_json() {
            let title = title.as_ref();
            self.write_stdout("");
            self.write_stdout(format!("🔧 {}", title));
            self.write_stdout("─".repeat(title.len() + 3));
        }
    }
    pub fn info(&self, msg: impl AsRef<str>) {
        self.emit_text(format!("ℹ️  {}", msg.as_ref()));
    }
    pub fn kv(&self, key: &str, value: &str) {
        self.emit_text(format!("  {}: {}", key, value));
    }
    pub fn json<T: serde::Serialize>(&self, data: &T) -> Result<(), serde_json::Error> {
        if self.is_json() {
            self.write_stdout(serde_json::to_string_pretty(data)?);
        }
        Ok(())
    }
    pub fn print(&self, msg: impl AsRef<str>) {
        self.emit_text(msg);
    }

    pub fn table<T: serde::Serialize>(
        &self,
        table: &dyn fmt::Display,
        json_data: Option<&T>,
    ) -> Result<(), serde_json::Error> {
        if let Some(data) = json_data {
            self.json(data)?;
        } else if !self.mode.is_quiet() && !self.mode.is_json() {
            self.write_stdout(table.to_string());
        }
        Ok(())
    }

    pub fn status(&self, label: impl AsRef<str>, state: StatusLine) {
        self.emit_text(format!("{} {}", state.icon(), label.as_ref()));
    }

    pub fn progress_with_status(&self, current: usize, total: usize, label: impl AsRef<str>) {
        if self.is_verbose() && !self.mode.is_json() && total > 0 {
            let pct = ((current * 100) / total).min(100);
            self.emit_verbose(format!("  [{:>3}%] {}", pct, label.as_ref()));
        }
    }

    pub fn bullet_list<I, S>(&self, items: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        if !self.mode.is_quiet() && !self.mode.is_json() {
            for item in items {
                self.write_stdout(format!("  • {}", item.as_ref()));
            }
        }
    }

    fn emit_text<S: AsRef<str>>(&self, msg: S) {
        if !self.mode.is_quiet() && !self.mode.is_json() {
            self.write_stdout(msg);
        }
    }

    fn emit_verbose<S: AsRef<str>>(&self, msg: S) {
        if self.is_verbose() && !self.mode.is_json() {
            self.write_stdout(msg);
        }
    }

    fn write_stdout<S: AsRef<str>>(&self, msg: S) {
        if let Ok(mut guard) = self.stdout.lock() {
            let _ = writeln!(guard, "{}", msg.as_ref());
        }
    }
    fn write_stderr<S: AsRef<str>>(&self, msg: S) {
        if let Ok(mut guard) = self.stderr.lock() {
            let _ = writeln!(guard, "{}", msg.as_ref());
        }
    }
}

#[allow(dead_code)]
pub fn is_ci() -> bool {
    env::var("CI")
        .map(|v| matches!(v.as_str(), "true" | "1"))
        .unwrap_or(false)
        || [
            "GITHUB_ACTIONS",
            "JENKINS_URL",
            "CIRCLECI",
            "TRAVIS",
            "GITLAB_CI",
            "BUILDKITE",
        ]
        .into_iter()
        .any(|key| env::var(key).is_ok())
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

#[cfg(test)]
mod tests;
