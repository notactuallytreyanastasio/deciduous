//! Event handling for the TUI
//!
//! Implements vim-style keybindings and mode switching

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{App, Focus, ModalContent, Mode, View};

/// Handle a key event, returns true if app should quit
pub fn handle_event(app: &mut App, key: KeyEvent) -> bool {
    // Handle help overlay first
    if app.show_help {
        if matches!(
            key.code,
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')
        ) {
            app.show_help = false;
        }
        return false;
    }

    // Handle modal
    if app.focus == Focus::Modal {
        return handle_modal(app, key);
    }

    // Handle file picker
    if app.focus == Focus::FilePicker {
        return handle_file_picker(app, key);
    }

    // Handle based on mode
    match app.mode {
        Mode::Search => handle_search_mode(app, key),
        Mode::Normal => handle_normal_mode(app, key),
        Mode::Command => handle_command_mode(app, key),
        Mode::BranchSearch => handle_branch_search_mode(app, key),
    }
}

fn handle_search_mode(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.search_query.clear();
            app.apply_filters();
        }
        KeyCode::Enter => {
            app.mode = Mode::Normal;
            app.focus = Focus::List;
        }
        KeyCode::Backspace => {
            app.search_query.pop();
            app.apply_filters();
        }
        KeyCode::Char(c) => {
            app.search_query.push(c);
            app.apply_filters();
        }
        _ => {}
    }
    false
}

fn handle_command_mode(app: &mut App, key: KeyEvent) -> bool {
    if key.code == KeyCode::Esc {
        app.mode = Mode::Normal;
    }
    false
}

fn handle_branch_search_mode(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.branch_search_query.clear();
        }
        KeyCode::Enter => {
            app.select_branch_from_search();
        }
        KeyCode::Down | KeyCode::Tab => {
            app.branch_search_next();
        }
        KeyCode::Up | KeyCode::BackTab => {
            app.branch_search_prev();
        }
        KeyCode::Backspace => {
            app.branch_search_query.pop();
            app.update_branch_search();
        }
        KeyCode::Char(c) => {
            app.branch_search_query.push(c);
            app.update_branch_search();
        }
        _ => {}
    }
    false
}

fn handle_normal_mode(app: &mut App, key: KeyEvent) -> bool {
    // Check for 'g' prefix first
    if app.pending_g {
        app.pending_g = false;
        match key.code {
            KeyCode::Char('g') => {
                // gg - jump to top (view-specific)
                match app.current_view {
                    View::Timeline => app.jump_to_top(),
                    View::Roadmap => app.roadmap_state.jump_to_top(),
                    View::Dag => {} // DAG doesn't have a selection to jump
                }
                return false;
            }
            _ => {
                // Invalid g-sequence, ignore
                return false;
            }
        }
    }

    match app.current_view {
        View::Timeline => handle_timeline_keys(app, key),
        View::Dag => handle_dag_keys(app, key),
        View::Roadmap => handle_roadmap_keys(app, key),
    }
}

fn handle_timeline_keys(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        // Quit
        KeyCode::Char('q') => return true,

        // Help
        KeyCode::Char('?') => {
            app.show_help = true;
        }

        // Navigation
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
        KeyCode::Char('g') => {
            app.pending_g = true;
        }
        KeyCode::Char('G') => app.jump_to_bottom(),

        // Page navigation
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => app.page_down(),
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => app.page_up(),
        KeyCode::PageDown => app.page_down(),
        KeyCode::PageUp => app.page_up(),

        // Search
        KeyCode::Char('/') => {
            app.mode = Mode::Search;
            app.focus = Focus::Search;
            app.search_query.clear();
        }

        // Filter by type
        KeyCode::Char('f') => {
            app.cycle_type_filter();
        }

        // Clear filters
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.type_filter = None;
            app.branch_filter = None;
            app.search_query.clear();
            app.apply_filters();
        }

        // Toggle detail panel
        KeyCode::Enter => {
            app.toggle_detail();
        }

        // Open files in editor (or toggle file browser if in detail mode)
        KeyCode::Char('o') => {
            if app.detail_in_files {
                app.open_current_file();
            } else if let Some(node) = app.selected_node() {
                let files = App::get_files(node);
                if !files.is_empty() {
                    app.show_file_picker(files);
                } else {
                    app.set_status("No files associated with this node".to_string());
                }
            }
        }

        // Open commit modal
        KeyCode::Char('O') => {
            app.show_commit_modal();
        }

        // Filter by branch (cycle)
        KeyCode::Char('b') => {
            app.cycle_branch_filter();
        }

        // Branch search (fuzzy)
        KeyCode::Char('B') => {
            app.enter_branch_search();
        }

        // Toggle timeline order
        KeyCode::Char('R') => {
            app.toggle_order();
        }

        // Show goal story (hierarchy from goal to outcomes)
        KeyCode::Char('s') => {
            app.show_goal_story();
        }

        // Toggle file browser in detail panel
        KeyCode::Char('F') => {
            app.toggle_file_browser();
        }

        // File navigation when in file browser mode
        KeyCode::Char('n') if app.detail_in_files => {
            app.next_file();
        }
        KeyCode::Char('N') if app.detail_in_files => {
            app.prev_file();
        }

        // Preview file content
        KeyCode::Char('p') => {
            app.show_file_preview();
        }

        // Show file diff
        KeyCode::Char('d') if app.detail_in_files => {
            app.show_file_diff();
        }

        // Refresh
        KeyCode::Char('r') => {
            if let Err(e) = app.reload_graph() {
                app.set_status(format!("Refresh failed: {}", e));
            } else {
                app.show_refresh_indicator();
            }
        }

        // Switch view
        KeyCode::Tab => app.toggle_view(),

        // Escape clears selection or exits modes
        KeyCode::Esc => {
            if app.detail_expanded {
                app.detail_expanded = false;
            }
        }

        _ => {}
    }
    false
}

