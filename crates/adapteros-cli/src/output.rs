use std::env;

use comfy_table::{presets::UTF8_FULL, Attribute, Cell, CellAlignment, Row, Table};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Text,
    Json,
    Quiet,
}

impl OutputMode {
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

#[derive(Debug, Clone)]
pub struct OutputWriter {
    mode: OutputMode,
    verbose: bool,
}

impl OutputWriter {
    pub fn new(mode: OutputMode, verbose: bool) -> Self {
        Self { mode, verbose }
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
    pub fn progress(&self, msg: impl AsRef<str>) {
        if self.is_verbose() && !self.mode.is_json() {
            println!("  {}", msg.as_ref());
        }
    }
    pub fn progress_done(&self, success: bool) {
        if self.is_verbose() && !self.mode.is_json() {
            println!("  {}", if success { "✓ Done" } else { "✗ Failed" });
        }
    }
    pub fn verbose(&self, msg: impl AsRef<str>) {
        if self.is_verbose() && !self.mode.is_json() {
            println!("  {}", msg.as_ref());
        }
    }
    pub fn blank(&self) {
        if !self.mode.is_quiet() && !self.mode.is_json() {
            println!();
        }
    }
    pub fn success(&self, msg: impl AsRef<str>) {
        if !self.mode.is_quiet() && !self.mode.is_json() {
            println!("✓ {}", msg.as_ref());
        }
    }
    pub fn result(&self, msg: impl AsRef<str>) {
        if !self.mode.is_json() {
            println!("{}", msg.as_ref());
        }
    }
    pub fn error(&self, msg: impl AsRef<str>) {
        eprintln!("❌ {}", msg.as_ref());
    }
    pub fn warning(&self, msg: impl AsRef<str>) {
        if !self.mode.is_quiet() {
            eprintln!("⚠️  {}", msg.as_ref());
        }
    }
    pub fn fatal_with_code(&mut self, code: &str, msg: &str) -> ! {
        let event_id = self.emit_cli_error(code, msg);
        self.error(&format!(
            "{} – see: aosctl explain {} (event: {})",
            msg, code, event_id
        ));
        std::process::exit(20);
    }
    fn emit_cli_error(&self, _code: &str, _msg: &str) -> String {
        "-".into()
    }
    pub fn section(&self, title: impl AsRef<str>) {
        let title = title.as_ref();
        if !self.mode.is_quiet() && !self.mode.is_json() {
            println!("\n🔧 {}", title);
            println!("{}", "─".repeat(title.len() + 3));
        }
    }
    pub fn info(&self, msg: impl AsRef<str>) {
        if !self.mode.is_quiet() && !self.mode.is_json() {
            println!("ℹ️  {}", msg.as_ref());
        }
    }
    pub fn kv(&self, key: &str, value: &str) {
        if !self.mode.is_quiet() && !self.mode.is_json() {
            println!("  {}: {}", key, value);
        }
    }
    pub fn is_json(&self) -> bool {
        self.mode.is_json()
    }
    pub fn json<T: serde::Serialize>(&self, data: &T) -> Result<(), serde_json::Error> {
        if self.mode.is_json() {
            println!("{}", serde_json::to_string_pretty(data)?);
        }
        Ok(())
    }
    pub fn print(&self, msg: impl AsRef<str>) {
        if !self.mode.is_quiet() && !self.mode.is_json() {
            println!("{}", msg.as_ref());
        }
    }
    pub fn table_row<R, C>(&self, row: R) -> TableLine
    where
        R: IntoIterator<Item = C>,
        C: Into<TableCell>,
    {
        row.into_iter().map(Into::into).collect()
    }
    pub fn table_header<H, C>(&self, header: H) -> TableLine
    where
        H: IntoIterator<Item = C>,
        C: Into<TableCell>,
    {
        style_line(self.table_row(header), Attribute::Bold)
    }
    pub fn table_footer<F, C>(&self, footer: F) -> TableLine
    where
        F: IntoIterator<Item = C>,
        C: Into<TableCell>,
    {
        style_line(self.table_row(footer), Attribute::Dim)
    }
    pub fn table<H, R, I, F, T>(
        &self,
        header: H,
        rows: R,
        footer: Option<F>,
        json_data: Option<&T>,
    ) -> Result<(), serde_json::Error>
    where
        H: IntoIterator,
        H::Item: Into<TableCell>,
        R: IntoIterator<Item = I>,
        I: IntoIterator,
        I::Item: Into<TableCell>,
        F: IntoIterator,
        F::Item: Into<TableCell>,
        T: serde::Serialize,
    {
        if self.mode.is_json() {
            if let Some(data) = json_data {
                self.json(data)?;
            }
            return Ok(());
        }
        if self.mode.is_quiet() {
            return Ok(());
        }
        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_header(line_to_row(self.table_header(header)));
        for row in rows {
            table.add_row(line_to_row(self.table_row(row)));
        }
        if let Some(footer) = footer {
            table.add_row(line_to_row(self.table_footer(footer)));
        }
        println!("{}", table);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct TableCell {
    pub value: String,
    pub alignment: Option<CellAlignment>,
    pub attrs: Vec<Attribute>,
}

impl TableCell {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            alignment: None,
            attrs: Vec::new(),
        }
    }
    pub fn align(mut self, alignment: CellAlignment) -> Self {
        self.alignment = Some(alignment);
        self
    }
    pub fn attr(mut self, attr: Attribute) -> Self {
        self.attrs.push(attr);
        self
    }
    fn into_cell(self) -> Cell {
        let mut cell = Cell::new(self.value);
        if let Some(alignment) = self.alignment {
            cell = cell.set_alignment(alignment);
        }
        if !self.attrs.is_empty() {
            cell = cell.add_attributes(self.attrs);
        }
        cell
    }
}

impl<T: Into<String>> From<T> for TableCell {
    fn from(value: T) -> Self {
        TableCell::new(value)
    }
}

pub type TableLine = Vec<TableCell>;

fn style_line(mut line: TableLine, attr: Attribute) -> TableLine {
    for cell in &mut line {
        cell.attrs.insert(0, attr);
    }
    line
}

fn line_to_row(line: TableLine) -> Row {
    Row::from(
        line.into_iter()
            .map(TableCell::into_cell)
            .collect::<Vec<_>>(),
    )
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct RowData {
        id: u32,
        name: &'static str,
    }

    #[test]
    fn header_and_footer_style() {
        let writer = OutputWriter::new(OutputMode::Text, false);
        let header = writer.table_header(vec![TableCell::new("ID").align(CellAlignment::Right)]);
        let footer = writer.table_footer(vec!["done"]);
        assert_eq!(header[0].alignment, Some(CellAlignment::Right));
        assert!(header[0].attrs.contains(&Attribute::Bold));
        assert!(footer[0].attrs.contains(&Attribute::Dim));
    }

    #[test]
    fn table_handles_modes() {
        let text = OutputWriter::new(OutputMode::Text, false);
        text.table(
            vec!["ID", "Name"],
            vec![vec!["1", "adapter"]],
            None::<Vec<&str>>,
            None::<&RowData>,
        )
        .unwrap();

        let json = OutputWriter::new(OutputMode::Json, false);
        let data = vec![RowData {
            id: 1,
            name: "adapter",
        }];
        json.table(vec!["ID"], vec![vec!["1"]], None::<Vec<&str>>, Some(&data))
            .unwrap();

        let quiet = OutputWriter::new(OutputMode::Quiet, false);
        quiet
            .table(
                vec!["ID"],
                vec![vec!["1"]],
                None::<Vec<&str>>,
                None::<&RowData>,
            )
            .unwrap();
    }
}
