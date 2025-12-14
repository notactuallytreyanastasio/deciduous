//! Roadmap view - displays roadmap items with sync status

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::db::RoadmapItem;

/// State for the roadmap view
#[derive(Debug, Clone, Default)]
pub struct RoadmapState {
    pub items: Vec<RoadmapItem>,
    pub selected_index: usize,
    pub scroll_offset: usize,
}

impl RoadmapState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_items(&mut self, items: Vec<RoadmapItem>) {
        self.items = items;
        if self.selected_index >= self.items.len() {
            self.selected_index = self.items.len().saturating_sub(1);
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.ensure_visible(20); // Assume 20 visible items
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.items.len() {
            self.selected_index += 1;
            self.ensure_visible(20);
        }
    }

    pub fn selected_item(&self) -> Option<&RoadmapItem> {
        self.items.get(self.selected_index)
    }

    fn ensure_visible(&mut self, visible_items: usize) {
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + visible_items {
            self.scroll_offset = self.selected_index.saturating_sub(visible_items - 1);
        }
    }
}

/// Draw the roadmap view
pub fn draw(frame: &mut Frame, state: &RoadmapState, area: Rect) {
    let block = Block::default()
        .title(" Roadmap ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if state.items.is_empty() {
        let empty = Paragraph::new("No roadmap items. Run 'deciduous roadmap init' first.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(empty, inner_area);
        return;
    }

    // Group items by section
    let items_per_row = 2; // 2 lines per item
    let visible_items = (inner_area.height as usize) / items_per_row;

    let list_items: Vec<ListItem> = state
        .items
        .iter()
        .enumerate()
        .skip(state.scroll_offset)
        .take(visible_items)
        .map(|(idx, item)| {
            let is_selected = idx == state.selected_index;

            // Build status indicators
            let mut status_spans = vec![];

            // Checkbox state
            let checkbox = match item.checkbox_state.as_str() {
                "checked" => Span::styled("[x]", Style::default().fg(Color::Green)),
                "unchecked" => Span::styled("[ ]", Style::default().fg(Color::DarkGray)),
                _ => Span::styled("   ", Style::default()),
            };
            status_spans.push(checkbox);
            status_spans.push(Span::raw(" "));

            // GitHub issue
            if let Some(issue_num) = item.github_issue_number {
                let issue_style = match item.github_issue_state.as_deref() {
                    Some("open") => Style::default().fg(Color::Green),
                    Some("closed") => Style::default().fg(Color::Magenta),
                    _ => Style::default().fg(Color::DarkGray),
                };
                status_spans.push(Span::styled(format!("#{} ", issue_num), issue_style));
            }

            // Outcome link
            if item.outcome_node_id.is_some() {
                status_spans.push(Span::styled("", Style::default().fg(Color::Blue)));
                status_spans.push(Span::raw(" "));
            }

            // Title
            let title = truncate_str(&item.title, inner_area.width as usize - 20);
            status_spans.push(Span::styled(
                title,
                if is_selected {
                    Style::default().fg(Color::White).bold()
                } else {
                    Style::default().fg(Color::White)
                },
            ));

            // Section on second line
            let section = item.section.as_deref().unwrap_or("(no section)");
            let section_line = Line::from(Span::styled(
                format!("    {}", section),
                Style::default().fg(Color::DarkGray),
            ));

            let style = if is_selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            ListItem::new(vec![
                Line::from(status_spans),
                section_line,
            ])
            .style(style)
        })
        .collect();

    let list = List::new(list_items);
    frame.render_widget(list, inner_area);
}

/// Truncate string to max width
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roadmap_state_new() {
        let state = RoadmapState::new();
        assert!(state.items.is_empty());
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 8), "hello...");
    }
}
