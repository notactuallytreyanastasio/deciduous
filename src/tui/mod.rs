//! Terminal User Interface for Deciduous
//!
//! A rich TUI for exploring and navigating the decision graph.
//! Features:
//! - Timeline view with vim-style navigation
//! - DAG visualization with hierarchical layout
//! - Node detail panel with code jumping
//! - Auto-refresh on database changes

pub mod app;
pub mod events;
pub mod types;
pub mod ui;
pub mod views;
pub mod widgets;

use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::{
    event::{poll, read, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::prelude::*;

use app::App;
use events::handle_event;

/// Run the TUI application
pub fn run(db_path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new(db_path)?;

    // Setup file watcher for auto-refresh
    let (tx, rx) = mpsc::channel();
    let db_path_for_watcher = app.db_path().to_path_buf();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                if event.kind.is_modify() {
                    let _ = tx.send(());
                }
            }
        },
        Config::default(),
    )?;

    // Watch the database file
    watcher.watch(&db_path_for_watcher, RecursiveMode::NonRecursive)?;

    // Run the main loop
    let result = run_app(&mut terminal, &mut app, rx);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    file_change_rx: mpsc::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        // Draw the UI
        terminal.draw(|f| ui::draw(f, app))?;

        // Handle input with timeout
        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if poll(timeout)? {
            match read()? {
                Event::Key(key) => {
                    if handle_event(app, key) {
                        return Ok(()); // Quit signal
                    }
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse(mouse);
                }
                Event::Resize(width, height) => {
                    app.resize(width, height);
                }
                _ => {}
            }
        }

        // Check for file changes (non-blocking)
        if file_change_rx.try_recv().is_ok() {
            app.reload_graph()?;
            app.show_refresh_indicator();
        }

        // Tick for animations/updates
        if last_tick.elapsed() >= tick_rate {
            app.tick();
            last_tick = Instant::now();
        }
    }
}
