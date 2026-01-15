use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, Screen};

mod adapters;
mod chat;
mod dashboard;
mod services;
mod status_bar;
mod training;
mod widgets;

use adapters::draw_adapters;
use chat::draw_chat;
use dashboard::draw_dashboard;
use services::draw_services;
use status_bar::draw_status_bar;
use training::draw_training;

// Main draw function that orchestrates all UI components
pub fn draw(f: &mut Frame, app: &App) {
    // Create main layout with status bar at top
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Status bar (now split into static + live boxes)
            Constraint::Min(0),    // Main content
        ])
        .split(f.area());

    // Always draw the status bar at the top
    draw_status_bar(f, app, chunks[0]);

    // Draw main content based on current screen
    match app.current_screen {
        Screen::Dashboard => draw_dashboard(f, app, chunks[1]),
        Screen::Adapters => draw_adapters(f, app, chunks[1]),
        Screen::Services => draw_services(f, app, chunks[1]),
        Screen::Training => draw_training(f, app, chunks[1]),
        Screen::Chat => draw_chat(f, app, chunks[1]),
        Screen::Logs => draw_logs(f, app, chunks[1]),
        Screen::Metrics => draw_metrics(f, app, chunks[1]),
        Screen::Config => draw_config(f, app, chunks[1]),
        Screen::Help => draw_help(f, app, chunks[1]),
    }

    // Draw any overlays (confirmations, errors)
    if let Some(msg) = &app.confirmation_message {
        draw_confirmation(f, msg);
    }

    if let Some(err) = &app.error_message {
        draw_error(f, err);
    }

    if app.setup_state.needs_setup() {
        draw_setup_prompt(f, app);
    }
}

fn draw_logs(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Log Viewer ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White));

    let mut lines = vec![Line::from(vec![
        Span::styled("Filters: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!(
            "trace={} tenant={}   [t] trace [n] tenant [x] clear",
            app.log_filter_trace.as_deref().unwrap_or("any"),
            app.log_filter_tenant.as_deref().unwrap_or("any")
        )),
    ])];

    if let Some(mode) = app.log_filter_mode {
        let label = match mode {
            crate::app::LogFilterMode::TraceId => "Trace",
            crate::app::LogFilterMode::Tenant => "Tenant",
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("Typing {} filter: ", label),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                app.log_filter_input.as_str(),
                Style::default().fg(Color::Cyan),
            ),
        ]));
    }

    let filtered = app.filtered_logs();
    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "No logs yet. Services will generate logs once started.",
            Style::default().fg(Color::Gray),
        )));
    } else {
        for entry in filtered {
            let level_style = match entry.level {
                crate::app::types::LogLevel::Error => Style::default().fg(Color::Red),
                crate::app::types::LogLevel::Warn => Style::default().fg(Color::Yellow),
                crate::app::types::LogLevel::Info => Style::default().fg(Color::Green),
                crate::app::types::LogLevel::Debug => Style::default().fg(Color::Blue),
            };

            let latency = entry
                .latency_ms
                .map(|l| format!("{}ms", l))
                .unwrap_or_else(|| "-".to_string());

            lines.push(Line::from(vec![
                Span::raw(format!("{} ", entry.timestamp.format("%H:%M:%S"))),
                Span::styled(
                    format!("[{:<5}] ", entry.level.as_str()),
                    level_style.add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<10} ", entry.component),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    entry.trace_id.as_deref().unwrap_or("-"),
                    Style::default().fg(Color::Magenta),
                ),
                Span::raw(" "),
                Span::styled(latency, Style::default().fg(Color::White)),
                Span::raw(" "),
                Span::raw(&entry.message),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

fn draw_metrics(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(0),    // Content
        ])
        .split(area);

    // Title
    let title = Paragraph::new("System Health Dashboard")
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Green))
        .alignment(Alignment::Center);
    f.render_widget(title, chunks[0]);

    // Split content area
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // Performance metrics
            Constraint::Length(6), // Resource usage
            Constraint::Min(0),    // Component status
        ])
        .split(chunks[1]);

    // Performance Metrics
    draw_performance_metrics(f, app, content_chunks[0]);

    // Resource Usage
    draw_resource_usage(f, app, content_chunks[1]);

    // Component Status
    draw_component_status(f, app, content_chunks[2]);
}

