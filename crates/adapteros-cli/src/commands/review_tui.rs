use crate::output::OutputWriter;
use adapteros_core::Result;
use adapteros_lora_worker::active_learning::{AbstainSampleRecord, GoldenCandidate};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fs, io};
use tracing::{error, info};

pub async fn run_review_tui(dir: Option<PathBuf>, output: &OutputWriter) -> Result<()> {
    let dir = dir.unwrap_or_else(|| adapteros_core::rebase_var_path("var/active_learning"));
    let queue_path = dir.join("abstain_queue.ndjson");

    if !queue_path.exists() {
        output.error(format!(
            "Active learning queue not found at {:?}",
            queue_path
        ));
        return Ok(());
    }

    // Load samples
    let samples = load_samples(&queue_path)?;
    if samples.is_empty() {
        output.success("No samples to review");
        return Ok(());
    }

    // Setup terminal
    enable_raw_mode().map_err(|e| {
        adapteros_core::AosError::internal(format!("Failed to enable raw mode: {}", e))
    })?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).map_err(|e| {
        adapteros_core::AosError::internal(format!("Failed to setup terminal: {}", e))
    })?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| {
        adapteros_core::AosError::internal(format!("Failed to create terminal: {}", e))
    })?;

    let mut app = ReviewApp::new(samples, dir);
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .ok();
    terminal.show_cursor().ok();

    if let Err(e) = res {
        output.error(format!("TUI error: {}", e));
    } else {
        output.success("Review session complete");
    }

    Ok(())
}

struct ReviewApp {
    samples: Vec<AbstainSampleRecord>,
    directory: PathBuf,
    current_index: usize,
    should_quit: bool,
    promoted_count: usize,
    discarded_count: usize,
}

impl ReviewApp {
    fn new(samples: Vec<AbstainSampleRecord>, directory: PathBuf) -> Self {
        Self {
            samples,
            directory,
            current_index: 0,
            should_quit: false,
            promoted_count: 0,
            discarded_count: 0,
        }
    }

    fn current_sample(&self) -> Option<&AbstainSampleRecord> {
        self.samples.get(self.current_index)
    }

    fn next(&mut self) {
        if self.current_index + 1 < self.samples.len() {
            self.current_index += 1;
        }
    }

    fn previous(&mut self) {
        if self.current_index > 0 {
            self.current_index -= 1;
        }
    }

    async fn promote_current(&mut self) -> Result<()> {
        if let Some(sample) = self.samples.get(self.current_index) {
            // Logic to promote to golden dataset
            let golden_path = self.directory.join("golden_promoted.jsonl");
            let candidate = GoldenCandidate {
                sample_id: sample.id.clone(),
                prompt: sample.prompt.clone(),
                prompt_digest_b3: sample.prompt_digest_b3.clone(),
                reason: sample.reason.clone(),
                confidence: sample.confidence,
                entropy: sample.entropy,
                status: "approved".to_string(),
            };

            let line = serde_json::to_string(&candidate)
                .map_err(adapteros_core::AosError::Serialization)?;
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&golden_path)?;

            use std::io::Write;
            file.write_all(line.as_bytes())?;
            file.write_all(b"\n")?;

            self.promoted_count += 1;
            self.samples.remove(self.current_index);
            if self.current_index >= self.samples.len() && !self.samples.is_empty() {
                self.current_index = self.samples.len() - 1;
            }
        }
        Ok(())
    }

    fn discard_current(&mut self) {
        if !self.samples.is_empty() {
            self.discarded_count += 1;
            self.samples.remove(self.current_index);
            if self.current_index >= self.samples.len() && !self.samples.is_empty() {
                self.current_index = self.samples.len() - 1;
            }
        }
    }
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut ReviewApp,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app)).map_err(|e| {
            adapteros_core::AosError::internal(format!("Terminal draw error: {}", e))
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
                    return Ok(());
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('n') | KeyCode::Right => app.next(),
                    KeyCode::Char('p') | KeyCode::Left => app.previous(),
                    KeyCode::Char('u') | KeyCode::Char('y') => {
                        app.promote_current().await?;
                        if app.samples.is_empty() {
                            return Ok(());
                        }
                    }
                    KeyCode::Char('d') | KeyCode::Char('x') => {
                        app.discard_current();
                        if app.samples.is_empty() {
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &ReviewApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Title
    let title = Paragraph::new("adapterOS Active Learning Review")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Content
    if let Some(sample) = app.current_sample() {
        let sample_info = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(6), Constraint::Min(10)])
            .split(chunks[1]);

        let meta_lines = vec![
            Line::from(vec![
                Span::raw("ID: "),
                Span::styled(&sample.id, Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::raw("Reason: "),
                Span::styled(&sample.reason, Style::default().fg(Color::Magenta)),
            ]),
            Line::from(vec![
                Span::raw("Confidence: "),
                Span::styled(
                    format!("{:.4}", sample.confidence),
                    Style::default().fg(Color::Green),
                ),
            ]),
            Line::from(vec![
                Span::raw("Timestamp: "),
                Span::raw(sample.timestamp_us.to_string()),
            ]),
            Line::from(vec![
                Span::raw("Progress: "),
                Span::raw(format!("{}/{}", app.current_index + 1, app.samples.len())),
            ]),
        ];
        let meta = Paragraph::new(meta_lines)
            .block(Block::default().title("Metadata").borders(Borders::ALL));
        f.render_widget(meta, sample_info[0]);

        let prompt = Paragraph::new(sample.prompt.as_deref().unwrap_or("[No Prompt]"))
            .wrap(Wrap { trim: true })
            .block(Block::default().title("Prompt").borders(Borders::ALL));
        f.render_widget(prompt, sample_info[1]);
    } else {
        let empty = Paragraph::new("No more samples to review")
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(empty, chunks[1]);
    }

    // Help
    let help = Paragraph::new("[U]pvote  [D]ownvote  [N]ext  [P]revious  [Q]uit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, chunks[2]);
}

fn load_samples(path: &Path) -> Result<Vec<AbstainSampleRecord>> {
    let content = fs::read_to_string(path)?;
    let mut samples = Vec::new();
    for line in content.lines() {
        if !line.trim().is_empty() {
            let sample: AbstainSampleRecord =
                serde_json::from_str(line).map_err(adapteros_core::AosError::Serialization)?;
            samples.push(sample);
        }
    }
    Ok(samples)
}
