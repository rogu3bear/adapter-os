use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;

pub fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    // Split status bar into static and live sections
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(65), // Static info
            Constraint::Percentage(35), // Live data
        ])
        .split(area);

    draw_static_status(f, app, chunks[0]);
    draw_live_data(f, app, chunks[1]);
}

fn draw_static_status(f: &mut Frame, app: &App, area: Rect) {
    let model_status = if app.model_status.loaded {
        "[OK] LOADED"
    } else {
        "[--] NOT LOADED"
    };

    let mode = if app.production_mode {
        "PROD"
    } else {
        "DEV " // Extra space to make it 4 chars
    };

    // Column widths defined for reference (using inline formatting instead)

    let status_content = vec![Line::from(vec![
        Span::raw(format!(" Model: {:<22} │ ", app.model_status.name)),
        Span::styled(
            format!("Status: {:<15}", model_status),
            Style::default().fg(if app.model_status.loaded {
                Color::Green
            } else {
                Color::Gray
            }),
        ),
        Span::raw(" │ "),
        Span::styled(
            format!("Mode: {:<4}", mode),
            Style::default()
                .fg(if app.production_mode {
                    Color::Red
                } else {
                    Color::Yellow
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ])];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White));

    let paragraph = Paragraph::new(status_content)
        .block(block)
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

fn draw_live_data(f: &mut Frame, app: &App, area: Rect) {
    let memory_percent = if app.model_status.total_memory_mb > 0 {
        (app.model_status.memory_usage_mb * 100) / app.model_status.total_memory_mb
    } else {
        0
    };

    // Create a pulsing border effect for live data (changes color based on update)
    let border_color = if app.metrics.queue_depth > 10 || memory_percent > 85 {
        Color::Yellow
    } else {
        Color::Green
    };

    let live_content = vec![Line::from(vec![
        Span::styled(
            " ▣ LIVE ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw("│ "),
        Span::raw("Mem: "),
        Span::styled(
            format!("{:>3}%", memory_percent),
            Style::default()
                .fg(if memory_percent > 85 {
                    Color::Red
                } else if memory_percent > 70 {
                    Color::Yellow
                } else {
                    Color::Green
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" │ "),
        Span::raw("Q: "),
        Span::styled(
            format!("{:>2}", app.metrics.queue_depth),
            Style::default()
                .fg(if app.metrics.queue_depth > 10 {
                    Color::Yellow
                } else {
                    Color::Green
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" │ "),
        Span::raw("TPS: "),
        Span::styled(
            format!("{:>4}", app.metrics.tokens_per_second),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ])];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        )
        .title(" ⟳ 1s ");

    let paragraph = Paragraph::new(live_content)
        .block(block)
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}