fn draw_performance_metrics(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Performance Metrics ")
        .borders(Borders::ALL);

    let metrics_text = vec![
        Line::from(vec![
            Span::raw("Inference Latency (P95): "),
            Span::styled(
                format!("{}ms", app.metrics.inference_latency_p95_ms),
                if app.metrics.inference_latency_p95_ms < 200 {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Yellow)
                },
            ),
            Span::raw(if app.metrics.inference_latency_p95_ms < 200 {
                " [Good < 200ms]"
            } else {
                " [Warning > 200ms]"
            }),
        ]),
        Line::from(vec![
            Span::raw("Tokens Per Second: "),
            Span::styled(
                format!("{} TPS", app.metrics.tokens_per_second),
                if app.metrics.tokens_per_second > 500 {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Yellow)
                },
            ),
            Span::raw(if app.metrics.tokens_per_second > 500 {
                " [Excellent > 500]"
            } else {
                " [Normal]"
            }),
        ]),
        Line::from(vec![
            Span::raw("Queue Depth: "),
            Span::styled(
                format!("{} jobs", app.metrics.queue_depth),
                if app.metrics.queue_depth < 10 {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Yellow)
                },
            ),
            Span::raw(if app.metrics.queue_depth < 10 {
                " [Normal < 10]"
            } else {
                " [Warning > 10]"
            }),
        ]),
    ];

    let paragraph = Paragraph::new(metrics_text).block(block);
    f.render_widget(paragraph, area);
}

fn draw_resource_usage(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Resource Usage ")
        .borders(Borders::ALL);

    let memory_bar = widgets::create_progress_bar(
        app.model_status.memory_usage_mb as f32 / app.model_status.total_memory_mb as f32,
        40,
    );

    let usage_text = vec![
        Line::from(vec![
            Span::raw("Memory:  "),
            Span::raw(memory_bar),
            Span::raw(format!(
                " {}/{} MB ({}%)",
                app.model_status.memory_usage_mb,
                app.model_status.total_memory_mb,
                (app.model_status.memory_usage_mb * 100) / app.model_status.total_memory_mb
            )),
        ]),
        Line::from(format!(
            "CPU:     {}%",
            app.system_status.cpu_percent as u32
        )),
        Line::from(format!(
            "Network: ↓ {:.1}MB/s  ↑ {:.1}MB/s",
            app.system_status.network_rx_mbps, app.system_status.network_tx_mbps
        )),
    ];

    let paragraph = Paragraph::new(usage_text).block(block);
    f.render_widget(paragraph, area);
}

