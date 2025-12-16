//! Timeline view - scrollable list of nodes with details

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState},
};

use crate::tui::app::App;
use crate::tui::ui::node_type_style;

/// Draw the timeline view (node list)
pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Timeline ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if app.filtered_nodes.is_empty() {
        let empty = ratatui::widgets::Paragraph::new("No nodes match your filters")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(empty, inner_area);
        return;
    }

    // Build list items
    let items: Vec<ListItem> = app
        .filtered_nodes
        .iter()
        .enumerate()
        .skip(app.scroll_offset)
        .take(inner_area.height as usize)
        .map(|(idx, node)| {
            let is_selected = idx == app.selected_index;

            // Build the node display
            let confidence = App::get_confidence(node);
            let commit = App::get_commit(node);

            // First line: type badge, confidence, commit hash
            let mut line1_spans = vec![
                Span::styled(
                    format!(" {} ", node.node_type.to_uppercase()),
                    node_type_style(&node.node_type),
                ),
                Span::raw(" "),
            ];

            if let Some(conf) = confidence {
                let conf_color = if conf >= 90 {
                    Color::Green
                } else if conf >= 70 {
                    Color::Yellow
                } else {
                    Color::Red
                };
                line1_spans.push(Span::styled(
                    format!("{}%", conf),
                    Style::default().fg(conf_color),
                ));
                line1_spans.push(Span::raw(" "));
            }

            if let Some(ref hash) = commit {
                line1_spans.push(Span::styled(
                    format!("#{}", &hash[..7.min(hash.len())]),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            // Second line: title (truncated)
            let title = truncate_str(&node.title, inner_area.width as usize - 4);

            // Third line: timestamp
            let timestamp = format_timestamp(&node.created_at);

            let content = vec![
                Line::from(line1_spans),
                Line::from(Span::styled(
                    format!("  {}", title),
                    if is_selected {
                        Style::default().fg(Color::White).bold()
                    } else {
                        Style::default().fg(Color::White)
                    },
                )),
                Line::from(Span::styled(
                    format!("  {}", timestamp),
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let style = if is_selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items);

    // Create list state for highlighting
    let mut state = ListState::default();
    state.select(Some(app.selected_index.saturating_sub(app.scroll_offset)));

    frame.render_stateful_widget(list, inner_area, &mut state);

    // Draw scroll indicator if there are more items
    if app.filtered_nodes.len() > inner_area.height as usize {
        let scroll_pos = if app.filtered_nodes.is_empty() {
            0
        } else {
            (app.scroll_offset * (inner_area.height as usize - 1)) / app.filtered_nodes.len()
        };

        // Simple scroll bar on the right edge
        for i in 0..inner_area.height {
            let is_thumb = i as usize == scroll_pos;
            let char = if is_thumb { '█' } else { '│' };
            let style = if is_thumb {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let cell = ratatui::buffer::Cell::default()
                .set_char(char)
                .set_style(style)
                .clone();

            if area.x + area.width > 0 {
                let x = area.x + area.width - 1;
                let y = inner_area.y + i;
                if y < area.y + area.height - 1 {
                    frame.buffer_mut()[(x, y)] = cell;
                }
            }
        }
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let char_len = max_len.saturating_sub(3);
        let truncated: String = s.chars().take(char_len).collect();
        format!("{}...", truncated)
    }
}

fn format_timestamp(ts: &str) -> String {
    // Parse ISO 8601 and format nicely
    // Input: "2024-12-10T18:04:00Z"
    // Output: "Dec 10 6:04 PM"
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        dt.format("%b %d %l:%M %p").to_string()
    } else {
        // Try without timezone
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S") {
            dt.format("%b %d %l:%M %p").to_string()
        } else {
            ts.to_string()
        }
    }
}
