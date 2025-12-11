//! UI rendering for the TUI

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use syntect::highlighting::{ThemeSet, Style as SyntectStyle};
use syntect::parsing::SyntaxSet;
use syntect::easy::HighlightLines;

use super::app::{App, Mode, View, ModalContent};
use super::views::{timeline, dag, detail};
use super::widgets::file_picker;

// Lazy static syntax highlighting resources
lazy_static::lazy_static! {
    static ref PS: SyntaxSet = SyntaxSet::load_defaults_newlines();
    static ref TS: ThemeSet = ThemeSet::load_defaults();
}

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

    if app.modal.is_some() {
        draw_modal(frame, app, area);
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

    // Branch filter or branch search
    if app.mode == Mode::BranchSearch {
        spans.push(Span::raw("│ Branch search: "));
        spans.push(Span::styled(
            &app.branch_search_query,
            Style::default().fg(Color::Cyan),
        ));
        spans.push(Span::styled("_", Style::default().fg(Color::Cyan).rapid_blink()));

        // Show matches
        if !app.branch_search_matches.is_empty() {
            spans.push(Span::raw(" → "));
            let selected = &app.branch_search_matches[app.branch_search_index];
            spans.push(Span::styled(
                selected.clone(),
                Style::default().fg(Color::Black).bg(Color::Cyan),
            ));
            if app.branch_search_matches.len() > 1 {
                spans.push(Span::styled(
                    format!(" ({}/{})", app.branch_search_index + 1, app.branch_search_matches.len()),
                    Style::default().fg(Color::DarkGray),
                ));
            }
        } else {
            spans.push(Span::styled(" (no matches)", Style::default().fg(Color::Red)));
        }
    } else {
        spans.push(Span::raw("│ Branch: "));
        let branch_text = app.branch_filter.as_deref().unwrap_or("All");
        spans.push(Span::styled(
            format!("[{}]", branch_text),
            if app.branch_filter.is_some() {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ));
    }

    // Search indicator
    if app.mode == Mode::Search || !app.search_query.is_empty() {
        spans.push(Span::raw(" │ Search: "));
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
            if app.detail_in_files {
                "n/N:files  p:preview  d:diff  o:open  F:exit  q:quit"
            } else {
                "j/k:move  o:files  O:commit  s:story  p:preview  F:browse  /:search  f:type  b:branch  q:quit"
            }
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

fn draw_modal(frame: &mut Frame, app: &App, area: Rect) {
    let Some(ref modal) = app.modal else { return };

    // Size based on content
    let popup_width = 70.min(area.width.saturating_sub(4));
    let popup_height = 12.min(area.height.saturating_sub(4));

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    match modal {
        ModalContent::Commit { hash, node_title, git_output } => {
            // Make commit modal much larger to show full git output
            let large_width = (area.width as f32 * 0.9) as u16;
            let large_height = (area.height as f32 * 0.9) as u16;

            let large_area = Rect {
                x: (area.width - large_width) / 2,
                y: (area.height - large_height) / 2,
                width: large_width,
                height: large_height,
            };

            frame.render_widget(Clear, large_area);

            let content = format!(
                "Node: {}\n\n{}",
                node_title, git_output
            );

            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len();
            let scroll_offset = app.modal_scroll.offset.min(total_lines.saturating_sub(1));

            // Style diff-like lines in the commit output
            let styled_lines: Vec<Line> = lines.iter().map(|line| {
                if line.starts_with('+') && !line.starts_with("+++") {
                    Line::from(Span::styled(*line, Style::default().fg(Color::Green)))
                } else if line.starts_with('-') && !line.starts_with("---") {
                    Line::from(Span::styled(*line, Style::default().fg(Color::Red)))
                } else if line.starts_with("@@") {
                    Line::from(Span::styled(*line, Style::default().fg(Color::Cyan)))
                } else if line.starts_with("diff ") || line.starts_with("index ") {
                    Line::from(Span::styled(*line, Style::default().fg(Color::Yellow)))
                } else {
                    Line::from(Span::raw(*line))
                }
            }).collect();

            let title_text = format!(" Commit: {} [j/k:scroll g/G:top/bot q:close] (line {}/{}) ",
                hash, scroll_offset + 1, total_lines);

            let modal_widget = Paragraph::new(styled_lines)
                .block(
                    Block::default()
                        .title(title_text)
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Yellow)),
                )
                .scroll((scroll_offset as u16, 0))
                .style(Style::default().fg(Color::White).bg(Color::Black));

            frame.render_widget(modal_widget, large_area);
            return; // Skip the default popup_area
        }
        ModalContent::NodeDetail { node_id } => {
            let node_info = if let Some(node) = app.graph.nodes.iter().find(|n| n.id == *node_id) {
                format!("\n  {} - {}\n\n  {}", node.node_type.to_uppercase(), node.title,
                    node.description.as_deref().unwrap_or("No description"))
            } else {
                format!("Node {} not found", node_id)
            };

            let modal_widget = Paragraph::new(node_info)
                .block(
                    Block::default()
                        .title(" Node Detail ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Cyan)),
                )
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(Color::White).bg(Color::Black));

            frame.render_widget(modal_widget, popup_area);
        }
        ModalContent::GoalStory { goal_id } => {
            draw_goal_story_modal(frame, app, *goal_id, area);
            return; // Uses its own sizing
        }
        ModalContent::FilePreview { path, content } => {
            draw_file_preview_modal(frame, app, area, path, content);
            return;
        }
        ModalContent::FileDiff { path, diff } => {
            draw_diff_modal(frame, app, area, path, diff);
            return;
        }
    }
}

/// Draw file preview modal with syntect syntax highlighting
fn draw_file_preview_modal(frame: &mut Frame, app: &App, area: Rect, path: &str, content: &str) {
    let popup_width = (area.width as f32 * 0.9) as u16;
    let popup_height = (area.height as f32 * 0.9) as u16;

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    // Get file extension for syntax detection
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("txt");

    // Get syntax for this file type
    let syntax = PS.find_syntax_by_extension(extension)
        .unwrap_or_else(|| PS.find_syntax_plain_text());

    let theme = &TS.themes["base16-ocean.dark"];
    let mut highlighter = HighlightLines::new(syntax, theme);

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let scroll_offset = app.modal_scroll.offset.min(total_lines.saturating_sub(1));

    // Build syntax-highlighted lines - convert to owned spans
    let styled_lines: Vec<Line> = lines.iter().enumerate().map(|(i, line)| {
        let line_num = format!("{:4} │ ", i + 1);
        let code_with_newline = format!("{}\n", line);

        let mut spans: Vec<Span> = vec![
            Span::styled(line_num, Style::default().fg(Color::DarkGray))
        ];

        // Highlight the line and convert syntect styles to ratatui styles
        if let Ok(highlighted) = highlighter.highlight_line(&code_with_newline, &PS) {
            for (style, text) in highlighted {
                let ratatui_style = syntect_to_ratatui_style(style);
                // Clone the text to make it owned
                spans.push(Span::styled(text.to_string(), ratatui_style));
            }
        } else {
            spans.push(Span::raw(line.to_string()));
        }

        Line::from(spans)
    }).collect();

    let scroll_info = format!(" File Preview - {} [j/k:scroll g/G:top/bot o:open q:close] (line {}/{}) ",
        path, scroll_offset + 1, total_lines);

    let modal_widget = Paragraph::new(styled_lines)
        .block(
            Block::default()
                .title(scroll_info)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .scroll((scroll_offset as u16, 0))
        .style(Style::default().bg(Color::Black));

    frame.render_widget(modal_widget, popup_area);
}

/// Convert syntect Style to ratatui Style
fn syntect_to_ratatui_style(syntect_style: SyntectStyle) -> Style {
    let fg = syntect_style.foreground;
    Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b))
}

