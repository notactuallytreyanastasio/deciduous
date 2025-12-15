//! Roadmap view - displays roadmap items with sync status
//!
//! Follows TEA (The Elm Architecture):
//! - Model: RoadmapState (data)
//! - Update: state mutation methods
//! - View: draw() function
//!
//! Pure functions are separated for testability.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::db::RoadmapItem;

// =============================================================================
// Model - State
// =============================================================================

/// View mode for roadmap items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoadmapViewMode {
    #[default]
    Active,    // Show incomplete items
    Completed, // Show completed items
}

/// State for the roadmap view
#[derive(Debug, Clone, Default)]
pub struct RoadmapState {
    /// All roadmap items from database
    all_items: Vec<RoadmapItem>,
    /// Currently visible items (filtered by mode)
    visible_items: Vec<RoadmapItem>,
    /// Current view mode
    pub view_mode: RoadmapViewMode,
    /// Selected index in visible items
    pub selected_index: usize,
    /// Scroll offset for viewport
    pub scroll_offset: usize,
    /// Whether detail panel is shown
    pub show_detail: bool,
}

// =============================================================================
// Pure Functions - Functional Core
// =============================================================================

/// Check if an item is complete (checkbox + outcome + issue closed)
pub fn is_item_complete(item: &RoadmapItem) -> bool {
    let checkbox_checked = item.checkbox_state == "checked";
    let has_outcome = item.outcome_change_id.is_some();
    let issue_closed = item.github_issue_state.as_deref() == Some("closed");
    checkbox_checked && has_outcome && issue_closed
}

/// Check if an item is partially complete (any completion criteria met)
pub fn is_item_partial(item: &RoadmapItem) -> bool {
    let checkbox_checked = item.checkbox_state == "checked";
    let has_outcome = item.outcome_change_id.is_some();
    let issue_closed = item.github_issue_state.as_deref() == Some("closed");
    (checkbox_checked || has_outcome || issue_closed) && !is_item_complete(item)
}

/// Filter items by view mode
pub fn filter_by_mode(items: &[RoadmapItem], mode: RoadmapViewMode) -> Vec<RoadmapItem> {
    match mode {
        RoadmapViewMode::Active => items
            .iter()
            .filter(|item| !is_item_complete(item))
            .cloned()
            .collect(),
        RoadmapViewMode::Completed => items
            .iter()
            .filter(|item| is_item_complete(item))
            .cloned()
            .collect(),
    }
}

/// Group items by section
pub fn group_by_section(items: &[RoadmapItem]) -> Vec<(String, Vec<&RoadmapItem>)> {
    use std::collections::BTreeMap;

    let mut groups: BTreeMap<String, Vec<&RoadmapItem>> = BTreeMap::new();

    for item in items {
        let section = item.section.clone().unwrap_or_else(|| "(No Section)".to_string());
        groups.entry(section).or_default().push(item);
    }

    groups.into_iter().collect()
}

/// Calculate new index after moving up
pub fn move_up(current: usize) -> usize {
    current.saturating_sub(1)
}

/// Calculate new index after moving down
pub fn move_down(current: usize, max: usize) -> usize {
    if max == 0 {
        0
    } else {
        (current + 1).min(max - 1)
    }
}

/// Calculate scroll offset to keep selection visible
pub fn calculate_scroll(selected: usize, current_offset: usize, visible_items: usize) -> usize {
    if visible_items == 0 {
        return 0;
    }
    if selected < current_offset {
        selected
    } else if selected >= current_offset + visible_items {
        selected.saturating_sub(visible_items - 1)
    } else {
        current_offset
    }
}

/// Clamp selection to valid range
pub fn clamp_selection(selected: usize, max: usize) -> usize {
    if max == 0 {
        0
    } else {
        selected.min(max - 1)
    }
}

/// Truncate string to max width with ellipsis
pub fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

/// Count items by completion status
pub fn count_by_status(items: &[RoadmapItem]) -> (usize, usize) {
    let complete = items.iter().filter(|i| is_item_complete(i)).count();
    let active = items.len() - complete;
    (active, complete)
}

// =============================================================================
// Update - State Mutations (Methods)
// =============================================================================

