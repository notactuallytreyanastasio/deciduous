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
pub mod msg; // TEA message types (what happened)
pub mod state; // Pure state transformations (functional core)
pub mod types;
pub mod ui;
pub mod update; // TEA update function (state transitions)
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

    // Run the app, ensuring cleanup happens even on error
    let result = run_app_inner(&mut terminal, db_path);

    // Restore terminal - this MUST run even if app fails
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();

    result
}

fn run_app_inner<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    db_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
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
    run_event_loop(terminal, &mut app, rx)
}

fn run_event_loop<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    file_change_rx: mpsc::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        // Draw the UI
        terminal.draw(|f| ui::draw(f, app))?;

        // Check if we need to open files in editor
        if let Some(files) = app.take_pending_editor_files() {
            open_files_in_editor(terminal, &files)?;
            app.set_status(format!("Opened {} file(s)", files.len()));
        }

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

/// Suspend the TUI, open files in editor, then resume
fn open_files_in_editor<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    files: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Command;

    // Get editor from environment
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

    // Leave alternate screen temporarily
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;
    crossterm::terminal::disable_raw_mode()?;

    // Open each file in editor
    for file in files {
        let status = Command::new(&editor).arg(file).status();

        if let Err(e) = status {
            eprintln!("Failed to open {}: {}", file, e);
        }
    }

    // Re-enter TUI mode
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::EnterAlternateScreen
    )?;
    terminal.clear()?;

    Ok(())
}
