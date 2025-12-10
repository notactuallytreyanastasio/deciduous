//! UI rendering for the TUI

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::app::{App, Mode, View};
use super::views::{timeline, dag, detail};
use super::widgets::file_picker;

/// Main draw function - orchestrates all rendering
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Main layout: header, content, footer
    let main_layout = Layout::vertical([
        Constraint::Length(1),  // Header
        Constraint::Length(1),  // Filter bar
        Constraint::Min(10),    // Content
        Constraint::Length(1),  // Footer/status
    ])
    .split(area);

    // Draw header
    draw_header(frame, app, main_layout[0]);

    // Draw filter bar
    draw_filter_bar(frame, app, main_layout[1]);

    // Draw main content based on view
    match app.current_view {
        View::Timeline => {
            if app.detail_expanded {
                // Split content horizontally
                let content_layout = Layout::horizontal([
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ])
                .split(main_layout[2]);

                timeline::draw(frame, app, content_layout[0]);
                detail::draw(frame, app, content_layout[1]);
            } else {
                timeline::draw(frame, app, main_layout[2]);
            }
        }
        View::Dag => {
            dag::draw(frame, app, main_layout[2]);
        }
    }

    // Draw footer
    draw_footer(frame, app, main_layout[3]);

    // Draw overlays
    if app.show_help {
        draw_help_overlay(frame, area);
    }

    if app.file_picker.is_some() {
        file_picker::draw(frame, app, area);
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let view_name = match app.current_view {
        View::Timeline => "Timeline",
        View::Dag => "DAG",
    };

    let node_count = app.filtered_nodes.len();
    let total_nodes = app.graph.nodes.len();
    let edge_count = app.graph.edges.len();

    let refresh_indicator = if app.refresh_shown_at.is_some() {
        " [Updated]"
    } else {
        ""
    };

    let header_text = format!(
        " Deciduous TUI │ {} │ [{}/{} nodes] [{} edges]{}",
        view_name, node_count, total_nodes, edge_count, refresh_indicator
    );

    let header = Paragraph::new(header_text)
        .style(Style::default().bg(Color::Blue).fg(Color::White).bold());

    frame.render_widget(header, area);
}

fn draw_filter_bar(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![Span::raw(" Filters: ")];

    // Type filter buttons
    let types = ["All", "goal", "decision", "option", "action", "outcome", "observation"];
    for t in types {
        let is_active = match &app.type_filter {
            None => t == "All",
            Some(f) => f == t,
        };
        let style = if is_active {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(format!("[{}]", t), style));
        spans.push(Span::raw(" "));
    }

    // Search indicator
    if app.mode == Mode::Search || !app.search_query.is_empty() {
        spans.push(Span::raw("│ Search: "));
        spans.push(Span::styled(
            &app.search_query,
            Style::default().fg(Color::Cyan),
        ));
        if app.mode == Mode::Search {
            spans.push(Span::styled("_", Style::default().fg(Color::Cyan).rapid_blink()));
        }
    }

    let filter_bar = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(Color::DarkGray));

    frame.render_widget(filter_bar, area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let keybinds = match app.current_view {
        View::Timeline => {
            "j/k:move  Enter:detail  o:open-code  /:search  f:filter  Tab:DAG  ?:help  q:quit"
        }
        View::Dag => {
            "h/j/k/l:pan  +/-:zoom  0:reset  Tab:Timeline  ?:help  q:quit"
        }
    };

    // Show status message if present, otherwise show keybinds
    let footer_text = if let Some((ref msg, _)) = app.status_message {
        msg.clone()
    } else {
        keybinds.to_string()
    };

    let footer = Paragraph::new(format!(" {}", footer_text))
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));

    frame.render_widget(footer, area);
}

fn draw_help_overlay(frame: &mut Frame, area: Rect) {
    // Center the help popup
    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = 22.min(area.height.saturating_sub(4));

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    // Clear the background
    frame.render_widget(Clear, popup_area);

    let help_text = r#"
  Timeline View
  ─────────────────────────────────
  j/k, ↑/↓     Move up/down
  gg           Jump to top
  G            Jump to bottom
  Ctrl+d/u     Page down/up
  Enter        Toggle detail panel
  o            Open associated files
  O            Open commit in git
  /            Search
  f            Cycle type filter
  Ctrl+c       Clear all filters
  Tab          Switch to DAG view
  r            Refresh
  q            Quit

  DAG View
  ─────────────────────────────────
  h/j/k/l      Pan view
  +/-          Zoom in/out
  0            Reset zoom
  Tab          Switch to Timeline

  Press ? or Esc to close
"#;

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White).bg(Color::Black));

    frame.render_widget(help, popup_area);
}

/// Get color for node type
pub fn node_type_color(node_type: &str) -> Color {
    match node_type {
        "goal" => Color::Green,
        "decision" => Color::Yellow,
        "option" => Color::Magenta,
        "action" => Color::Red,
        "outcome" => Color::Cyan,
        "observation" => Color::Blue,
        _ => Color::White,
    }
}

/// Get style for node type badge
pub fn node_type_style(node_type: &str) -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(node_type_color(node_type))
        .bold()
}
