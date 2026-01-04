//! UI rendering for the TUI

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

use super::app::{App, ModalContent, ModalSection, Mode, View};
use super::views::{dag, detail, roadmap, timeline};
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
        Constraint::Length(1), // Header
        Constraint::Length(1), // Filter bar
        Constraint::Min(10),   // Content
        Constraint::Length(1), // Footer/status
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
                let content_layout =
                    Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
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
        View::Roadmap => {
            roadmap::draw(frame, &app.roadmap_state, main_layout[2]);
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
        View::Roadmap => "Roadmap",
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

    let header =
        Paragraph::new(header_text).style(Style::default().bg(Color::Blue).fg(Color::White).bold());

    frame.render_widget(header, area);
}

fn draw_filter_bar(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![Span::raw(" Filters: ")];

    // Type filter buttons
    let types = [
        "All",
        "goal",
        "decision",
        "option",
        "action",
        "outcome",
        "observation",
    ];
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
        spans.push(Span::styled(
            "_",
            Style::default().fg(Color::Cyan).rapid_blink(),
        ));

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
                    format!(
                        " ({}/{})",
                        app.branch_search_index + 1,
                        app.branch_search_matches.len()
                    ),
                    Style::default().fg(Color::DarkGray),
                ));
            }
        } else {
            spans.push(Span::styled(
                " (no matches)",
                Style::default().fg(Color::Red),
            ));
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
            spans.push(Span::styled(
                "_",
                Style::default().fg(Color::Cyan).rapid_blink(),
            ));
        }
    }

    let filter_bar = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::DarkGray));

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
        View::Dag => "h/j/k/l:pan  +/-:zoom  0:reset  Tab:Timeline  ?:help  q:quit",
        View::Roadmap => "j/k:move  r:refresh  Tab:Timeline  ?:help  q:quit",
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
    let popup_height = 32.min(area.height.saturating_sub(4));

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
  Tab          Switch view
  r            Refresh
  q            Quit

  DAG View
  ─────────────────────────────────
  h/j/k/l      Pan view
  +/-          Zoom in/out
  0            Reset zoom

  Roadmap View
  ─────────────────────────────────
  j/k, ↑/↓     Move up/down
  Enter        Toggle detail panel
  o            Open GitHub issue
  c            Toggle checkbox
  Shift+Tab    Toggle Active/Completed
  r            Refresh

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
        ModalContent::Commit {
            hash,
            node_title,
            commit_message,
            diff_lines,
            files,
        } => {
            draw_commit_modal(
                frame,
                app,
                area,
                hash,
                node_title,
                commit_message,
                diff_lines,
                files,
            );
        }
        ModalContent::NodeDetail { node_id } => {
            let node_info = if let Some(node) = app.graph.nodes.iter().find(|n| n.id == *node_id) {
                format!(
                    "\n  {} - {}\n\n  {}",
                    node.node_type.to_uppercase(),
                    node.title,
                    node.description.as_deref().unwrap_or("No description")
                )
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
        }
        ModalContent::FilePreview { path, content } => {
            draw_file_preview_modal(frame, app, area, path, content);
        }
        ModalContent::FileDiff { path, diff } => {
            draw_diff_modal(frame, app, area, path, diff);
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
    let syntax = PS
        .find_syntax_by_extension(extension)
        .unwrap_or_else(|| PS.find_syntax_plain_text());

    let theme = &TS.themes["base16-mocha.dark"];
    let mut highlighter = HighlightLines::new(syntax, theme);

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let scroll_offset = app.modal_scroll.offset.min(total_lines.saturating_sub(1));

    // Build syntax-highlighted lines - convert to owned spans
    let styled_lines: Vec<Line> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let line_num = format!("{:4} │ ", i + 1);
            let code_with_newline = format!("{}\n", line);

            let mut spans: Vec<Span> =
                vec![Span::styled(line_num, Style::default().fg(Color::DarkGray))];

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
        })
        .collect();

    let scroll_info = format!(
        " File Preview - {} [j/k:scroll g/G:top/bot o:open q:close] (line {}/{}) ",
        path,
        scroll_offset + 1,
        total_lines
    );

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

