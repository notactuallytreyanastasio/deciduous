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
    let prompt = App::get_prompt(node);

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

    // Prompt section - show the user prompt that triggered this node
    if let Some(ref p) = prompt {
        if !p.is_empty() {
            lines.push(Line::from(Span::styled(
                "─── Prompt ───",
                Style::default().fg(Color::LightBlue).bold(),
            )));
            lines.push(Line::from(""));

            // Word-wrap the prompt text for readability
            let prompt_width = (inner_area.width as usize).saturating_sub(4).max(20);
            for line in p.lines() {
                // Simple word wrap
                let words: Vec<&str> = line.split_whitespace().collect();
                let mut current_line = String::new();

                for word in words {
                    if current_line.is_empty() {
                        current_line = format!("  {}", word);
                    } else if current_line.len() + word.len() < prompt_width {
                        current_line.push(' ');
                        current_line.push_str(word);
                    } else {
                        lines.push(Line::from(Span::styled(
                            current_line.clone(),
                            Style::default().fg(Color::LightBlue).italic(),
                        )));
                        current_line = format!("  {}", word);
                    }
                }

                if !current_line.is_empty() {
                    lines.push(Line::from(Span::styled(
                        current_line,
                        Style::default().fg(Color::LightBlue).italic(),
                    )));
                }
            }
            lines.push(Line::from(""));
        }
    }

    // Metadata section
    lines.push(Line::from(Span::styled(
        format!(
            "ID: {} │ {} │ {}",
            node.id,
            node.status,
            format_date(&node.created_at)
        ),
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

    for edge in incoming.iter() {
        if let Some(from_node) = app.get_node_by_id(edge.from_node_id) {
            let type_color = node_type_color(&from_node.node_type);
            let edge_indicator = match edge.edge_type.as_str() {
                "chosen" => "←✓",
                "rejected" => "←✗",
                _ => "← ",
            };
            let edge_style = match edge.edge_type.as_str() {
                "chosen" => Style::default().fg(Color::Green),
                "rejected" => Style::default().fg(Color::Red),
                _ => Style::default().fg(Color::Cyan),
            };

            // Type badge
            lines.push(Line::from(vec![
                Span::styled(edge_indicator, edge_style),
                Span::raw(" "),
                Span::styled(
                    format!("[{}]", from_node.node_type.to_uppercase()),
                    Style::default().fg(type_color).bold(),
                ),
                Span::styled(
                    format!(" #{}", from_node.id),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));

            // Full title (no truncation)
            lines.push(Line::from(Span::styled(
                format!("   {}", from_node.title),
                Style::default().fg(Color::White),
            )));

            // Show rationale if present
            if let Some(ref rationale) = edge.rationale {
                if !rationale.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("   └ {}", rationale),
                        Style::default().fg(Color::DarkGray).italic(),
                    )));
                }
            }
        }
    }
    lines.push(Line::from(""));

    // Outgoing edges
    lines.push(Line::from(Span::styled(
        format!("Outgoing ({})", outgoing.len()),
        Style::default().fg(Color::Yellow).bold(),
    )));

    for edge in outgoing.iter() {
        if let Some(to_node) = app.get_node_by_id(edge.to_node_id) {
            let type_color = node_type_color(&to_node.node_type);
            let edge_indicator = match edge.edge_type.as_str() {
                "chosen" => "→✓",
                "rejected" => "→✗",
                _ => "→ ",
            };
            let edge_style = match edge.edge_type.as_str() {
                "chosen" => Style::default().fg(Color::Green).bold(),
                "rejected" => Style::default().fg(Color::Red),
                _ => Style::default().fg(Color::Yellow),
            };

            // Type badge
            lines.push(Line::from(vec![
                Span::styled(edge_indicator, edge_style),
                Span::raw(" "),
                Span::styled(
                    format!("[{}]", to_node.node_type.to_uppercase()),
                    Style::default().fg(type_color).bold(),
                ),
                Span::styled(
                    format!(" #{}", to_node.id),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));

            // Full title (no truncation)
            lines.push(Line::from(Span::styled(
                format!("   {}", to_node.title),
                Style::default().fg(Color::White),
            )));

            // Show rationale if present
            if let Some(ref rationale) = edge.rationale {
                if !rationale.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("   └ {}", rationale),
                        Style::default().fg(Color::DarkGray).italic(),
                    )));
                }
            }
        }
    }
    lines.push(Line::from(""));

    // Separator
    lines.push(Line::from(Span::styled(
        "─".repeat(inner_area.width as usize - 2),
        Style::default().fg(Color::DarkGray),
    )));

    // Files - interactive list
    if !files.is_empty() {
        let file_header = if app.detail_in_files {
            format!(
                "─── Files ({}/{}) [F:exit n/N:nav p:preview d:diff o:open] ───",
                app.detail_file_index + 1,
                files.len()
            )
        } else {
            format!("─── Files ({}) [F:browse p:preview] ───", files.len())
        };
        lines.push(Line::from(Span::styled(
            file_header,
            Style::default().fg(Color::Magenta).bold(),
        )));

        for (i, file) in files.iter().enumerate() {
            let is_selected = app.detail_in_files && i == app.detail_file_index;
            let prefix = if is_selected { "▶ " } else { "  " };
            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Magenta)
            } else {
                Style::default().fg(Color::Magenta)
            };
            lines.push(Line::from(Span::styled(
                format!("{}{}", prefix, file),
                style,
            )));
        }
        lines.push(Line::from(""));
    }

    // Commit - show full info from git
    if let Some(ref hash) = commit {
        lines.push(Line::from(Span::styled(
            "─── Commit ───",
            Style::default().fg(Color::Yellow).bold(),
        )));

        // Run git log to get commit info
        if let Ok(output) = std::process::Command::new("git")
            .args(["log", "-1", "--format=%h %s%n%n%b", hash])
            .output()
        {
            let commit_info = String::from_utf8_lossy(&output.stdout);
            for line in commit_info.lines() {
                if !line.is_empty() {
                    lines.push(Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(Color::White),
                    )));
                }
            }
        } else {
            lines.push(Line::from(vec![
                Span::styled("Hash: ", Style::default().fg(Color::DarkGray)),
                Span::styled(hash, Style::default().fg(Color::Yellow)),
            ]));
        }
        lines.push(Line::from(""));
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

fn format_date(ts: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        dt.format("%m/%d/%y").to_string()
    } else {
        ts.split('T').next().unwrap_or(ts).to_string()
    }
}