/// Draw diff modal with +/- coloring
fn draw_diff_modal(frame: &mut Frame, app: &App, area: Rect, path: &str, diff: &str) {
    let popup_width = (area.width as f32 * 0.9) as u16;
    let popup_height = (area.height as f32 * 0.9) as u16;

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    let lines: Vec<&str> = diff.lines().collect();
    let total_lines = lines.len();
    let scroll_offset = app.modal_scroll.offset.min(total_lines.saturating_sub(1));

    // Build styled lines for diff content
    let styled_lines: Vec<Line> = lines.iter().map(|line| {
        if line.starts_with('+') && !line.starts_with("+++") {
            Line::from(Span::styled(*line, Style::default().fg(Color::Green)))
        } else if line.starts_with('-') && !line.starts_with("---") {
            Line::from(Span::styled(*line, Style::default().fg(Color::Red)))
        } else if line.starts_with("@@") {
            Line::from(Span::styled(*line, Style::default().fg(Color::Cyan)))
        } else if line.starts_with("diff ") || line.starts_with("index ") || line.starts_with("+++") || line.starts_with("---") {
            Line::from(Span::styled(*line, Style::default().fg(Color::Yellow)))
        } else {
            Line::from(Span::raw(*line))
        }
    }).collect();

    let scroll_info = format!(" File Diff - {} [j/k:scroll g/G:top/bot o:open q:close] (line {}/{}) ",
        path, scroll_offset + 1, total_lines);

    let modal_widget = Paragraph::new(styled_lines)
        .block(
            Block::default()
                .title(scroll_info)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .scroll((scroll_offset as u16, 0))
        .style(Style::default().fg(Color::White).bg(Color::Black));

    frame.render_widget(modal_widget, popup_area);
}

fn draw_goal_story_modal(frame: &mut Frame, app: &App, goal_id: i32, area: Rect) {
    // Make the story modal larger - it shows more content
    let popup_width = (area.width as f32 * 0.85) as u16;
    let popup_height = (area.height as f32 * 0.85) as u16;

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    // Get the goal and its descendants
    let descendants = app.get_goal_descendants(goal_id);

    let goal_title = app.get_node_by_id(goal_id)
        .map(|n| n.title.as_str())
        .unwrap_or("Unknown Goal");

    // Build hierarchical text representation
    let mut lines: Vec<Line> = vec![
        Line::from(""),
    ];

    for (_id, node, depth) in &descendants {
        let indent = "  ".repeat(*depth);
        let prefix = if *depth == 0 {
            "🎯 "
        } else {
            match node.node_type.as_str() {
                "decision" => "├─ 🤔 ",
                "option" => "│  ├─ 💡 ",
                "action" => "│  └─ ⚡ ",
                "outcome" => "└─ ✅ ",
                "observation" => "│  📝 ",
                _ => "├─ ",
            }
        };

        let type_color = node_type_color(&node.node_type);
        let type_badge = format!("[{}]", node.node_type.to_uppercase());

        // Truncate title if too long
        let max_title_len = popup_width.saturating_sub(20 + (depth * 2) as u16) as usize;
        let title = if node.title.len() > max_title_len {
            format!("{}...", &node.title[..max_title_len.saturating_sub(3)])
        } else {
            node.title.clone()
        };

        lines.push(Line::from(vec![
            Span::raw(format!("{}{}", indent, prefix)),
            Span::styled(type_badge, Style::default().fg(type_color).bold()),
            Span::raw(" "),
            Span::styled(title, Style::default().fg(Color::White)),
        ]));

        // Add confidence if present
        if let Some(conf) = super::app::App::get_confidence(node) {
            let conf_color = if conf >= 80 {
                Color::Green
            } else if conf >= 50 {
                Color::Yellow
            } else {
                Color::Red
            };
            lines.push(Line::from(vec![
                Span::raw(format!("{}      ", indent)),
                Span::styled(format!("{}%", conf), Style::default().fg(conf_color)),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Press Esc or q to close",
        Style::default().fg(Color::DarkGray),
    )));

    let story_widget = Paragraph::new(lines)
        .block(
            Block::default()
                .title(format!(" Goal Story: {} ", goal_title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)),
        )
        .style(Style::default().bg(Color::Black));

    frame.render_widget(story_widget, popup_area);
}
