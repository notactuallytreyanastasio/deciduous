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

/// Check if an item is complete for display purposes.
/// An item is complete if:
/// - Checkbox is checked, OR
/// - It's in a "Completed" section (case-insensitive)
pub fn is_item_complete(item: &RoadmapItem) -> bool {
    let checkbox_checked = item.checkbox_state == "checked";
    let in_completed_section = item.section.as_deref()
        .map(|s| s.to_lowercase().contains("completed"))
        .unwrap_or(false);
    checkbox_checked || in_completed_section
}

/// Check if an item is fully synced (all three criteria met).
/// This is the strict check for sync verification.
pub fn is_item_fully_synced(item: &RoadmapItem) -> bool {
    let checkbox_checked = item.checkbox_state == "checked";
    let has_outcome = item.outcome_change_id.is_some();
    let issue_closed = item.github_issue_state.as_deref() == Some("closed");
    checkbox_checked && has_outcome && issue_closed
}

/// Check if an item is partially complete (some but not all sync criteria met)
pub fn is_item_partial(item: &RoadmapItem) -> bool {
    // Item is complete for display but not fully synced
    if is_item_complete(item) && !is_item_fully_synced(item) {
        return true;
    }
    // Or has some sync criteria but checkbox not checked
    let has_outcome = item.outcome_change_id.is_some();
    let issue_closed = item.github_issue_state.as_deref() == Some("closed");
    (has_outcome || issue_closed) && item.checkbox_state != "checked"
}

/// Check if an item is a section header (not a real task)
/// Section headers have checkbox_state = "none"
pub fn is_section_header(item: &RoadmapItem) -> bool {
    item.checkbox_state == "none"
}

