//! DAG View - Hierarchical graph visualization

use std::collections::{HashMap, HashSet, VecDeque};

use ratatui::{
    prelude::*,
    widgets::{
        canvas::{Canvas, Line as CanvasLine, Rectangle},
        Block, Borders, Paragraph,
    },
};

use crate::tui::app::App;
use crate::tui::ui::node_type_color;
use crate::DecisionNode;

/// Node position in the DAG layout
#[derive(Debug, Clone)]
struct NodePosition {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    node_id: i32,
}

/// Draw the DAG view
pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(format!(
            " DAG │ zoom: {}% │ [+/-] zoom  [h/j/k/l] pan  [0] reset ",
            (app.dag_zoom * 100.0) as i32
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if app.graph.nodes.is_empty() {
        let empty = Paragraph::new("No nodes in graph")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(empty, inner_area);
        return;
    }

    // Calculate hierarchical layout
    let positions = calculate_layout(&app.graph.nodes, &app.graph.edges, app);

    // Draw using canvas
    let canvas = Canvas::default()
        .x_bounds([
            app.dag_offset_x as f64 - (inner_area.width as f64 / 2.0) / app.dag_zoom as f64,
            app.dag_offset_x as f64 + (inner_area.width as f64 / 2.0) / app.dag_zoom as f64,
        ])
        .y_bounds([
            app.dag_offset_y as f64 - (inner_area.height as f64) / app.dag_zoom as f64,
            app.dag_offset_y as f64 + (inner_area.height as f64) / app.dag_zoom as f64,
        ])
        .paint(|ctx| {
            // Draw edges first (behind nodes)
            for edge in &app.graph.edges {
                let from_pos = positions.iter().find(|p| p.node_id == edge.from_node_id);
                let to_pos = positions.iter().find(|p| p.node_id == edge.to_node_id);

                if let (Some(from), Some(to)) = (from_pos, to_pos) {
                    let color = match edge.edge_type.as_str() {
                        "chosen" => Color::Green,
                        "rejected" => Color::Red,
                        "blocks" => Color::Red,
                        "enables" => Color::Cyan,
                        _ => Color::DarkGray,
                    };

                    // Draw line from bottom of source to top of target
                    ctx.draw(&CanvasLine {
                        x1: from.x + from.width / 2.0,
                        y1: from.y - from.height,
                        x2: to.x + to.width / 2.0,
                        y2: to.y,
                        color,
                    });
                }
            }

            // Draw nodes
            for pos in &positions {
                if let Some(node) = app.graph.nodes.iter().find(|n| n.id == pos.node_id) {
                    let color = node_type_color(&node.node_type);

                    // Draw node box
                    ctx.draw(&Rectangle {
                        x: pos.x,
                        y: pos.y - pos.height,
                        width: pos.width,
                        height: pos.height,
                        color,
                    });

                    // Draw node label (type abbreviation)
                    let label = format!(
                        "{}",
                        node.node_type.chars().next().unwrap_or('?').to_uppercase()
                    );
                    ctx.print(pos.x + 1.0, pos.y - pos.height / 2.0, label);
                }
            }
        });

    frame.render_widget(canvas, inner_area);

    // Draw legend in corner
    let legend_area = Rect {
        x: inner_area.x + inner_area.width.saturating_sub(25),
        y: inner_area.y,
        width: 24,
        height: 8,
    };

    let legend_text = vec![
        Line::from(vec![
            Span::styled("■", Style::default().fg(Color::Green)),
            Span::raw(" goal"),
        ]),
        Line::from(vec![
            Span::styled("■", Style::default().fg(Color::Yellow)),
            Span::raw(" decision"),
        ]),
        Line::from(vec![
            Span::styled("■", Style::default().fg(Color::Magenta)),
            Span::raw(" option"),
        ]),
        Line::from(vec![
            Span::styled("■", Style::default().fg(Color::Red)),
            Span::raw(" action"),
        ]),
        Line::from(vec![
            Span::styled("■", Style::default().fg(Color::Cyan)),
            Span::raw(" outcome"),
        ]),
        Line::from(vec![
            Span::styled("■", Style::default().fg(Color::Blue)),
            Span::raw(" observation"),
        ]),
    ];

    let legend = Paragraph::new(legend_text).style(Style::default().bg(Color::Black));
    frame.render_widget(legend, legend_area);
}

/// Calculate hierarchical layout positions for nodes
fn calculate_layout(
    nodes: &[DecisionNode],
    edges: &[crate::DecisionEdge],
    _app: &App,
) -> Vec<NodePosition> {
    if nodes.is_empty() {
        return vec![];
    }

    // Build adjacency lists
    let mut children: HashMap<i32, Vec<i32>> = HashMap::new();
    let mut parents: HashMap<i32, Vec<i32>> = HashMap::new();

    for edge in edges {
        children
            .entry(edge.from_node_id)
            .or_default()
            .push(edge.to_node_id);
        parents
            .entry(edge.to_node_id)
            .or_default()
            .push(edge.from_node_id);
    }

    // Find root nodes (no incoming edges)
    let all_node_ids: HashSet<i32> = nodes.iter().map(|n| n.id).collect();
    let has_parent: HashSet<i32> = edges.iter().map(|e| e.to_node_id).collect();
    let roots: Vec<i32> = all_node_ids.difference(&has_parent).cloned().collect();

    // Assign levels using BFS from roots
    let mut levels: HashMap<i32, usize> = HashMap::new();
    let mut queue: VecDeque<(i32, usize)> = VecDeque::new();

    for root in &roots {
        queue.push_back((*root, 0));
        levels.insert(*root, 0);
    }

    // For orphan nodes (no edges at all), assign level 0
    for node in nodes {
        levels.entry(node.id).or_insert(0);
    }

    while let Some((node_id, level)) = queue.pop_front() {
        if let Some(child_ids) = children.get(&node_id) {
            for &child_id in child_ids {
                let new_level = level + 1;
                let current = levels.get(&child_id).cloned().unwrap_or(0);
                if new_level > current {
                    levels.insert(child_id, new_level);
                    queue.push_back((child_id, new_level));
                }
            }
        }
    }

    // Group nodes by level
    let mut level_groups: HashMap<usize, Vec<i32>> = HashMap::new();
    for (node_id, level) in &levels {
        level_groups.entry(*level).or_default().push(*node_id);
    }

    // Calculate positions
    let node_width = 12.0;
    let node_height = 4.0;
    let h_spacing = 16.0;
    let v_spacing = 8.0;

    let max_level = levels.values().max().cloned().unwrap_or(0);

    let mut positions = Vec::new();

    for (level, node_ids) in &level_groups {
        let count = node_ids.len();
        let total_width = count as f64 * (node_width + h_spacing) - h_spacing;
        let start_x = -total_width / 2.0;

        for (i, &node_id) in node_ids.iter().enumerate() {
            let x = start_x + i as f64 * (node_width + h_spacing);
            // Invert Y so roots are at top (higher Y in canvas = lower on screen)
            let y = (max_level - *level) as f64 * (node_height + v_spacing);

            positions.push(NodePosition {
                x,
                y,
                width: node_width,
                height: node_height,
                node_id,
            });
        }
    }

    positions
}