impl RoadmapState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set all items and refresh visible items
    pub fn set_items(&mut self, items: Vec<RoadmapItem>) {
        self.all_items = items;
        self.refresh_visible();
    }

    /// Refresh visible items based on current mode
    fn refresh_visible(&mut self) {
        self.visible_items = filter_by_mode(&self.all_items, self.view_mode);
        self.selected_index = clamp_selection(self.selected_index, self.visible_items.len());
    }

    /// Toggle between active and completed view
    pub fn toggle_mode(&mut self) {
        self.view_mode = match self.view_mode {
            RoadmapViewMode::Active => RoadmapViewMode::Completed,
            RoadmapViewMode::Completed => RoadmapViewMode::Active,
        };
        self.refresh_visible();
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        self.selected_index = move_up(self.selected_index);
        self.ensure_visible(20);
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        self.selected_index = move_down(self.selected_index, self.visible_items.len());
        self.ensure_visible(20);
    }

    /// Toggle detail panel
    pub fn toggle_detail(&mut self) {
        self.show_detail = !self.show_detail;
    }

    /// Get currently selected item
    pub fn selected_item(&self) -> Option<&RoadmapItem> {
        self.visible_items.get(self.selected_index)
    }

    /// Get visible items (for rendering)
    pub fn visible_items(&self) -> &[RoadmapItem] {
        &self.visible_items
    }

    /// Get counts for status bar
    pub fn get_counts(&self) -> (usize, usize) {
        count_by_status(&self.all_items)
    }

    /// Get GitHub issue URL for selected item (if it has an issue)
    pub fn selected_issue_url(&self) -> Option<String> {
        self.selected_item().and_then(|item| {
            item.github_issue_number.map(|num| {
                format!(
                    "https://github.com/notactuallytreyanastasio/deciduous/issues/{}",
                    num
                )
            })
        })
    }

    /// Get selected item ID and current checkbox state (for toggling)
    pub fn selected_item_checkbox_info(&self) -> Option<(i32, String)> {
        self.selected_item().map(|item| {
            (item.id, item.checkbox_state.clone())
        })
    }

    /// Ensure selection is visible in viewport
    fn ensure_visible(&mut self, visible_items: usize) {
        self.scroll_offset = calculate_scroll(
            self.selected_index,
            self.scroll_offset,
            visible_items,
        );
    }
}

// =============================================================================
// View - Rendering
// =============================================================================

/// Draw the roadmap view (main list)
pub fn draw(frame: &mut Frame, state: &RoadmapState, area: Rect) {
    if state.show_detail {
        // Split area: list on left, detail on right
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        draw_list(frame, state, chunks[0]);
        draw_detail(frame, state, chunks[1]);
    } else {
        draw_list(frame, state, area);
    }
}

/// Draw the roadmap list
fn draw_list(frame: &mut Frame, state: &RoadmapState, area: Rect) {
    let (active_count, complete_count) = state.get_counts();

    let title = match state.view_mode {
        RoadmapViewMode::Active => format!(" Roadmap - Active ({}) ", active_count),
        RoadmapViewMode::Completed => format!(" Roadmap - Completed ({}) ", complete_count),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if state.visible_items.is_empty() {
        let msg = match state.view_mode {
            RoadmapViewMode::Active => "No active items. Press Shift+Tab to view completed.",
            RoadmapViewMode::Completed => "No completed items. Press Shift+Tab to view active.",
        };
        let empty = Paragraph::new(msg)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(empty, inner_area);
        return;
    }

    // Calculate visible items
    let item_height = 2; // 2 lines per item
    let visible_count = (inner_area.height as usize) / item_height;

    let list_items: Vec<ListItem> = state
        .visible_items
        .iter()
        .enumerate()
        .skip(state.scroll_offset)
        .take(visible_count)
        .map(|(idx, item)| render_item(item, idx == state.selected_index, inner_area.width))
        .collect();

    let list = List::new(list_items);
    frame.render_widget(list, inner_area);

    // Help hint at bottom
    let help = " j/k: nav | Enter: detail | Shift+Tab: toggle view | r: refresh ";
    if area.height > 3 {
        let help_area = Rect::new(area.x + 1, area.y + area.height - 1, area.width - 2, 1);
        let help_text = Paragraph::new(help)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(help_text, help_area);
    }
}

/// Render a single roadmap item as a ListItem
fn render_item(item: &RoadmapItem, is_selected: bool, width: u16) -> ListItem<'static> {
    let mut spans = vec![];

    // Checkbox state
    let checkbox = match item.checkbox_state.as_str() {
        "checked" => Span::styled("☑ ", Style::default().fg(Color::Green)),
        "unchecked" => Span::styled("☐ ", Style::default().fg(Color::DarkGray)),
        _ => Span::styled("  ", Style::default()),
    };
    spans.push(checkbox);

    // Outcome indicator
    if item.outcome_change_id.is_some() {
        spans.push(Span::styled("⚡", Style::default().fg(Color::Yellow)));
    } else {
        spans.push(Span::styled("⚡", Style::default().fg(Color::DarkGray)));
    }
    spans.push(Span::raw(" "));

    // Issue status
    if let Some(issue_num) = item.github_issue_number {
        let issue_style = match item.github_issue_state.as_deref() {
            Some("open") => Style::default().fg(Color::Green),
            Some("closed") => Style::default().fg(Color::Magenta),
            _ => Style::default().fg(Color::DarkGray),
        };
        let icon = if item.github_issue_state.as_deref() == Some("closed") {
            "🔒"
        } else {
            "🔓"
        };
        spans.push(Span::styled(format!("{} #{} ", icon, issue_num), issue_style));
    } else {
        spans.push(Span::styled("   ", Style::default().fg(Color::DarkGray)));
    }

    // Title
    let max_title_len = (width as usize).saturating_sub(20);
    let title = truncate_str(&item.title, max_title_len);
    let title_style = if is_selected {
        Style::default().fg(Color::White).bold()
    } else if is_item_complete(item) {
        Style::default().fg(Color::Green)
    } else if is_item_partial(item) {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };
    spans.push(Span::styled(title, title_style));

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

    ListItem::new(vec![Line::from(spans), section_line]).style(style)
}

