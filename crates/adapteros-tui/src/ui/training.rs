//! Training jobs monitoring screen for the TUI.
//!
//! Displays active, queued, and completed training jobs with progress indicators,
//! loss metrics, and throughput information.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::App;

/// Draw the training jobs screen
pub fn draw_training(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Stats header
            Constraint::Min(10),   // Job list
            Constraint::Length(8), // Selected job detail
        ])
        .split(area);

    draw_training_stats(f, app, chunks[0]);
    draw_training_jobs_list(f, app, chunks[1]);
    draw_selected_job_detail(f, app, chunks[2]);
}

/// Draw the stats header showing job counts by status
fn draw_training_stats(f: &mut Frame, app: &App, area: Rect) {
    let running = app
        .training_jobs
        .iter()
        .filter(|j| j.status == "running")
        .count();
    let queued = app
        .training_jobs
        .iter()
        .filter(|j| j.status == "queued")
        .count();
    let completed = app
        .training_jobs
        .iter()
        .filter(|j| j.status == "completed")
        .count();
    let failed = app
        .training_jobs
        .iter()
        .filter(|j| j.status == "failed")
        .count();

    let stats = Line::from(vec![
        Span::styled("Active: ", Style::default().fg(Color::Gray)),
        Span::styled(
            format!("{}", running),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("Queued: ", Style::default().fg(Color::Gray)),
        Span::styled(format!("{}", queued), Style::default().fg(Color::Yellow)),
        Span::raw("  "),
        Span::styled("Completed: ", Style::default().fg(Color::Gray)),
        Span::styled(format!("{}", completed), Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled("Failed: ", Style::default().fg(Color::Gray)),
        Span::styled(format!("{}", failed), Style::default().fg(Color::Red)),
    ]);

    let block = Block::default()
        .title(" Training Jobs ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(stats).block(block);
    f.render_widget(paragraph, area);
}

/// Draw the list of training jobs
fn draw_training_jobs_list(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().title(" Jobs ").borders(Borders::ALL);

    if app.training_jobs.is_empty() {
        let empty_msg = Paragraph::new(Line::from(Span::styled(
            "No training jobs. Use `aosctl train` to start training.",
            Style::default().fg(Color::Gray),
        )))
        .block(block);
        f.render_widget(empty_msg, area);
        return;
    }

    // Header line
    let header = ListItem::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "JOB                  STATUS    PROGRESS        LOSS    TPS",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
        ),
    ]));

    let separator = ListItem::new(Line::from(
        "  ----------------------------------------------------------------",
    ));

    let mut items = vec![header, separator];

    for (i, job) in app.training_jobs.iter().enumerate() {
        let selected = i == app.selected_training_job;
        let prefix = if selected { "> " } else { "  " };

        // Progress bar
        let progress_bar = create_progress_bar(job.progress_pct, 6);

        // Status color
        let status_style = match job.status.as_str() {
            "running" => Style::default().fg(Color::Green),
            "queued" => Style::default().fg(Color::Yellow),
            "completed" => Style::default().fg(Color::Cyan),
            "failed" => Style::default().fg(Color::Red),
            "paused" => Style::default().fg(Color::Magenta),
            _ => Style::default().fg(Color::Gray),
        };

        let loss_str = if job.current_loss > 0.0 {
            format!("{:.3}", job.current_loss)
        } else {
            "-".to_string()
        };

        let tps_str = if job.tokens_per_second > 0.0 {
            format!("{:.0}", job.tokens_per_second)
        } else {
            "-".to_string()
        };

        let line_style = if selected {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let line = Line::from(vec![
            Span::styled(prefix, line_style),
            Span::styled(format!("{:<20}", truncate_str(&job.id, 18)), line_style),
            Span::styled(format!("{:<10}", job.status.to_uppercase()), status_style),
            Span::raw(progress_bar),
            Span::styled(format!(" {:>3.0}%", job.progress_pct), line_style),
            Span::raw("  "),
            Span::styled(
                format!("{:>6}", loss_str),
                Style::default().fg(Color::Magenta),
            ),
            Span::raw("  "),
            Span::styled(format!("{:>5}", tps_str), Style::default().fg(Color::Cyan)),
        ]);

        items.push(ListItem::new(line));
    }

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

/// Draw details for the selected training job
fn draw_selected_job_detail(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Selected Job ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let content = if let Some(job) = app.training_jobs.get(app.selected_training_job) {
        let epoch_info = if job.total_epochs > 0 {
            format!("Epoch: {}/{}", job.current_epoch, job.total_epochs)
        } else {
            "Epoch: -".to_string()
        };

        let batch_info = if job.total_batches > 0 {
            format!("Batch: {}/{}", job.current_batch, job.total_batches)
        } else {
            "Batch: -".to_string()
        };

        let lr_info = if job.learning_rate > 0.0 {
            format!("LR: {:.6}", job.learning_rate)
        } else {
            "LR: -".to_string()
        };

        let dataset_info = job
            .dataset_name
            .as_ref()
            .map(|name| {
                let samples = job
                    .dataset_samples
                    .map(|s| format!(" ({} samples)", s))
                    .unwrap_or_default();
                format!("Dataset: {}{}", name, samples)
            })
            .unwrap_or_else(|| "Dataset: -".to_string());

        let backend_info = job
            .backend
            .as_ref()
            .map(|b| format!("Backend: {}", b))
            .unwrap_or_else(|| "Backend: -".to_string());

        let started_info = job
            .started_at
            .map(|t| {
                let elapsed = chrono::Utc::now().signed_duration_since(t);
                let mins = elapsed.num_minutes();
                if mins > 0 {
                    format!("Started: {}m ago", mins)
                } else {
                    "Started: just now".to_string()
                }
            })
            .unwrap_or_else(|| "Started: -".to_string());

        let checkpoints_info = format!("Checkpoints: {} saved", job.checkpoints_saved);

        vec![
            Line::from(vec![
                Span::styled("Job ID: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    &job.id,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled(epoch_info, Style::default().fg(Color::White)),
                Span::raw("  |  "),
                Span::styled(batch_info, Style::default().fg(Color::White)),
                Span::raw("  |  "),
                Span::styled(lr_info, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled(dataset_info, Style::default().fg(Color::White)),
                Span::raw("  |  "),
                Span::styled(backend_info, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled(started_info, Style::default().fg(Color::Gray)),
                Span::raw("  |  "),
                Span::styled(checkpoints_info, Style::default().fg(Color::Gray)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "[C]",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("ancel  "),
                Span::styled(
                    "[P]",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("ause  "),
                Span::styled(
                    "[R]",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("esume  "),
                Span::styled(
                    "[L]",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("ogs"),
            ]),
        ]
    } else {
        vec![Line::from(Span::styled(
            "No job selected",
            Style::default().fg(Color::Gray),
        ))]
    };

    let paragraph = Paragraph::new(content).block(block);
    f.render_widget(paragraph, area);
}

/// Creates a progress bar using Unicode block characters
fn create_progress_bar(progress: f32, width: usize) -> String {
    let filled = ((progress / 100.0) * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "\u{2588}".repeat(filled), "\u{2591}".repeat(empty))
}

/// Truncates a string to a maximum length, adding ellipsis if needed
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_progress_bar() {
        assert_eq!(
            create_progress_bar(0.0, 6),
            "\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}"
        );
        assert_eq!(
            create_progress_bar(50.0, 6),
            "\u{2588}\u{2588}\u{2588}\u{2591}\u{2591}\u{2591}"
        );
        assert_eq!(
            create_progress_bar(100.0, 6),
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}"
        );
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("short", 10), "short");
        assert_eq!(truncate_str("this-is-a-long-string", 10), "this-is...");
        assert_eq!(truncate_str("exactly10c", 10), "exactly10c");
    }
}
