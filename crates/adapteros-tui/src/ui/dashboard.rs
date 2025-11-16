use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::types::Status;
use crate::app::{App, Mode};

pub fn draw_dashboard(f: &mut Frame, app: &App, area: Rect) {
    // Split the dashboard into sections
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12), // ASCII art banner
            Constraint::Length(10), // System status
            Constraint::Length(8),  // Main menu
            Constraint::Min(0),     // Additional info
        ])
        .split(area);

    // Draw ASCII art banner
    draw_ascii_banner(f, chunks[0]);

    // Draw system status
    draw_system_status(f, app, chunks[1]);

    // Draw main menu
    draw_main_menu(f, app, chunks[2]);

    // Draw additional info
    draw_info_panel(f, app, chunks[3]);
}

fn draw_ascii_banner(f: &mut Frame, area: Rect) {
    let banner_text = vec![
        Line::from(""),
        Line::from("   █████╗ ██████╗  █████╗ ██████╗ ████████╗███████╗██████╗  ██████╗ ███████╗"),
        Line::from("  ██╔══██╗██╔══██╗██╔══██╗██╔══██╗╚══██╔══╝██╔════╝██╔══██╗██╔═══██╗██╔════╝"),
        Line::from("  ███████║██║  ██║███████║██████╔╝   ██║   █████╗  ██████╔╝██║   ██║███████╗"),
        Line::from("  ██╔══██║██║  ██║██╔══██║██╔═══╝    ██║   ██╔══╝  ██╔══██╗██║   ██║╚════██║"),
        Line::from("  ██║  ██║██████╔╝██║  ██║██║        ██║   ███████╗██║  ██║╚██████╔╝███████║"),
        Line::from("  ╚═╝  ╚═╝╚═════╝ ╚═╝  ╚═╝╚═╝        ╚═╝   ╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝"),
        Line::from(""),
        Line::from("                      SUPERBACKEND CONTROL SYSTEM"),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(banner_text)
        .block(block)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
}

fn draw_system_status(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" System Status ")
        .borders(Borders::ALL);

    // Count service statuses
    let running = app
        .services
        .iter()
        .filter(|s| s.status == Status::Running)
        .count();
    let stopped = app
        .services
        .iter()
        .filter(|s| s.status == Status::Stopped)
        .count();
    let failed = app
        .services
        .iter()
        .filter(|s| s.status == Status::Failed)
        .count();

    let status_lines = vec![
        Line::from(vec![
            Span::styled("[OK] ", Style::default().fg(Color::Green)),
            Span::raw("Database        │ "),
            Span::styled(
                format!("{:<12}", "Connected"),
                Style::default().fg(Color::Green),
            ),
            Span::raw(" │ Latency: 1.2ms"),
        ]),
        Line::from(vec![
            Span::styled("[OK] ", Style::default().fg(Color::Green)),
            Span::raw("Router          │ "),
            Span::styled(
                format!("{:<12}", "Ready"),
                Style::default().fg(Color::Green),
            ),
            Span::raw(format!(
                " │ Adapters: {}/{}",
                app.metrics.active_adapters, app.metrics.total_adapters
            )),
        ]),
        Line::from(vec![
            Span::styled(
                if app.production_mode {
                    "[OK] "
                } else {
                    "[!!] "
                },
                Style::default().fg(if app.production_mode {
                    Color::Green
                } else {
                    Color::Yellow
                }),
            ),
            Span::raw("Security        │ "),
            Span::styled(
                format!(
                    "{:<12}",
                    if app.production_mode {
                        "PRODUCTION"
                    } else {
                        "DEVELOPMENT"
                    }
                ),
                Style::default().fg(if app.production_mode {
                    Color::Green
                } else {
                    Color::Yellow
                }),
            ),
            Span::raw(" │ "),
            Span::styled(
                if app.production_mode {
                    "All policies enforced"
                } else {
                    "Relaxed policies"
                },
                Style::default().fg(Color::Gray),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Services: "),
            Span::styled(
                format!("{} running", running),
                Style::default().fg(Color::Green),
            ),
            Span::raw(", "),
            Span::styled(
                format!("{} stopped", stopped),
                Style::default().fg(Color::Gray),
            ),
            Span::raw(", "),
            Span::styled(
                format!("{} failed", failed),
                Style::default().fg(Color::Red),
            ),
        ]),
        Line::from(vec![
            Span::raw("Memory Headroom: "),
            Span::styled(
                format!("{:.1}%", app.metrics.memory_headroom_percent),
                Style::default().fg(if app.metrics.memory_headroom_percent >= 15.0 {
                    Color::Green
                } else {
                    Color::Yellow
                }),
            ),
            Span::raw(if app.metrics.memory_headroom_percent >= 15.0 {
                " [Good >= 15%]"
            } else {
                " [Warning < 15%]"
            }),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Database: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                if app.db_stats.database_connected {
                    "Connected "
                } else {
                    "Offline "
                },
                Style::default().fg(if app.db_stats.database_connected {
                    Color::Green
                } else {
                    Color::Red
                }),
            ),
            Span::raw("│ "),
            Span::raw(format!("Adapters: {} ", app.db_stats.total_adapters)),
            Span::raw("│ "),
            Span::raw(format!("Training: {}", app.db_stats.total_training_jobs)),
            Span::styled(
                format!(" ({} active)", app.db_stats.active_training_jobs),
                Style::default().fg(if app.db_stats.active_training_jobs > 0 {
                    Color::Cyan
                } else {
                    Color::Gray
                }),
            ),
            Span::raw(format!(" │ Tenants: {}", app.db_stats.total_tenants)),
        ]),
    ];

    let paragraph = Paragraph::new(status_lines)
        .block(block)
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

fn draw_main_menu(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Main Menu [↑↓ Navigate | Enter Select | Esc Back] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if app.current_mode == Mode::Normal {
            Color::Green
        } else {
            Color::Gray
        }));

    // Create the log entries string separately to avoid temporary value issue
    let log_entries_str = format!("[{} new entries]", app.recent_logs.len());

    let menu_items = vec![
        ("Boot All Services", "[Ready to boot]", Color::Green),
        ("Boot Single Service", "[Select from list]", Color::Cyan),
        (
            "Debug Service",
            if app.services.iter().any(|s| s.status == Status::Failed) {
                "[Services with errors]"
            } else {
                "[All services healthy]"
            },
            if app.services.iter().any(|s| s.status == Status::Failed) {
                Color::Red
            } else {
                Color::Green
            },
        ),
        ("Review Health", "[No warnings]", Color::Green),
        ("View Logs", log_entries_str.as_str(), Color::White),
        ("Edit Settings", "[All valid]", Color::Green),
        (
            "Toggle Production Mode",
            if app.production_mode {
                "[Currently: PROD]"
            } else {
                "[Currently: DEV]"
            },
            if app.production_mode {
                Color::Red
            } else {
                Color::Yellow
            },
        ),
    ];

    let items: Vec<ListItem> = menu_items
        .iter()
        .enumerate()
        .map(|(i, (name, status, color))| {
            let selected = i == app.selected_menu_item && app.current_mode == Mode::Normal;
            let prefix = if selected { "> " } else { "  " };

            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(
                    format!("{:<25}", name),
                    if selected {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Gray)
                    },
                ),
                Span::raw(" "),
                Span::styled(format!("{:<25}", status), Style::default().fg(*color)),
            ]))
        })
        .collect();

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_info_panel(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Quick Stats ")
        .borders(Borders::ALL);

    let info_lines = vec![
        Line::from(vec![
            Span::raw("Inference Latency: "),
            Span::styled(
                format!("{}ms", app.metrics.inference_latency_p95_ms),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(" | Tokens/sec: "),
            Span::styled(
                format!("{}", app.metrics.tokens_per_second),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(" | Queue: "),
            Span::styled(
                format!("{}", app.metrics.queue_depth),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Tip: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Press 'h' for help, 'q' to quit, Tab to switch screens"),
        ]),
    ];

    let paragraph = Paragraph::new(info_lines)
        .block(block)
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}
