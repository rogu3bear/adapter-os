use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, Mode};
use crate::app::types::Status;

pub fn draw_services(f: &mut Frame, app: &App, area: Rect) {
    // Split into service list and detail view
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),     // Service list
            Constraint::Length(8),  // Service details
        ])
        .split(area);

    draw_service_list(f, app, chunks[0]);
    draw_service_details(f, app, chunks[1]);
}

fn draw_service_list(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Select Service to Control ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(
            if app.current_mode == Mode::ServiceSelect {
                Color::Green
            } else {
                Color::White
            }
        ));

    // Create header with proper column alignment
    let header = ListItem::new(Line::from(vec![
        Span::raw("    "),
        Span::styled(
            "Status  Service              State      Dependencies      Action",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
        ),
    ]));

    let separator = ListItem::new(Line::from(
        "   ──────────────────────────────────────────────────────────────────",
    ));

    // Create service items
    let mut items = vec![header, separator];

    for (i, service) in app.services.iter().enumerate() {
        let selected = i == app.selected_service && app.current_mode == Mode::ServiceSelect;
        let prefix = if selected { " > " } else { "   " };

        let (status_indicator, status_color, action) = match service.status {
            Status::Running => ("[OK]", Color::Green, "[Restart]"),
            Status::Starting => ("[..]", Color::Yellow, "[Wait...]"),
            Status::Stopped => ("[--]", Color::Gray, "[Start]  "),
            Status::Failed => ("[XX]", Color::Red, "[Debug]  "),
            Status::Warning => ("[!!]", Color::Yellow, "[Check]  "),
        };

        let dependencies = match service.name.as_str() {
            "Database" => "None             ",
            "Router" => "Database         ",
            "Policy Engine" => "Router           ",
            "Training Service" => "Database,Router  ",
            "Telemetry" => "Metrics          ",
            _ => "None             ",
        };

        let item = ListItem::new(Line::from(vec![
            Span::raw(prefix),
            Span::styled(
                format!("{:<6}", status_indicator),
                Style::default().fg(status_color),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{:<20}", service.name),
                if selected {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ),
            Span::styled(
                format!("{:<11}", service.status.as_str()),
                Style::default().fg(status_color),
            ),
            Span::raw(dependencies),
            Span::styled(
                action,
                Style::default().fg(Color::Cyan),
            ),
        ]));

        items.push(item);
    }

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_service_details(f: &mut Frame, app: &App, area: Rect) {
    let selected_service = &app.services[app.selected_service];

    let block = Block::default()
        .title(format!(" Selected: {} ", selected_service.name))
        .borders(Borders::ALL);

    let mut details = vec![
        Line::from(vec![
            Span::raw("Status: "),
            Span::styled(
                selected_service.status.as_str(),
                Style::default().fg(match selected_service.status {
                    Status::Running => Color::Green,
                    Status::Starting => Color::Yellow,
                    Status::Stopped => Color::Gray,
                    Status::Failed => Color::Red,
                    Status::Warning => Color::Yellow,
                }),
            ),
        ]),
        Line::from(vec![
            Span::raw("Message: "),
            Span::raw(&selected_service.message),
        ]),
    ];

    if selected_service.status == Status::Failed {
        details.push(Line::from(vec![
            Span::styled(
                "Last Error: ",
                Style::default().fg(Color::Red),
            ),
            Span::raw("Failed to load adapter manifest"),
        ]));
        details.push(Line::from(format!("Time: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"))));
    }

    details.push(Line::from(""));
    details.push(Line::from(vec![
        Span::styled(
            "[S] Start  [D] Debug  [L] Logs  [R] Restart  [Esc] Back",
            Style::default().fg(Color::Cyan),
        ),
    ]));

    let paragraph = Paragraph::new(details)
        .block(block)
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}