/// Filter items by view mode, excluding section headers
pub fn filter_by_mode(items: &[RoadmapItem], mode: RoadmapViewMode) -> Vec<RoadmapItem> {
    // First filter out section headers - they're not real tasks
    let tasks: Vec<_> = items.iter()
        .filter(|item| !is_section_header(item))
        .collect();

    match mode {
        RoadmapViewMode::Active => tasks
            .into_iter()
            .filter(|item| !is_item_complete(item))
            .cloned()
            .collect(),
        RoadmapViewMode::Completed => tasks
            .into_iter()
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
    // Only count actual tasks, not section headers
    let tasks: Vec<_> = items.iter()
        .filter(|i| !is_section_header(i))
        .collect();
    let complete = tasks.iter().filter(|i| is_item_complete(i)).count();
    let active = tasks.len() - complete;
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

    /// Jump to top (gg)
    pub fn jump_to_top(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    /// Jump to bottom (G)
    pub fn jump_to_bottom(&mut self) {
        if !self.visible_items.is_empty() {
            self.selected_index = self.visible_items.len() - 1;
            self.ensure_visible(20);
        }
    }

    /// Page down (Ctrl-D)
    pub fn page_down(&mut self, page_size: usize) {
        let new_index = (self.selected_index + page_size).min(
            self.visible_items.len().saturating_sub(1)
        );
        self.selected_index = new_index;
        self.ensure_visible(20);
    }

    /// Page up (Ctrl-U)
    pub fn page_up(&mut self, page_size: usize) {
        self.selected_index = self.selected_index.saturating_sub(page_size);
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

/// Draw the roadmap list with section grouping
fn draw_list(frame: &mut Frame, state: &RoadmapState, area: Rect) {
    let (active_count, complete_count) = state.get_counts();

    // Draw tab bar at top
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    // Tab bar
    draw_tab_bar(frame, state, chunks[0], active_count, complete_count);

    // Main list area
    let list_area = chunks[1];

    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner_area = block.inner(list_area);
    frame.render_widget(block, list_area);

    if state.visible_items.is_empty() {
        let msg = match state.view_mode {
            RoadmapViewMode::Active => "No active items. Press Shift+Tab to view completed.",
            RoadmapViewMode::Completed => "No completed items. Press Shift+Tab to view active.",
        };
        let empty = Paragraph::new(msg)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(empty, inner_area);
    } else {
        // Build list with section headers
        let mut list_items: Vec<ListItem> = vec![];
        let mut current_section: Option<&str> = None;
        let mut display_idx = 0;

        // Calculate how many items we can show (2 lines per item + 1 line per section header)
        let max_lines = inner_area.height as usize;
        let mut lines_used = 0;

        for (idx, item) in state.visible_items.iter().enumerate() {
            if idx < state.scroll_offset {
                // Track section changes even for skipped items
                if item.section.as_deref() != current_section {
                    current_section = item.section.as_deref();
                }
                continue;
            }

            // Check if we need a section header
            if item.section.as_deref() != current_section {
                current_section = item.section.as_deref();

                // Add section header (1 line)
                if lines_used < max_lines {
                    let section_name = current_section.unwrap_or("(no section)");
                    list_items.push(render_section_header(section_name, inner_area.width));
                    lines_used += 1;
                }
            }

            // Add item (2 lines)
            if lines_used + 2 <= max_lines {
                let is_selected = idx == state.selected_index;
                list_items.push(render_item_grouped(item, display_idx, is_selected, inner_area.width));
                display_idx += 1;
                lines_used += 2;
            } else {
                break;
            }
        }

        let list = List::new(list_items);
        frame.render_widget(list, inner_area);
    }

    // Help bar at bottom
    draw_help_bar(frame, chunks[2]);
}

/// Draw the tab bar showing Active/Completed tabs
fn draw_tab_bar(frame: &mut Frame, state: &RoadmapState, area: Rect, active_count: usize, complete_count: usize) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Roadmap ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Create tab labels
    let active_style = if state.view_mode == RoadmapViewMode::Active {
        Style::default().fg(Color::Black).bg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let completed_style = if state.view_mode == RoadmapViewMode::Completed {
        Style::default().fg(Color::Black).bg(Color::Green).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let tabs = Line::from(vec![
        Span::raw(" "),
        Span::styled(format!(" Active ({}) ", active_count), active_style),
        Span::raw("  "),
        Span::styled(format!(" Completed ({}) ", complete_count), completed_style),
        Span::raw("  "),
        Span::styled("Shift+Tab to switch", Style::default().fg(Color::DarkGray).italic()),
    ]);

    let tabs_widget = Paragraph::new(tabs);
    frame.render_widget(tabs_widget, inner);
}

/// Draw the help bar at the bottom
fn draw_help_bar(frame: &mut Frame, area: Rect) {
    let help = Line::from(vec![
        Span::styled(" j/k", Style::default().fg(Color::Cyan)),
        Span::raw(":nav "),
        Span::styled("G/gg", Style::default().fg(Color::Cyan)),
        Span::raw(":jump "),
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::raw(":detail "),
        Span::styled("o", Style::default().fg(Color::Cyan)),
        Span::raw(":open issue "),
        Span::styled("c", Style::default().fg(Color::Cyan)),
        Span::raw(":toggle check "),
        Span::styled("r", Style::default().fg(Color::Cyan)),
        Span::raw(":refresh "),
        Span::styled("?", Style::default().fg(Color::Cyan)),
        Span::raw(":help"),
    ]);

    let help_widget = Paragraph::new(help)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(help_widget, area);
}

/// Render a section header
fn render_section_header(section: &str, _width: u16) -> ListItem<'static> {
    let line = Line::from(vec![
        Span::styled("── ", Style::default().fg(Color::DarkGray)),
        Span::styled(section.to_string(), Style::default().fg(Color::Cyan).bold()),
        Span::styled(" ──", Style::default().fg(Color::DarkGray)),
    ]);
    ListItem::new(vec![line])
}

/// Render a roadmap item (grouped under section, no section name shown)
fn render_item_grouped(item: &RoadmapItem, index: usize, is_selected: bool, width: u16) -> ListItem<'static> {
    // Line 1: indent, number, checkbox, title
    let mut line1_spans = vec![];

    // Indent (2 spaces)
    line1_spans.push(Span::raw("  "));

    // Row number
    let num_style = if is_selected {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };
    line1_spans.push(Span::styled(format!("{:>2} ", index + 1), num_style));

    // Checkbox
    let checkbox = match item.checkbox_state.as_str() {
        "checked" => Span::styled("[x]", Style::default().fg(Color::Green).bold()),
        "unchecked" => Span::styled("[ ]", Style::default().fg(Color::DarkGray)),
        _ => Span::styled("   ", Style::default()),
    };
    line1_spans.push(checkbox);
    line1_spans.push(Span::raw(" "));

    // Title
    let max_title_len = (width as usize).saturating_sub(12);
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
    line1_spans.push(Span::styled(title, title_style));

    // Line 2: status indicators (indented)
    let mut line2_spans = vec![];
    line2_spans.push(Span::raw("       ")); // Indent to align under title

    // Issue link
    if let Some(issue_num) = item.github_issue_number {
        let issue_style = match item.github_issue_state.as_deref() {
            Some("open") => Style::default().fg(Color::Green),
            Some("closed") => Style::default().fg(Color::Magenta),
            _ => Style::default().fg(Color::DarkGray),
        };
        let state_char = if item.github_issue_state.as_deref() == Some("closed") { "+" } else { "o" };
        line2_spans.push(Span::styled(format!("#{}[{}]", issue_num, state_char), issue_style));
    }

    // Outcome indicator
    if item.outcome_change_id.is_some() {
        line2_spans.push(Span::styled(" [outcome]", Style::default().fg(Color::Yellow)));
    }

    let style = if is_selected {
        Style::default().bg(Color::Rgb(40, 40, 50))
    } else {
        Style::default()
    };

    ListItem::new(vec![
        Line::from(line1_spans),
        Line::from(line2_spans),
    ]).style(style)
}

/// Render a single roadmap item as a ListItem (legacy, kept for reference)
#[allow(dead_code)]
fn render_item(item: &RoadmapItem, index: usize, is_selected: bool, width: u16) -> ListItem<'static> {
    // Line 1: Number, checkbox, title
    let mut line1_spans = vec![];

    // Row number (right-aligned, 3 chars)
    let num_style = if is_selected {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };
    line1_spans.push(Span::styled(format!("{:>3} ", index + 1), num_style));

    // Checkbox state - simple ASCII checkbox
    let checkbox = match item.checkbox_state.as_str() {
        "checked" => Span::styled("[x]", Style::default().fg(Color::Green).bold()),
        "unchecked" => Span::styled("[ ]", Style::default().fg(Color::DarkGray)),
        _ => Span::styled("   ", Style::default()),
    };
    line1_spans.push(checkbox);
    line1_spans.push(Span::raw(" "));

    // Title
    let max_title_len = (width as usize).saturating_sub(15);
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
    line1_spans.push(Span::styled(title, title_style));

    // Line 2: Section and status indicators
    let mut line2_spans = vec![];
    line2_spans.push(Span::raw("      ")); // Indent to align with title

    // Section
    let section = item.section.as_deref().unwrap_or("(no section)");
    line2_spans.push(Span::styled(
        truncate_str(section, 25),
        Style::default().fg(Color::DarkGray),
    ));

    // Status indicators (compact, no emojis)
    let mut status_parts = vec![];

    // Issue link
    if let Some(issue_num) = item.github_issue_number {
        let issue_style = match item.github_issue_state.as_deref() {
            Some("open") => Style::default().fg(Color::Green),
            Some("closed") => Style::default().fg(Color::Magenta),
            _ => Style::default().fg(Color::DarkGray),
        };
        let state_char = if item.github_issue_state.as_deref() == Some("closed") { "+" } else { "o" };
        status_parts.push(Span::styled(format!(" #{}[{}]", issue_num, state_char), issue_style));
    }

    // Outcome indicator
    if item.outcome_change_id.is_some() {
        status_parts.push(Span::styled(" [outcome]", Style::default().fg(Color::Yellow)));
    }

    line2_spans.extend(status_parts);

    // Line 3: Empty line for spacing
    let line3 = Line::from(Span::raw(""));

    let style = if is_selected {
        Style::default().bg(Color::Rgb(40, 40, 50))
    } else {
        Style::default()
    };

    ListItem::new(vec![
        Line::from(line1_spans),
        Line::from(line2_spans),
        line3,
    ]).style(style)
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
        "--- Completion Status ---",
        Style::default().fg(Color::DarkGray),
    )));

    // Checkbox status
    let checkbox_icon = if item.checkbox_state == "checked" { "[x]" } else { "[ ]" };
    let checkbox_status = if item.checkbox_state == "checked" { "Checked" } else { "Unchecked" };
    let checkbox_color = if item.checkbox_state == "checked" { Color::Green } else { Color::Red };
    lines.push(Line::from(vec![
        Span::styled(format!("{} Checkbox: ", checkbox_icon), Style::default().fg(checkbox_color)),
        Span::styled(checkbox_status, Style::default().fg(checkbox_color)),
    ]));

    // Outcome status
    let outcome_icon = if item.outcome_change_id.is_some() { "[+]" } else { "[-]" };
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
    let issue_icon = if item.github_issue_state.as_deref() == Some("closed") { "[+]" } else { "[-]" };
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
        "[COMPLETE]"
    } else {
        "[INCOMPLETE]"
    };
    let status_color = if is_complete { Color::Green } else { Color::Yellow };
    lines.push(Line::from(Span::styled(
        status_text,
        Style::default().fg(status_color).bold(),
    )));

    // Keyboard hints
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "--- Actions ---",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(vec![
        Span::styled("o", Style::default().fg(Color::Cyan)),
        Span::raw(" - Open GitHub issue"),
    ]));
    lines.push(Line::from(vec![
        Span::styled("c", Style::default().fg(Color::Cyan)),
        Span::raw(" - Toggle checkbox"),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::raw(" - Close detail panel"),
    ]));

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
        make_item_with_section(id, title, checkbox, outcome, issue_state, "In Progress")
    }

    fn make_item_with_section(id: i32, title: &str, checkbox: &str, outcome: Option<&str>, issue_state: Option<&str>, section: &str) -> RoadmapItem {
        RoadmapItem {
            id,
            change_id: format!("change-{}", id),
            title: title.to_string(),
            description: None,
            section: Some(section.to_string()),
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
        // Checkbox checked -> complete
        let checked = make_item(1, "Checked", "checked", None, None);
        assert!(is_item_complete(&checked));

        // In "Completed" section -> complete (even with unchecked checkbox)
        let in_completed = make_item_with_section(2, "In Completed", "unchecked", None, None, "Completed");
        assert!(is_item_complete(&in_completed));

        // Case insensitive section check
        let in_completed_case = make_item_with_section(3, "Case", "unchecked", None, None, "COMPLETED ITEMS");
        assert!(is_item_complete(&in_completed_case));

        // Unchecked and not in completed section -> not complete
        let unchecked = make_item(4, "Unchecked", "unchecked", None, None);
        assert!(!is_item_complete(&unchecked));

        // Both checked AND in completed section -> still complete
        let both = make_item_with_section(5, "Both", "checked", Some("o"), Some("closed"), "Completed");
        assert!(is_item_complete(&both));
    }

    #[test]
    fn test_is_item_fully_synced() {
        // All three criteria met -> fully synced
        let synced = make_item(1, "Synced", "checked", Some("outcome-1"), Some("closed"));
        assert!(is_item_fully_synced(&synced));

        // Missing checkbox -> not synced
        let no_checkbox = make_item(2, "No Checkbox", "unchecked", Some("outcome-2"), Some("closed"));
        assert!(!is_item_fully_synced(&no_checkbox));

        // Missing outcome -> not synced
        let no_outcome = make_item(3, "No Outcome", "checked", None, Some("closed"));
        assert!(!is_item_fully_synced(&no_outcome));

        // Issue not closed -> not synced
        let open_issue = make_item(4, "Open Issue", "checked", Some("outcome-4"), Some("open"));
        assert!(!is_item_fully_synced(&open_issue));

        // Just checked -> not synced (but is complete for display)
        let just_checked = make_item(5, "Just Checked", "checked", None, None);
        assert!(!is_item_fully_synced(&just_checked));
        assert!(is_item_complete(&just_checked)); // but IS complete
    }

    #[test]
    fn test_is_item_partial() {
        // Fully synced item is not partial
        let synced = make_item(1, "Synced", "checked", Some("outcome-1"), Some("closed"));
        assert!(!is_item_partial(&synced));

        // Checked but not fully synced -> partial
        let checked_only = make_item(2, "Checked Only", "checked", None, None);
        assert!(is_item_partial(&checked_only));

        // Has outcome but not checked -> partial
        let has_outcome = make_item(3, "Has Outcome", "unchecked", Some("o-3"), None);
        assert!(is_item_partial(&has_outcome));

        // Unchecked with no sync criteria -> not partial
        let none_met = make_item(4, "None Met", "unchecked", None, None);
        assert!(!is_item_partial(&none_met));

        // In completed section but not synced -> partial
        let in_section = make_item_with_section(5, "In Section", "unchecked", None, None, "Completed");
        assert!(is_item_partial(&in_section));
    }

    #[test]
    fn test_is_section_header() {
        // Section header has checkbox_state = "none"
        let section = make_item(1, "In Progress", "none", None, None);
        assert!(is_section_header(&section));

        // Real tasks have checked/unchecked
        let checked = make_item(2, "Task", "checked", None, None);
        assert!(!is_section_header(&checked));

        let unchecked = make_item(3, "Task", "unchecked", None, None);
        assert!(!is_section_header(&unchecked));
    }

    #[test]
    fn test_filter_by_mode() {
        let items = vec![
            make_item(1, "Section Header", "none", None, None),                       // section header - excluded
            make_item(2, "Checked", "checked", None, None),                           // complete (checked)
            make_item(3, "Unchecked", "unchecked", None, None),                        // active
            make_item_with_section(4, "In Completed", "unchecked", None, None, "Completed"), // complete (section)
        ];

        let active = filter_by_mode(&items, RoadmapViewMode::Active);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].title, "Unchecked");

        let completed = filter_by_mode(&items, RoadmapViewMode::Completed);
        assert_eq!(completed.len(), 2);
        assert!(completed.iter().any(|i| i.title == "Checked"));
        assert!(completed.iter().any(|i| i.title == "In Completed"));
    }

    #[test]
    fn test_count_by_status() {
        let items = vec![
            make_item(1, "Section Header", "none", None, None),                       // section header - excluded
            make_item(2, "Checked", "checked", None, None),                           // complete
            make_item(3, "Unchecked 1", "unchecked", None, None),                      // active
            make_item(4, "Unchecked 2", "unchecked", None, None),                      // active
            make_item_with_section(5, "In Completed", "unchecked", None, None, "Completed"), // complete
        ];

        let (active, complete) = count_by_status(&items);
        assert_eq!(active, 2);   // Unchecked 1, Unchecked 2
        assert_eq!(complete, 2); // Checked, In Completed (section header excluded)
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
            make_item(1, "Checked", "checked", None, None),      // complete (checked)
            make_item(2, "Active", "unchecked", None, None),     // active
        ]);

        // Default is Active mode - only unchecked items
        assert_eq!(state.view_mode, RoadmapViewMode::Active);
        assert_eq!(state.visible_items.len(), 1);
        assert_eq!(state.visible_items[0].title, "Active");

        // Toggle to Completed - only checked items
        state.toggle_mode();
        assert_eq!(state.view_mode, RoadmapViewMode::Completed);
        assert_eq!(state.visible_items.len(), 1);
        assert_eq!(state.visible_items[0].title, "Checked");

        // Toggle back to Active
        state.toggle_mode();
        assert_eq!(state.view_mode, RoadmapViewMode::Active);
        assert_eq!(state.visible_items.len(), 1);
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