fn draw_component_status(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Component Status ")
        .borders(Borders::ALL);

    let mut lines = Vec::new();

    for service in &app.services {
        let status_color = match service.status {
            crate::app::types::Status::Running => Color::Green,
            crate::app::types::Status::Starting => Color::Yellow,
            crate::app::types::Status::Stopped => Color::Gray,
            crate::app::types::Status::Failed => Color::Red,
            crate::app::types::Status::Warning => Color::Yellow,
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("[{}] ", service.status.color_code()),
                Style::default().fg(status_color),
            ),
            Span::styled(
                format!("{:<20}", service.name),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" │ "),
            Span::raw(format!("{:<40}", service.message)),
        ]));
    }

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn draw_config(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Configuration Editor ")
        .borders(Borders::ALL)
        .border_style(
            Style::default().fg(if app.current_mode == crate::app::Mode::ConfigEdit {
                Color::Yellow
            } else {
                Color::Blue
            }),
        );

    let mut config_text = vec![
        Line::from(vec![Span::styled(
            "Server Configuration",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
    ];

    let fields = [
        (
            "Server Port:        ",
            format!("{}", app.config.server_port),
        ),
        (
            "Max Connections:    ",
            format!("{}", app.config.max_connections),
        ),
        ("Model Path:         ", app.config.model_path.clone()),
        (
            "K-Sparse Value:     ",
            format!("{}", app.config.k_sparse_value),
        ),
        ("Batch Size:         ", format!("{}", app.config.batch_size)),
        (
            "Cache Size:         ",
            format!("{} MB", app.config.cache_size_mb),
        ),
        (
            "JWT Mode:           ",
            app.config.jwt_mode.as_str().to_string(),
        ),
        (
            "Require PF Deny:    ",
            if app.config.require_pf_deny {
                "YES".to_string()
            } else {
                "NO".to_string()
            },
        ),
    ];

    for (i, (label, value)) in fields.iter().enumerate() {
        let is_selected =
            app.current_mode == crate::app::Mode::ConfigEdit && i == app.selected_config_field;

        let mut spans = vec![
            Span::raw(if is_selected { "> " } else { "  " }),
            Span::raw(*label),
        ];

        if is_selected {
            spans.push(Span::styled(
                format!("{} ", value),
                Style::default().add_modifier(Modifier::DIM),
            ));
            spans.push(Span::styled(
                format!(" [{}]", app.config_edit_value),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                value.to_string(),
                Style::default().fg(Color::Cyan),
            ));
        }

        config_text.push(Line::from(spans));
    }

    config_text.push(Line::from(""));
    config_text.push(Line::from(vec![
        Span::raw("Production Mode:    "),
        Span::styled(
            if app.production_mode { "[ON]" } else { "[OFF]" },
            Style::default().fg(if app.production_mode {
                Color::Green
            } else {
                Color::Red
            }),
        ),
    ]));

    if app.current_mode == crate::app::Mode::ConfigEdit {
        config_text.push(Line::from(""));
        config_text.push(Line::from(vec![Span::styled(
            "EDIT MODE: Type new value and press Enter to apply, Esc to cancel",
            Style::default().fg(Color::Yellow),
        )]));
    }

    let paragraph = Paragraph::new(config_text)
        .block(block)
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

fn draw_help(f: &mut Frame, _app: &App, area: Rect) {
    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let help_text = vec![
        Line::from(vec![Span::styled(
            "Keyboard Controls",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from("Navigation:"),
        Line::from("  ↑/↓       Navigate menu items"),
        Line::from("  ←/→       Switch between screens"),
        Line::from("  Tab       Next screen"),
        Line::from("  Shift+Tab Previous screen"),
        Line::from("  Enter     Select/activate"),
        Line::from("  Esc       Go back/cancel"),
        Line::from(""),
        Line::from("Quick Keys:"),
        Line::from("  d         Dashboard"),
        Line::from("  s         Services screen"),
        Line::from("  a         Adapters screen"),
        Line::from("  r         Training screen"),
        Line::from("  i         Chat/Inference screen"),
        Line::from("  l         Logs screen"),
        Line::from("  m         Metrics screen"),
        Line::from("  c         Config screen"),
        Line::from("  b         Boot all services"),
        Line::from("  p         Toggle production mode"),
        Line::from("  h         Toggle this help"),
        Line::from("  q         Quit"),
        Line::from("  Ctrl+C    Force quit"),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

fn draw_confirmation(f: &mut Frame, message: &str) {
    let area = centered_rect(50, 20, f.area());

    let block = Block::default()
        .title(" Confirmation ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(message)
        .block(block)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Yellow));

    f.render_widget(paragraph, area);
}

fn draw_error(f: &mut Frame, message: &str) {
    let area = centered_rect(50, 20, f.area());

    let block = Block::default()
        .title(" Error ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    let paragraph = Paragraph::new(message)
        .block(block)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Red));

    f.render_widget(paragraph, area);
}

fn draw_setup_prompt(f: &mut Frame, app: &App) {
    let area = centered_rect(70, 50, f.area());
    let setup = &app.setup_state;

    let mut lines = vec![
        Line::from(Span::styled(
            "adapterOS control center is waiting to bootstrap services",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if !setup.infrastructure_online {
        lines.push(Line::from(Span::raw(
            "Core services are offline. Press 'b' to start everything from this TUI or run ./launch.sh",
        )));
        lines.push(Line::from(""));
    }

    if !setup.missing_prereqs.is_empty() {
        lines.push(Line::from(Span::styled(
            "Missing prerequisites detected:",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        for item in &setup.missing_prereqs {
            lines.push(Line::from(format!("  - {}", item)));
        }
        lines.push(Line::from(""));
    }

    lines.push(Line::from(
        "This panel should be the first process you start - it can launch every other adapterOS service and guide setup.",
    ));
    lines.push(Line::from(""));
    lines.push(Line::from("Shortcuts:"));
    lines.push(Line::from("  - 'b' - bootstrap all services"));
    lines.push(Line::from("  - 'q' - quit TUI after starting services"));
    lines.push(Line::from("  - aos status --json - inspect from shell"));

    if let Some(action) = &setup.last_action {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Last action: {}", action),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        if let Some(output) = &setup.last_output {
            lines.push(Line::from(output.as_str()));
        }
    }

    let block = Block::default()
        .title(" System Setup Required ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

// Helper function to create a centered rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
