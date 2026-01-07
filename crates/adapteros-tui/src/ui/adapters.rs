use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::App;

/// Draw the Adapter browser screen with list and detail panes
pub fn draw_adapters(f: &mut Frame, app: &App, area: Rect) {
    // Split into left (list) and right (detail) panes
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_adapter_list(f, app, chunks[0]);
    draw_adapter_detail(f, app, chunks[1]);
}

fn draw_adapter_list(f: &mut Frame, app: &App, area: Rect) {
    // Create header with stats
    let loaded_count = app.adapters.iter().filter(|a| a.loaded).count();
    let total_count = app.adapters.len();

    let header = format!(
        " Adapters | Total: {}  Loaded: {} ",
        total_count, loaded_count
    );

    let block = Block::default()
        .title(header)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    // Build list items
    let items: Vec<ListItem> = if app.adapters.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  No adapters registered. Use 'aosctl adapter register' to add adapters.",
            Style::default().fg(Color::Gray),
        )))]
    } else {
        app.adapters
            .iter()
            .enumerate()
            .map(|(i, adapter)| {
                let selected = i == app.selected_adapter;
                let prefix = if selected { "> " } else { "  " };

                let mut status_tags = String::new();
                if adapter.loaded {
                    status_tags.push_str(" [LOADED]");
                }

                let style = if selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else if adapter.loaded {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::White)
                };

                let line = Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(&adapter.id, style),
                    Span::styled(status_tags, Style::default().fg(Color::Cyan)),
                ]);

                ListItem::new(line)
            })
            .collect()
    };

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    f.render_widget(list, area);
}

fn draw_adapter_detail(f: &mut Frame, app: &App, area: Rect) {
    // Split detail pane into info and actions
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(5)])
        .split(area);

    draw_adapter_info(f, app, chunks[0]);
    draw_adapter_actions(f, chunks[1]);
}

fn draw_adapter_info(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().title(" Detail ").borders(Borders::ALL);

    let content = if let Some(adapter) = app.adapters.get(app.selected_adapter) {
        let memory_str = adapter
            .memory_mb
            .map(|m| format!("{} MB", m))
            .unwrap_or_else(|| "-".to_string());

        vec![
            Line::from(vec![
                Span::styled("ID: ", Style::default().fg(Color::Gray)),
                Span::styled(&adapter.id, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Name: ", Style::default().fg(Color::Gray)),
                Span::styled(&adapter.name, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Version: ", Style::default().fg(Color::Gray)),
                Span::styled(&adapter.version, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("Memory: ", Style::default().fg(Color::Gray)),
                Span::styled(memory_str, Style::default().fg(Color::Magenta)),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    if adapter.loaded { "LOADED" } else { "UNLOADED" },
                    if adapter.loaded {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Gray)
                    },
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Use arrow keys to select, actions below",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    } else {
        vec![Line::from(Span::styled(
            "No adapter selected",
            Style::default().fg(Color::Gray),
        ))]
    };

    let paragraph = Paragraph::new(content).block(block);
    f.render_widget(paragraph, area);
}

fn draw_adapter_actions(f: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Actions ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let actions = vec![Line::from(vec![
        Span::styled(
            "[L]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Load   "),
        Span::styled(
            "[U]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Unload   "),
        Span::styled(
            "[P]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Pin   "),
        Span::styled(
            "[S]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Swap"),
    ])];

    let paragraph = Paragraph::new(actions).block(block);
    f.render_widget(paragraph, area);
}
