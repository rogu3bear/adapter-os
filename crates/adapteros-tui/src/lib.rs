//! AdapterOS TUI - Terminal User Interface dashboard
//!
//! Provides an interactive terminal dashboard for monitoring and controlling AdapterOS.
//!
//! # Usage
//!
//! ```no_run
//! use adapteros_tui::run_tui;
//!
//! #[tokio::main]
//! async fn main() {
//!     run_tui(None).await.unwrap();
//! }
//! ```

pub mod app;
pub mod ui;

pub use app::App;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};
use tokio::time::sleep;
use tracing::{error, info};

use ui::draw;

/// Run the TUI dashboard
///
/// # Arguments
/// * `server_url` - Optional server URL override (default: http://localhost:8080)
///
/// # Returns
/// * `Ok(())` on successful exit
/// * `Err` on terminal or application error
pub async fn run_tui(server_url: Option<String>) -> Result<()> {
    info!("Starting adapterOS TUI Control System");

    // Setup terminal
    let mut terminal = setup_terminal()?;

    // Create app state with optional URL override
    let mut app = match server_url {
        Some(url) => App::new_with_url(url).await?,
        None => App::new().await?,
    };

    // Run the TUI
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    restore_terminal(&mut terminal)?;

    if let Err(e) = res {
        error!("Application error: {}", e);
        return Err(e);
    }

    info!("adapterOS TUI shutdown gracefully");
    Ok(())
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()>
where
    <B as ratatui::backend::Backend>::Error: Send + Sync + 'static,
{
    loop {
        // Draw the UI
        terminal.draw(|f| draw(f, app))?;

        // Handle events with a small timeout to allow for async updates
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    // Handle Ctrl+C for exit
                    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
                        break;
                    }

                    // Handle other key events
                    match key.code {
                        KeyCode::Char('q') => {
                            if app.should_quit() {
                                break;
                            }
                        }
                        KeyCode::Up => app.on_up(),
                        KeyCode::Down => app.on_down(),
                        KeyCode::Left => app.on_left(),
                        KeyCode::Right => app.on_right(),
                        KeyCode::Enter => app.on_enter().await?,
                        KeyCode::Tab => app.on_tab(),
                        KeyCode::BackTab => app.on_backtab(),
                        KeyCode::Esc => app.on_escape(),
                        KeyCode::Backspace => app.on_backspace(),
                        KeyCode::Char(c) => app.on_char(c).await?,
                        _ => {}
                    }
                }
                Event::Mouse(_) => {
                    // Mouse events can be handled here if needed
                }
                Event::Resize(_, _) => {
                    // Terminal resize is handled automatically by ratatui
                }
                _ => {}
            }
        }

        // Update app state (fetch new metrics, etc.)
        app.update().await?;

        // Small sleep to prevent busy looping
        sleep(Duration::from_millis(50)).await;
    }

    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}