/// Draw the detail panel for selected item
fn draw_detail(frame: &mut Frame, state: &RoadmapState, area: Rect) {
    let block = Block::default()
        .title(" Item Detail ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let Some(item) = state.selected_item() else {
        let empty = Paragraph::new("No item selected")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, inner_area);
        return;
    };

    // Build detail content
    let mut lines = vec![];

    // Title
    lines.push(Line::from(Span::styled(
        &item.title,
        Style::default().fg(Color::Cyan).bold(),
    )));
    lines.push(Line::from(""));

    // Section
    if let Some(ref section) = item.section {
        lines.push(Line::from(vec![
            Span::styled("Section: ", Style::default().fg(Color::DarkGray)),
            Span::styled(section.as_str(), Style::default().fg(Color::White)),
        ]));
    }

    // Description
    if let Some(ref desc) = item.description {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Description:",
            Style::default().fg(Color::DarkGray),
        )));
        // Word wrap description
        for line in desc.lines() {
            lines.push(Line::from(Span::styled(
                line,
                Style::default().fg(Color::White),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "─ Completion Status ─",
        Style::default().fg(Color::DarkGray),
    )));

    // Checkbox status
    let checkbox_icon = if item.checkbox_state == "checked" { "☑" } else { "☐" };
    let checkbox_status = if item.checkbox_state == "checked" { "Checked" } else { "Unchecked" };
    let checkbox_color = if item.checkbox_state == "checked" { Color::Green } else { Color::Red };
    lines.push(Line::from(vec![
        Span::styled(format!("{} Checkbox: ", checkbox_icon), Style::default().fg(checkbox_color)),
        Span::styled(checkbox_status, Style::default().fg(checkbox_color)),
    ]));

    // Outcome status
    let outcome_icon = if item.outcome_change_id.is_some() { "⚡" } else { "○" };
    let outcome_color = if item.outcome_change_id.is_some() { Color::Green } else { Color::Red };
    let outcome_text = match &item.outcome_change_id {
        Some(id) => format!("Linked ({})", &id[..8.min(id.len())]),
        None => "Not linked".to_string(),
    };
    lines.push(Line::from(vec![
        Span::styled(format!("{} Outcome: ", outcome_icon), Style::default().fg(outcome_color)),
        Span::styled(outcome_text, Style::default().fg(outcome_color)),
    ]));

    // GitHub issue status
    let issue_icon = if item.github_issue_state.as_deref() == Some("closed") { "🔒" } else { "🔓" };
    let issue_color = if item.github_issue_state.as_deref() == Some("closed") { Color::Green } else { Color::Red };
    let issue_text = match (item.github_issue_number, &item.github_issue_state) {
        (Some(num), Some(state)) => format!("#{} ({})", num, state),
        (Some(num), None) => format!("#{}", num),
        _ => "No issue".to_string(),
    };
    lines.push(Line::from(vec![
        Span::styled(format!("{} Issue: ", issue_icon), Style::default().fg(issue_color)),
        Span::styled(issue_text, Style::default().fg(issue_color)),
    ]));

    // Overall status
    lines.push(Line::from(""));
    let is_complete = is_item_complete(item);
    let status_text = if is_complete {
        "✓ COMPLETE"
    } else {
        "○ INCOMPLETE"
    };
    let status_color = if is_complete { Color::Green } else { Color::Yellow };
    lines.push(Line::from(Span::styled(
        status_text,
        Style::default().fg(status_color).bold(),
    )));

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(para, inner_area);
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(id: i32, title: &str, checkbox: &str, outcome: Option<&str>, issue_state: Option<&str>) -> RoadmapItem {
        RoadmapItem {
            id,
            change_id: format!("change-{}", id),
            title: title.to_string(),
            description: None,
            section: Some("Test Section".to_string()),
            parent_id: None,
            checkbox_state: checkbox.to_string(),
            github_issue_number: if issue_state.is_some() { Some(id) } else { None },
            github_issue_state: issue_state.map(|s| s.to_string()),
            outcome_node_id: if outcome.is_some() { Some(id) } else { None },
            outcome_change_id: outcome.map(|s| s.to_string()),
            markdown_line_start: None,
            markdown_line_end: None,
            content_hash: None,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
            last_synced_at: None,
        }
    }

    #[test]
    fn test_is_item_complete() {
        // Complete item: all three criteria met
        let complete = make_item(1, "Complete", "checked", Some("outcome-1"), Some("closed"));
        assert!(is_item_complete(&complete));

        // Missing checkbox
        let no_checkbox = make_item(2, "No Checkbox", "unchecked", Some("outcome-2"), Some("closed"));
        assert!(!is_item_complete(&no_checkbox));

        // Missing outcome
        let no_outcome = make_item(3, "No Outcome", "checked", None, Some("closed"));
        assert!(!is_item_complete(&no_outcome));

        // Issue not closed
        let open_issue = make_item(4, "Open Issue", "checked", Some("outcome-4"), Some("open"));
        assert!(!is_item_complete(&open_issue));
    }

    #[test]
    fn test_is_item_partial() {
        // Complete item is not partial
        let complete = make_item(1, "Complete", "checked", Some("outcome-1"), Some("closed"));
        assert!(!is_item_partial(&complete));

        // One criterion met
        let one_met = make_item(2, "One Met", "checked", None, None);
        assert!(is_item_partial(&one_met));

        // Two criteria met
        let two_met = make_item(3, "Two Met", "checked", Some("outcome-3"), Some("open"));
        assert!(is_item_partial(&two_met));

        // None met
        let none_met = make_item(4, "None Met", "unchecked", None, None);
        assert!(!is_item_partial(&none_met));
    }

    #[test]
    fn test_filter_by_mode() {
        let items = vec![
            make_item(1, "Complete", "checked", Some("o-1"), Some("closed")),
            make_item(2, "Active 1", "unchecked", None, None),
            make_item(3, "Active 2", "checked", None, Some("open")),
        ];

        let active = filter_by_mode(&items, RoadmapViewMode::Active);
        assert_eq!(active.len(), 2);
        assert!(active.iter().all(|i| !is_item_complete(i)));

        let completed = filter_by_mode(&items, RoadmapViewMode::Completed);
        assert_eq!(completed.len(), 1);
        assert!(completed.iter().all(is_item_complete));
    }

    #[test]
    fn test_count_by_status() {
        let items = vec![
            make_item(1, "Complete", "checked", Some("o-1"), Some("closed")),
            make_item(2, "Active", "unchecked", None, None),
            make_item(3, "Active", "checked", None, Some("open")),
        ];

        let (active, complete) = count_by_status(&items);
        assert_eq!(active, 2);
        assert_eq!(complete, 1);
    }

    #[test]
    fn test_move_up() {
        assert_eq!(move_up(5), 4);
        assert_eq!(move_up(0), 0);
    }

    #[test]
    fn test_move_down() {
        assert_eq!(move_down(5, 10), 6);
        assert_eq!(move_down(9, 10), 9);
        assert_eq!(move_down(0, 0), 0);
    }

    #[test]
    fn test_clamp_selection() {
        assert_eq!(clamp_selection(5, 10), 5);
        assert_eq!(clamp_selection(15, 10), 9);
        assert_eq!(clamp_selection(5, 0), 0);
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 8), "hello...");
        assert_eq!(truncate_str("hi", 2), "hi");
    }

    #[test]
    fn test_roadmap_state_toggle_mode() {
        let mut state = RoadmapState::new();
        state.set_items(vec![
            make_item(1, "Complete", "checked", Some("o-1"), Some("closed")),
            make_item(2, "Active", "unchecked", None, None),
        ]);

        // Default is Active mode
        assert_eq!(state.view_mode, RoadmapViewMode::Active);
        assert_eq!(state.visible_items.len(), 1);

        // Toggle to Completed
        state.toggle_mode();
        assert_eq!(state.view_mode, RoadmapViewMode::Completed);
        assert_eq!(state.visible_items.len(), 1);

        // Toggle back to Active
        state.toggle_mode();
        assert_eq!(state.view_mode, RoadmapViewMode::Active);
    }

    #[test]
    fn test_selected_issue_url() {
        let mut state = RoadmapState::new();
        state.set_items(vec![
            make_item(1, "With Issue", "unchecked", None, Some("open")),
            make_item(2, "No Issue", "unchecked", None, None),
        ]);

        // First item has issue #1
        state.selected_index = 0;
        let url = state.selected_issue_url();
        assert!(url.is_some());
        assert!(url.unwrap().contains("/issues/1"));

        // Second item has no issue
        state.selected_index = 1;
        assert!(state.selected_issue_url().is_none());
    }
}
