//! Detail panel view - shows full node information

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::tui::app::App;
use crate::tui::ui::{node_type_color, node_type_style};

/// Draw the detail panel for the selected node
pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Detail ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let Some(node) = app.selected_node() else {
        let empty = Paragraph::new("Select a node to view details")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(empty, inner_area);
        return;
    };

    // Get metadata
    let confidence = App::get_confidence(node);
    let commit = App::get_commit(node);
    let files = App::get_files(node);
    let branch = App::get_branch(node);

    // Build content lines
    let mut lines: Vec<Line> = vec![];

    // Type badge with confidence
    let mut header_spans = vec![
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
        header_spans.push(Span::styled(
            format!("{}%", conf),
            Style::default().fg(conf_color).bold(),
        ));
    }

    lines.push(Line::from(header_spans));
    lines.push(Line::from(""));

    // Title
    lines.push(Line::from(Span::styled(
        &node.title,
        Style::default().fg(Color::White).bold(),
    )));
    lines.push(Line::from(""));

    // Description
    if let Some(ref desc) = node.description {
        if !desc.is_empty() {
            for line in desc.lines() {
                lines.push(Line::from(Span::styled(
                    line,
                    Style::default().fg(Color::Gray),
                )));
            }
            lines.push(Line::from(""));
        }
    }

    // Metadata section
    lines.push(Line::from(Span::styled(
        format!("ID: {} │ {} │ {}", node.id, node.status, format_date(&node.created_at)),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    // Separator
    lines.push(Line::from(Span::styled(
        "─".repeat(inner_area.width as usize - 2),
        Style::default().fg(Color::DarkGray),
    )));

    // Connections
    let (incoming, outgoing) = app.get_node_edges(node.id);

    // Incoming edges
    lines.push(Line::from(Span::styled(
        format!("Incoming ({})", incoming.len()),
        Style::default().fg(Color::Cyan).bold(),
    )));

    for edge in incoming.iter().take(5) {
        if let Some(from_node) = app.get_node_by_id(edge.from_node_id) {
            let type_color = node_type_color(&from_node.node_type);
            lines.push(Line::from(vec![
                Span::styled("← ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("[{}] ", from_node.node_type.chars().next().unwrap_or('?').to_uppercase()),
                    Style::default().fg(type_color),
                ),
                Span::styled(
                    truncate_str(&from_node.title, inner_area.width as usize - 10),
                    Style::default().fg(Color::White),
                ),
            ]));

            // Show rationale if present
            if let Some(ref rationale) = edge.rationale {
                if !rationale.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", truncate_str(rationale, inner_area.width as usize - 6)),
                        Style::default().fg(Color::DarkGray).italic(),
                    )));
                }
            }
        }
    }
    if incoming.len() > 5 {
        lines.push(Line::from(Span::styled(
            format!("  ... and {} more", incoming.len() - 5),
            Style::default().fg(Color::DarkGray),
        )));
    }

    lines.push(Line::from(""));

    // Outgoing edges
    lines.push(Line::from(Span::styled(
        format!("Outgoing ({})", outgoing.len()),
        Style::default().fg(Color::Yellow).bold(),
    )));

    for edge in outgoing.iter().take(5) {
        if let Some(to_node) = app.get_node_by_id(edge.to_node_id) {
            let type_color = node_type_color(&to_node.node_type);
            let edge_style = match edge.edge_type.as_str() {
                "chosen" => Style::default().fg(Color::Green).bold(),
                "rejected" => Style::default().fg(Color::Red),
                _ => Style::default().fg(Color::Yellow),
            };

            lines.push(Line::from(vec![
                Span::styled("→ ", edge_style),
                Span::styled(
                    format!("[{}] ", to_node.node_type.chars().next().unwrap_or('?').to_uppercase()),
                    Style::default().fg(type_color),
                ),
                Span::styled(
                    truncate_str(&to_node.title, inner_area.width as usize - 10),
                    Style::default().fg(Color::White),
                ),
            ]));

            // Show rationale if present
            if let Some(ref rationale) = edge.rationale {
                if !rationale.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", truncate_str(rationale, inner_area.width as usize - 6)),
                        Style::default().fg(Color::DarkGray).italic(),
                    )));
                }
            }
        }
    }
    if outgoing.len() > 5 {
        lines.push(Line::from(Span::styled(
            format!("  ... and {} more", outgoing.len() - 5),
            Style::default().fg(Color::DarkGray),
        )));
    }

    lines.push(Line::from(""));

    // Separator
    lines.push(Line::from(Span::styled(
        "─".repeat(inner_area.width as usize - 2),
        Style::default().fg(Color::DarkGray),
    )));

    // Files
    if !files.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("Files: {}", files.join(", ")),
            Style::default().fg(Color::Magenta),
        )));
    }

    // Commit
    if let Some(ref hash) = commit {
        lines.push(Line::from(vec![
            Span::styled("Commit: ", Style::default().fg(Color::DarkGray)),
            Span::styled(hash, Style::default().fg(Color::Yellow)),
        ]));
    }

    // Branch
    if let Some(ref br) = branch {
        lines.push(Line::from(vec![
            Span::styled("Branch: ", Style::default().fg(Color::DarkGray)),
            Span::styled(br, Style::default().fg(Color::Green)),
        ]));
    }

    lines.push(Line::from(""));

    // Action hints
    if !files.is_empty() {
        lines.push(Line::from(Span::styled(
            "[o] Open in editor",
            Style::default().fg(Color::Cyan),
        )));
    }
    if commit.is_some() {
        lines.push(Line::from(Span::styled(
            "[O] View commit",
            Style::default().fg(Color::Cyan),
        )));
    }
    lines.push(Line::from(Span::styled(
        "[Enter] Toggle panel",
        Style::default().fg(Color::Cyan),
    )));

    let detail = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll as u16, 0));

    frame.render_widget(detail, inner_area);
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

fn format_date(ts: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        dt.format("%m/%d/%y").to_string()
    } else {
        ts.split('T').next().unwrap_or(ts).to_string()
    }
}
