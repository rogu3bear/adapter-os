use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::types::Status;
use crate::app::{App, LogFilterMode};

pub fn draw_dashboard(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(36),
            Constraint::Percentage(30),
        ])
        .split(area);

    draw_adapter_vram(f, app, chunks[0]);
    draw_request_log(f, app, chunks[1]);
    draw_health(f, app, chunks[2]);
}

fn draw_adapter_vram(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Active Adapters & VRAM ")
        .borders(Borders::ALL);

    let loaded: Vec<_> = app.adapters.iter().filter(|a| a.loaded).collect();
    let headroom = app.metrics.memory_headroom_percent;

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                "VRAM ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "{}/{} MB  ",
                app.model_status.memory_usage_mb, app.model_status.total_memory_mb
            )),
            Span::styled(
                format!("{:.1}% headroom", headroom),
                Style::default().fg(if headroom >= 15.0 {
                    Color::Green
                } else {
                    Color::Yellow
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "Loaded ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "{} of {} adapters",
                loaded.len(),
                app.adapters.len()
            )),
        ]),
        Line::from(""),
    ];

    if loaded.is_empty() {
        lines.push(Line::from(Span::styled(
            "No adapters loaded. Swap in adapters to start using VRAM.",
            Style::default().fg(Color::Gray),
        )));
    } else {
        for adapter in loaded.iter().take(8) {
            let mem = adapter.memory_mb.unwrap_or(0);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<18}", adapter.id),
                    Style::default().fg(Color::White),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{:>4} MB", mem),
                    Style::default().fg(Color::Magenta),
                ),
                Span::raw("  "),
                Span::styled(adapter.version.as_str(), Style::default().fg(Color::Gray)),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn draw_request_log(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Recent Request Log ")
        .borders(Borders::ALL);

    let filters = format!(
        "Filters: trace={} tenant={}   [t] trace [n] tenant [x] clear",
        app.log_filter_trace.as_deref().unwrap_or("any"),
        app.log_filter_tenant.as_deref().unwrap_or("any")
    );

    let mut items: Vec<ListItem> = Vec::new();

    if let Some(mode) = app.log_filter_mode {
        let label = match mode {
            LogFilterMode::TraceId => "Trace ID",
            LogFilterMode::Tenant => "Tenant",
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("{} filter: ", label),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                app.log_filter_input.as_str(),
                Style::default().fg(Color::Cyan),
            ),
        ])));
    } else {
        items.push(ListItem::new(Line::from(filters)));
    }

    let logs = app.filtered_logs();
    if logs.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "No request logs yet.",
            Style::default().fg(Color::Gray),
        ))));
    } else {
        for entry in logs.into_iter().take(10) {
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

            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{} ", entry.timestamp.format("%H:%M:%S")),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(
                    format!("{:<5}", entry.level.as_str()),
                    level_style.add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    entry.trace_id.as_deref().unwrap_or("-"),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(" "),
                Span::styled(
                    entry.tenant_id.as_deref().unwrap_or("-"),
                    Style::default().fg(Color::Magenta),
                ),
                Span::raw(" "),
                Span::styled(latency, Style::default().fg(Color::White)),
                Span::raw(" "),
                Span::raw(&entry.message),
            ])));
        }
    }

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_health(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" System Health (Heartbeat) ")
        .borders(Borders::ALL);

    let running = app
        .services
        .iter()
        .filter(|s| s.status == Status::Running)
        .count();
    let failed = app
        .services
        .iter()
        .filter(|s| s.status == Status::Failed)
        .count();

    let status_line = if let Some(health) = &app.health_status {
        let color = match health.status.to_lowercase().as_str() {
            "healthy" | "ready" | "online" => Color::Green,
            "degraded" | "warning" => Color::Yellow,
            _ => Color::Red,
        };
        Line::from(vec![
            Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(health.status.as_str(), Style::default().fg(color)),
            Span::raw("  "),
            Span::raw(
                health
                    .version
                    .as_deref()
                    .map(|v| format!("v{}", v))
                    .unwrap_or_else(|| "version: -".to_string()),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                if app.setup_state.infrastructure_online {
                    "online"
                } else {
                    "offline"
                },
                Style::default().fg(if app.setup_state.infrastructure_online {
                    Color::Green
                } else {
                    Color::Red
                }),
            ),
        ])
    };

    let heartbeat = Line::from(vec![
        Span::styled("Heartbeat: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!("{}s ago", app.last_update.elapsed().as_secs())),
        Span::raw("  Services "),
        Span::styled(
            format!("{} running", running),
            Style::default().fg(Color::Green),
        ),
        Span::raw(", "),
        Span::styled(
            format!("{} failed", failed),
            Style::default().fg(if failed > 0 { Color::Red } else { Color::Gray }),
        ),
    ]);

    let mut lines = vec![
        status_line,
        heartbeat,
        Line::from(vec![
            Span::styled(
                "Latency P95: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("{}ms", app.metrics.inference_latency_p95_ms)),
            Span::raw("  TPS: "),
            Span::raw(format!("{}", app.metrics.tokens_per_second)),
            Span::raw("  Queue: "),
            Span::raw(format!("{}", app.metrics.queue_depth)),
        ]),
    ];

    if let Some(health) = &app.health_status {
        lines.push(Line::from(vec![
            Span::styled("Uptime: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{}s", health.uptime_seconds)),
        ]));
    }

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}
