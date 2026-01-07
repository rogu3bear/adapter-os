//! Chat screen for interactive inference
//!
//! Layout:
//! ```text
//! +-- Chat -----------------------------------------------------------+
//! | Model: Qwen2.5-7B  |  Adapter: code-review-v1  |  Tokens: 234     |
//! +-------------------------------------------------------------------+
//! |                                                                   |
//! | USER: How do I implement error handling in Rust?                  |
//! |                                                                   |
//! | ASSISTANT: In Rust, error handling is typically done using        |
//! | the Result<T, E> type. Here's a common pattern:                   |
//! |                                                                   |
//! | ```rust                                                           |
//! | fn read_file(path: &str) -> Result<String, std::io::Error> {      |
//! |     std::fs::read_to_string(path)                                 |
//! | }                                                                 |
//! | ```                                                               |
//! |                                                                   |
//! +-------------------------------------------------------------------+
//! | > Type message... (Enter to send, Esc to cancel)                  |
//! | [_________________________________________________]              |
//! +-------------------------------------------------------------------+
//! ```

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::{types::ChatRole, App, Mode, Screen};

/// Draw the complete chat screen with header, history, and input areas
pub fn draw_chat(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header with model info
            Constraint::Min(10),   // Chat history
            Constraint::Length(4), // Input area
        ])
        .split(area);

    draw_chat_header(f, app, chunks[0]);
    draw_chat_history(f, app, chunks[1]);
    draw_chat_input(f, app, chunks[2]);
}

/// Draw the header bar showing model, adapter, and token count
fn draw_chat_header(f: &mut Frame, app: &App, area: Rect) {
    // Calculate token count from messages (simple word-based estimate)
    let token_count: usize = app
        .chat_messages
        .iter()
        .map(|m| m.content.split_whitespace().count())
        .sum();

    let model_name = &app.model_status.name;
    let adapter_name = app
        .adapters
        .first()
        .map(|a| a.id.as_str())
        .unwrap_or("none");

    let streaming_indicator = if app.chat_streaming {
        Span::styled(
            " [streaming...] ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::SLOW_BLINK),
        )
    } else {
        Span::raw("")
    };

    let header = Line::from(vec![
        Span::styled("Model: ", Style::default().fg(Color::Gray)),
        Span::styled(model_name, Style::default().fg(Color::Cyan)),
        Span::raw("  |  "),
        Span::styled("Adapter: ", Style::default().fg(Color::Gray)),
        Span::styled(adapter_name, Style::default().fg(Color::Green)),
        Span::raw("  |  "),
        Span::styled("Tokens: ", Style::default().fg(Color::Gray)),
        Span::styled(
            format!("{}", token_count),
            Style::default().fg(Color::Magenta),
        ),
        streaming_indicator,
    ]);

    let block = Block::default()
        .title(" Chat ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(header).block(block);
    f.render_widget(paragraph, area);
}

/// Draw the chat history with message bubbles
fn draw_chat_history(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Conversation ")
        .borders(Borders::ALL);

    if app.chat_messages.is_empty() {
        let empty_msg = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No messages yet. Type a message below to start chatting.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Press 'i' from any screen to open Chat.",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        let paragraph = Paragraph::new(empty_msg).block(block);
        f.render_widget(paragraph, area);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.chat_messages {
        let (role_label, role_style) = match msg.role {
            ChatRole::User => (
                "USER",
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            ChatRole::Assistant => (
                "ASSISTANT",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            ChatRole::System => (
                "SYSTEM",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        };

        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            format!("{}: ", role_label),
            role_style,
        )]));

        // Wrap message content, preserving line breaks
        for line in msg.content.lines() {
            if line.is_empty() {
                lines.push(Line::from(""));
            } else {
                lines.push(Line::from(Span::raw(format!("  {}", line))));
            }
        }
    }

    // If streaming, add a cursor indicator
    if app.chat_streaming {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "ASSISTANT: ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "...",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ]));
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

/// Draw the input area with prompt and help text
fn draw_chat_input(f: &mut Frame, app: &App, area: Rect) {
    let is_chat_screen = app.current_screen == Screen::Chat;
    let is_chat_input_mode = app.current_mode == Mode::ChatInput;

    let border_color = if is_chat_screen && is_chat_input_mode {
        Color::Yellow
    } else if is_chat_screen {
        Color::Cyan
    } else {
        Color::White
    };

    let title = if is_chat_input_mode {
        " Input (typing...) "
    } else {
        " Input (Enter to type) "
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let input_content = if app.chat_input.is_empty() && !is_chat_input_mode {
        vec![Line::from(Span::styled(
            "Type your message here...",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        vec![Line::from(vec![
            Span::raw("> "),
            Span::styled(&app.chat_input, Style::default().fg(Color::White)),
            if is_chat_input_mode {
                Span::styled("_", Style::default().fg(Color::Yellow))
            } else {
                Span::raw("")
            },
        ])]
    };

    let help_line = Line::from(vec![
        Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
        Span::raw(if is_chat_input_mode {
            " Send  "
        } else {
            " Type  "
        }),
        Span::styled("[Esc]", Style::default().fg(Color::Yellow)),
        Span::raw(if is_chat_input_mode {
            " Cancel  "
        } else {
            " Back  "
        }),
        Span::styled("[Ctrl+C]", Style::default().fg(Color::Yellow)),
        Span::raw(" Stop"),
    ]);

    let mut lines = input_content;
    lines.push(help_line);

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}