/// Draw diff modal with syntax highlighting + diff coloring
///
/// This reads the actual file to build proper syntax state, then applies
/// highlighting to the diff lines with the correct context.
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

    // Get file extension for syntax detection
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("txt");

    // Get syntax for this file type
    let syntax = PS
        .find_syntax_by_extension(extension)
        .unwrap_or_else(|| PS.find_syntax_plain_text());

    let theme = &TS.themes["base16-mocha.dark"];

    // Read the actual file to build syntax highlighting state
    // This allows us to highlight diff lines with proper context
    let file_lines: Vec<String> = std::fs::read_to_string(path)
        .map(|s| s.lines().map(|l| l.to_string()).collect())
        .unwrap_or_default();

    // Pre-highlight the entire file to build a map of line -> styled spans
    let mut file_highlighter = HighlightLines::new(syntax, theme);
    let mut highlighted_file: std::collections::HashMap<String, Vec<(SyntectStyle, String)>> =
        std::collections::HashMap::new();

    for line in &file_lines {
        let code_with_newline = format!("{}\n", line);
        if let Ok(highlighted) = file_highlighter.highlight_line(&code_with_newline, &PS) {
            let owned: Vec<(SyntectStyle, String)> = highlighted
                .into_iter()
                .map(|(s, t)| (s, t.to_string()))
                .collect();
            highlighted_file.insert(line.clone(), owned);
        }
    }

    let diff_lines: Vec<&str> = diff.lines().collect();
    let total_lines = diff_lines.len();
    let scroll_offset = app.modal_scroll.offset.min(total_lines.saturating_sub(1));

    // Build styled lines with syntax highlighting + diff colors
    let styled_lines: Vec<Line> = diff_lines
        .iter()
        .map(|line| {
            if line.starts_with("@@") {
                // Hunk header - cyan
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::Cyan),
                ))
            } else if line.starts_with("diff ")
                || line.starts_with("index ")
                || line.starts_with("+++")
                || line.starts_with("---")
            {
                // Diff metadata - yellow
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::Yellow),
                ))
            } else if line.starts_with('+') && line.len() > 1 {
                // Added line - use highlighted version with green tint
                let code = &line[1..];
                let mut spans: Vec<Span> =
                    vec![Span::styled("+", Style::default().fg(Color::Green).bold())];

                if let Some(highlighted) = highlighted_file.get(code) {
                    for (style, text) in highlighted {
                        let fg = style.foreground;
                        // Tint towards green while preserving some syntax color
                        let tinted = Color::Rgb(
                            (fg.r as u16 * 6 / 10) as u8,
                            ((fg.g as u16 * 6 / 10) + 90).min(255) as u8,
                            (fg.b as u16 * 6 / 10) as u8,
                        );
                        spans.push(Span::styled(text.clone(), Style::default().fg(tinted)));
                    }
                } else {
                    spans.push(Span::styled(
                        code.to_string(),
                        Style::default().fg(Color::Green),
                    ));
                }
                Line::from(spans)
            } else if line.starts_with('-') && line.len() > 1 {
                // Removed line - use highlighted version with red tint
                let code = &line[1..];
                let mut spans: Vec<Span> =
                    vec![Span::styled("-", Style::default().fg(Color::Red).bold())];

                if let Some(highlighted) = highlighted_file.get(code) {
                    for (style, text) in highlighted {
                        let fg = style.foreground;
                        // Tint towards red while preserving some syntax color
                        let tinted = Color::Rgb(
                            ((fg.r as u16 * 6 / 10) + 90).min(255) as u8,
                            (fg.g as u16 * 6 / 10) as u8,
                            (fg.b as u16 * 6 / 10) as u8,
                        );
                        spans.push(Span::styled(text.clone(), Style::default().fg(tinted)));
                    }
                } else {
                    spans.push(Span::styled(
                        code.to_string(),
                        Style::default().fg(Color::Red),
                    ));
                }
                Line::from(spans)
            } else if line.starts_with(' ') && line.len() > 1 {
                // Context line - use highlighted version normally
                let code = &line[1..];
                let mut spans: Vec<Span> = vec![Span::styled(" ", Style::default())];

                if let Some(highlighted) = highlighted_file.get(code) {
                    for (style, text) in highlighted {
                        let ratatui_style = syntect_to_ratatui_style(*style);
                        spans.push(Span::styled(text.clone(), ratatui_style));
                    }
                } else {
                    spans.push(Span::raw(code.to_string()));
                }
                Line::from(spans)
            } else if line.starts_with('+') || line.starts_with('-') {
                // Empty +/- line
                let color = if line.starts_with('+') {
                    Color::Green
                } else {
                    Color::Red
                };
                Line::from(Span::styled(line.to_string(), Style::default().fg(color)))
            } else {
                Line::from(Span::raw(line.to_string()))
            }
        })
        .collect();

    let scroll_info = format!(
        " File Diff - {} [j/k:scroll g/G:top/bot o:open q:close] (line {}/{}) ",
        path,
        scroll_offset + 1,
        total_lines
    );

    let modal_widget = Paragraph::new(styled_lines)
        .block(
            Block::default()
                .title(scroll_info)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .scroll((scroll_offset as u16, 0))
        .style(Style::default().bg(Color::Black));

    frame.render_widget(modal_widget, popup_area);
}