fn handle_dag_keys(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        // Quit
        KeyCode::Char('q') => return true,

        // Help
        KeyCode::Char('?') => {
            app.show_help = true;
        }

        // Pan
        KeyCode::Char('h') | KeyCode::Left => app.dag_pan(-1, 0),
        KeyCode::Char('j') | KeyCode::Down => app.dag_pan(0, 1),
        KeyCode::Char('k') | KeyCode::Up => app.dag_pan(0, -1),
        KeyCode::Char('l') | KeyCode::Right => app.dag_pan(1, 0),

        // Zoom
        KeyCode::Char('+') | KeyCode::Char('=') => app.dag_zoom_in(),
        KeyCode::Char('-') => app.dag_zoom_out(),
        KeyCode::Char('0') => app.dag_reset_zoom(),

        // Switch view
        KeyCode::Tab => app.toggle_view(),

        // Refresh
        KeyCode::Char('r') => {
            if let Err(e) = app.reload_graph() {
                app.set_status(format!("Refresh failed: {}", e));
            } else {
                app.show_refresh_indicator();
            }
        }

        KeyCode::Esc => {}

        _ => {}
    }
    false
}

fn handle_roadmap_keys(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        // Quit
        KeyCode::Char('q') => return true,

        // Help
        KeyCode::Char('?') => {
            app.show_help = true;
        }

        // Navigation
        KeyCode::Char('j') | KeyCode::Down => app.roadmap_state.move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.roadmap_state.move_up(),

        // Jump to top (gg - handled via pending_g in normal_mode)
        KeyCode::Char('g') => {
            app.pending_g = true;
        }

        // Jump to bottom (G)
        KeyCode::Char('G') => {
            app.roadmap_state.jump_to_bottom();
        }

        // Page navigation
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.roadmap_state.page_down(10);
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.roadmap_state.page_up(10);
        }
        KeyCode::PageDown => app.roadmap_state.page_down(10),
        KeyCode::PageUp => app.roadmap_state.page_up(10),

        // Toggle detail panel (Enter)
        KeyCode::Enter => {
            app.roadmap_state.toggle_detail();
        }

        // Toggle between Active/Completed view (Shift+Tab)
        KeyCode::BackTab => {
            app.roadmap_state.toggle_mode();
            let mode_name = match app.roadmap_state.view_mode {
                super::views::roadmap::RoadmapViewMode::Active => "Active",
                super::views::roadmap::RoadmapViewMode::Completed => "Completed",
            };
            app.set_status(format!("Switched to {} items", mode_name));
        }

        // Switch view (Tab cycles through main views)
        KeyCode::Tab => app.toggle_view(),

        // Refresh roadmap items
        KeyCode::Char('r') => {
            app.load_roadmap_items();
            app.set_status("Roadmap refreshed".to_string());
        }

        // Open GitHub issue in browser
        KeyCode::Char('o') => {
            if app.roadmap_state.github_repo.is_none() {
                app.set_status(
                    "No GitHub repo configured. Set up roadmap sync or add git remote.".to_string(),
                );
            } else if let Some(url) = app.roadmap_state.selected_issue_url() {
                // Open URL in default browser
                #[cfg(target_os = "macos")]
                let result = std::process::Command::new("open").arg(&url).spawn();
                #[cfg(target_os = "linux")]
                let result = std::process::Command::new("xdg-open").arg(&url).spawn();
                #[cfg(target_os = "windows")]
                let result = std::process::Command::new("cmd")
                    .args(["/C", "start", "", &url])
                    .spawn();

                match result {
                    Ok(_) => app.set_status(format!("Opened {}", url)),
                    Err(e) => app.set_status(format!("Failed to open browser: {}", e)),
                }
            } else {
                app.set_status("No GitHub issue linked to this item".to_string());
            }
        }

        // Toggle checkbox state (mark complete/incomplete)
        KeyCode::Char('c') => {
            if let Some((item_id, current_state)) = app.roadmap_state.selected_item_checkbox_info()
            {
                let new_state = if current_state == "checked" {
                    "unchecked"
                } else {
                    "checked"
                };
                match app.toggle_roadmap_checkbox(item_id, new_state) {
                    Ok(()) => {
                        app.load_roadmap_items();
                        let action = if new_state == "checked" {
                            "checked"
                        } else {
                            "unchecked"
                        };
                        app.set_status(format!("Item marked as {}", action));
                    }
                    Err(e) => app.set_status(format!("Failed to update: {}", e)),
                }
            } else {
                app.set_status("No item selected".to_string());
            }
        }

        // Close detail panel
        KeyCode::Esc => {
            if app.roadmap_state.show_detail {
                app.roadmap_state.show_detail = false;
            }
        }

        _ => {}
    }
    false
}