/// Draw commit modal with split view: commit info on top, diff on bottom
fn draw_commit_modal(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    hash: &str,
    node_title: &str,
    commit_message: &str,
    diff_lines: &[super::app::StyledDiffLine],
    files: &[String],
) {
    let popup_width = (area.width as f32 * 0.9) as u16;
    let popup_height = (area.height as f32 * 0.9) as u16;

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    // Calculate inner area (inside the outer border)
    let inner_area = Rect {
        x: popup_area.x + 1,
        y: popup_area.y + 1,
        width: popup_area.width.saturating_sub(2),
        height: popup_area.height.saturating_sub(2),
    };

    // Calculate message lines height (commit message + header info)
    let message_lines: Vec<&str> = commit_message.lines().collect();
    // Header (3 lines) + message + files list + spacing
    let files_display = if files.len() > 3 {
        format!(
            "{} (+{} more)",
            files.iter().take(3).cloned().collect::<Vec<_>>().join(", "),
            files.len() - 3
        )
    } else {
        files.join(", ")
    };
    let top_section_height = 6 + message_lines.len().min(8) as u16; // Max 8 lines of commit message

    // Split into top (commit info) and bottom (diff) sections
    let layout = Layout::vertical([
        Constraint::Length(top_section_height),
        Constraint::Min(5), // Diff section takes the rest
    ])
    .split(inner_area);

    let top_area = layout[0];
    let bottom_area = layout[1];

    // Determine which section is focused
    let top_focused = app.commit_modal.section == ModalSection::Top;
    let bottom_focused = app.commit_modal.section == ModalSection::Bottom;

    // === Top section: Commit info ===
    let top_border_color = if top_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let mut top_lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("Commit: ", Style::default().fg(Color::DarkGray)),
            Span::styled(hash, Style::default().fg(Color::Yellow).bold()),
        ]),
        Line::from(vec![
            Span::styled("Node: ", Style::default().fg(Color::DarkGray)),
            Span::styled(node_title, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Files: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&files_display, Style::default().fg(Color::Magenta)),
        ]),
        Line::from(""),
        Line::styled("─── Message ───", Style::default().fg(Color::DarkGray)),
    ];

    // Add commit message lines (capped at 8)
    for line in message_lines.iter().take(8) {
        top_lines.push(Line::from(Span::styled(
            *line,
            Style::default().fg(Color::White),
        )));
    }
    if message_lines.len() > 8 {
        top_lines.push(Line::from(Span::styled(
            format!("... ({} more lines)", message_lines.len() - 8),
            Style::default().fg(Color::DarkGray),
        )));
    }

    let top_widget = Paragraph::new(top_lines)
        .block(
            Block::default()
                .title(format!(" Commit {} ", &hash[..7.min(hash.len())]))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(top_border_color)),
        )
        .style(Style::default().bg(Color::Black));

    frame.render_widget(top_widget, top_area);

    // === Bottom section: Diff (pre-processed, no I/O during render) ===
    let bottom_border_color = if bottom_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let total_diff_lines = diff_lines.len();
    let scroll_offset = app
        .commit_modal
        .diff_scroll
        .min(total_diff_lines.saturating_sub(1));

    // Convert pre-processed diff lines to styled Lines using cached syntax highlighting
    use super::app::DiffLineType;
    let styled_diff_lines: Vec<Line> = diff_lines
        .iter()
        .map(|diff_line| {
            match diff_line.line_type {
                DiffLineType::Hunk => Line::from(Span::styled(
                    diff_line.content.clone(),
                    Style::default().fg(Color::Cyan),
                )),
                DiffLineType::Header => Line::from(Span::styled(
                    diff_line.content.clone(),
                    Style::default().fg(Color::Yellow),
                )),
                DiffLineType::Added => {
                    // Show '+' in green, then syntax highlighted content
                    let mut spans = vec![Span::styled("+", Style::default().fg(Color::Green))];
                    if diff_line.styled_spans.is_empty() {
                        // Fallback: just show content in green
                        let content = if diff_line.content.len() > 1 {
                            &diff_line.content[1..]
                        } else {
                            ""
                        };
                        spans.push(Span::styled(
                            content.to_string(),
                            Style::default().fg(Color::Green),
                        ));
                    } else {
                        // Use pre-computed syntax highlighting with green background tint
                        for (color, text) in &diff_line.styled_spans {
                            spans.push(Span::styled(text.clone(), Style::default().fg(*color)));
                        }
                    }
                    Line::from(spans)
                }
                DiffLineType::Removed => {
                    // Show '-' in red, then syntax highlighted content
                    let mut spans = vec![Span::styled("-", Style::default().fg(Color::Red))];
                    if diff_line.styled_spans.is_empty() {
                        // Fallback: just show content in red
                        let content = if diff_line.content.len() > 1 {
                            &diff_line.content[1..]
                        } else {
                            ""
                        };
                        spans.push(Span::styled(
                            content.to_string(),
                            Style::default().fg(Color::Red),
                        ));
                    } else {
                        // Use pre-computed syntax highlighting
                        for (color, text) in &diff_line.styled_spans {
                            spans.push(Span::styled(text.clone(), Style::default().fg(*color)));
                        }
                    }
                    Line::from(spans)
                }
                DiffLineType::Context => {
                    // Show ' ' then syntax highlighted content
                    let mut spans = vec![Span::raw(" ")];
                    if diff_line.styled_spans.is_empty() {
                        let content = if diff_line.content.len() > 1 {
                            &diff_line.content[1..]
                        } else {
                            ""
                        };
                        spans.push(Span::raw(content.to_string()));
                    } else {
                        for (color, text) in &diff_line.styled_spans {
                            spans.push(Span::styled(text.clone(), Style::default().fg(*color)));
                        }
                    }
                    Line::from(spans)
                }
                DiffLineType::Other => Line::from(Span::raw(diff_line.content.clone())),
            }
        })
        .collect();

    let scroll_info = if total_diff_lines == 0 {
        " Diff [q:close] (no diff available) ".to_string()
    } else {
        format!(
            " Diff [j/k:navigate g/G:top/bot q:close] (line {}/{}) ",
            scroll_offset + 1,
            total_diff_lines
        )
    };

    // If diff is empty, show a helpful message
    let display_lines = if styled_diff_lines.is_empty() {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No diff content available.",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  This could mean:",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  • The commit hash doesn't exist in this repository",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  • The commit has no file changes (merge commit, etc.)",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  • Git couldn't be accessed",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    } else {
        styled_diff_lines
    };

    let bottom_widget = Paragraph::new(display_lines)
        .block(
            Block::default()
                .title(scroll_info)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(bottom_border_color)),
        )
        .scroll((scroll_offset as u16, 0))
        .style(Style::default().bg(Color::Black));

    frame.render_widget(bottom_widget, bottom_area);

    // Draw outer border around the whole modal
    let outer_border = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue))
        .title(" Commit Details [q:close] ");

    frame.render_widget(outer_border, popup_area);
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

    let goal_title = app
        .get_node_by_id(goal_id)
        .map(|n| n.title.as_str())
        .unwrap_or("Unknown Goal");

    // Build hierarchical text representation
    let mut lines: Vec<Line> = vec![Line::from("")];

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