fn handle_file_picker(app: &mut App, key: KeyEvent) -> bool {
    if let Some(ref mut picker) = app.file_picker {
        match key.code {
            KeyCode::Esc => {
                app.file_picker = None;
                app.focus = Focus::List;
            }
            KeyCode::Char('j') | KeyCode::Down => picker.move_down(),
            KeyCode::Char('k') | KeyCode::Up => picker.move_up(),
            KeyCode::Char(' ') => picker.toggle_current(),
            KeyCode::Enter => {
                let selected = picker.get_selected_files();
                let files = if selected.is_empty() {
                    // If nothing selected, use current cursor item
                    vec![picker.files[picker.cursor].clone()]
                } else {
                    selected
                };
                app.file_picker = None;
                app.focus = Focus::List;
                app.open_files(files);
            }
            KeyCode::Char('a') => {
                // Select all
                for sel in picker.selected.iter_mut() {
                    *sel = true;
                }
            }
            KeyCode::Char('q') => {
                app.file_picker = None;
                app.focus = Focus::List;
            }
            _ => {}
        }
    }
    false
}

fn handle_modal(app: &mut App, key: KeyEvent) -> bool {
    // Check if we're in a commit modal - handle it specially
    if matches!(app.modal, Some(ModalContent::Commit { .. })) {
        return handle_commit_modal(app, key);
    }

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.close_modal();
        }
        // Scrolling
        KeyCode::Char('j') | KeyCode::Down => {
            app.modal_scroll_down(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.modal_scroll_up(1);
        }
        KeyCode::Char('d')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            app.modal_scroll_down(10);
        }
        KeyCode::Char('u')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            app.modal_scroll_up(10);
        }
        KeyCode::Char('g') => {
            app.modal_scroll.offset = 0; // Jump to top
        }
        KeyCode::Char('G') => {
            app.modal_scroll.offset = app.modal_scroll.total_lines.saturating_sub(10);
        }
        // Open file in editor (for file/diff modals)
        KeyCode::Char('o') => {
            if app.get_modal_file_path().is_some() {
                app.open_modal_file();
                app.close_modal();
            }
        }
        _ => {}
    }
    false
}

fn handle_commit_modal(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.close_modal();
        }
        // Navigation - j/k move between sections or scroll diff
        KeyCode::Char('j') | KeyCode::Down => {
            app.commit_modal_down(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.commit_modal_up(1);
        }
        // Page down/up in diff section
        KeyCode::Char('d')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            app.commit_modal_page_down(10);
        }
        KeyCode::Char('u')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            app.commit_modal_page_up(10);
        }
        // Jump to top/bottom
        KeyCode::Char('g') => {
            app.commit_modal_top();
        }
        KeyCode::Char('G') => {
            app.commit_modal_bottom();
        }
        _ => {}
    }
    false
}
