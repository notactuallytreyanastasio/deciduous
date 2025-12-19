//! Trace view - displays API trace sessions and spans
//!
//! Follows TEA (The Elm Architecture):
//! - Model: TraceState (data)
//! - Update: state mutation methods
//! - View: draw() function
//!
//! Shows trace sessions from `deciduous proxy` commands with token usage,
//! expandable spans, and linking to decision nodes.

use ratatui::{
    prelude::*,
    widgets::{
        Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Wrap,
    },
};

use crate::db::{DecisionNode, TraceContent, TraceSession, TraceSpan};
use std::collections::HashMap;

// =============================================================================
// Model - State
// =============================================================================

/// View mode for trace display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TraceViewMode {
    #[default]
    Sessions, // List of trace sessions
    Spans,      // Spans within a session
    SpanDetail, // Full content for a span
}

/// Which content tab is active in span detail
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailTab {
    #[default]
    Thinking,
    Response,
    Tools,
    Nodes,
}

/// State for the trace view
#[derive(Debug, Clone, Default)]
pub struct TraceState {
    /// All trace sessions from database
    pub sessions: Vec<TraceSession>,
    /// Currently selected session index
    pub selected_session_idx: usize,
    /// Session scroll offset
    pub session_scroll: usize,
    /// Expanded session ID (shows spans)
    pub expanded_session: Option<String>,
    /// Spans for the expanded session
    pub spans: Vec<TraceSpan>,
    /// Selected span index
    pub selected_span_idx: usize,
    /// Span scroll offset
    pub span_scroll: usize,
    /// Current view mode
    pub view_mode: TraceViewMode,
    /// Detail content for selected span
    pub detail_content: Vec<TraceContent>,
    /// Detail scroll offset
    pub detail_scroll: usize,
    /// Which detail tab is active
    pub detail_tab: DetailTab,
    /// Show detail panel (for spans view)
    pub show_detail: bool,
    /// Node counts per span (span_id -> count)
    pub node_counts: HashMap<i32, i64>,
    /// Nodes for the detail view (Nodes tab)
    pub detail_nodes: Vec<DecisionNode>,
}

// =============================================================================
// Pure Functions - Functional Core
// =============================================================================

/// Format duration in human-readable form
pub fn format_duration(started: &str, ended: Option<&str>) -> String {
    if let Some(end) = ended {
        // Parse ISO timestamps and calculate duration
        if let (Ok(start), Ok(finish)) = (
            chrono::DateTime::parse_from_rfc3339(started),
            chrono::DateTime::parse_from_rfc3339(end),
        ) {
            let duration = finish.signed_duration_since(start);
            let secs = duration.num_seconds();
            if secs < 60 {
                return format!("{}s", secs);
            } else if secs < 3600 {
                return format!("{}m {}s", secs / 60, secs % 60);
            } else {
                return format!("{}h {}m", secs / 3600, (secs % 3600) / 60);
            }
        }
    }
    "...".to_string()
}

/// Format duration from milliseconds
pub fn format_duration_ms(ms: Option<i32>) -> String {
    match ms {
        Some(millis) if millis < 1000 => format!("{}ms", millis),
        Some(millis) => format!("{:.1}s", millis as f64 / 1000.0),
        None => "-".to_string(),
    }
}

/// Format token count with K suffix for large numbers
pub fn format_tokens(count: i32) -> String {
    if count >= 10000 {
        format!("{:.0}k", count as f64 / 1000.0)
    } else if count >= 1000 {
        format!("{:.1}k", count as f64 / 1000.0)
    } else {
        format!("{}", count)
    }
}

/// Format relative time (e.g., "2h ago")
pub fn format_relative_time(timestamp: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp) {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(dt.with_timezone(&chrono::Utc));
        let secs = duration.num_seconds();

        if secs < 60 {
            "now".to_string()
        } else if secs < 3600 {
            format!("{}m ago", secs / 60)
        } else if secs < 86400 {
            format!("{}h ago", secs / 3600)
        } else {
            format!("{}d ago", secs / 86400)
        }
    } else {
        timestamp.to_string()
    }
}

/// Truncate string with ellipsis
pub fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
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

/// Get model short name (opus, sonnet, haiku)
pub fn model_short_name(model: Option<&str>) -> &str {
    match model {
        Some(m) if m.contains("opus") => "opus",
        Some(m) if m.contains("sonnet") => "sonnet",
        Some(m) if m.contains("haiku") => "haiku",
        Some(_) => "model",
        None => "-",
    }
}

// =============================================================================
// Update - State Mutations (Methods)
// =============================================================================

impl TraceState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set sessions from database
    pub fn set_sessions(&mut self, sessions: Vec<TraceSession>) {
        self.sessions = sessions;
        self.selected_session_idx = 0;
        self.session_scroll = 0;
    }

    /// Set spans for expanded session
    pub fn set_spans(&mut self, spans: Vec<TraceSpan>) {
        self.spans = spans;
        self.selected_span_idx = 0;
        self.span_scroll = 0;
    }

    /// Set detail content for a span
    pub fn set_detail_content(&mut self, content: Vec<TraceContent>) {
        self.detail_content = content;
        self.detail_scroll = 0;
    }

    /// Set node counts for spans (for efficient display)
    pub fn set_node_counts(&mut self, counts: HashMap<i32, i64>) {
        self.node_counts = counts;
    }

    /// Set nodes for detail view (Nodes tab)
    pub fn set_detail_nodes(&mut self, nodes: Vec<DecisionNode>) {
        self.detail_nodes = nodes;
    }

    /// Clear all state (for refresh)
    pub fn clear(&mut self) {
        self.sessions.clear();
        self.spans.clear();
        self.detail_content.clear();
        self.expanded_session = None;
        self.view_mode = TraceViewMode::Sessions;
    }

    // --- Navigation ---

    /// Move selection up
    pub fn move_up(&mut self) {
        match self.view_mode {
            TraceViewMode::Sessions => {
                self.selected_session_idx = self.selected_session_idx.saturating_sub(1);
                self.ensure_session_visible(20);
            }
            TraceViewMode::Spans => {
                self.selected_span_idx = self.selected_span_idx.saturating_sub(1);
                self.ensure_span_visible(20);
            }
            TraceViewMode::SpanDetail => {
                self.detail_scroll = self.detail_scroll.saturating_sub(1);
            }
        }
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        match self.view_mode {
            TraceViewMode::Sessions => {
                if !self.sessions.is_empty() {
                    self.selected_session_idx =
                        (self.selected_session_idx + 1).min(self.sessions.len() - 1);
                    self.ensure_session_visible(20);
                }
            }
            TraceViewMode::Spans => {
                if !self.spans.is_empty() {
                    self.selected_span_idx = (self.selected_span_idx + 1).min(self.spans.len() - 1);
                    self.ensure_span_visible(20);
                }
            }
            TraceViewMode::SpanDetail => {
                self.detail_scroll += 1;
            }
        }
    }

    /// Jump to top
    pub fn jump_to_top(&mut self) {
        match self.view_mode {
            TraceViewMode::Sessions => {
                self.selected_session_idx = 0;
                self.session_scroll = 0;
            }
            TraceViewMode::Spans => {
                self.selected_span_idx = 0;
                self.span_scroll = 0;
            }
            TraceViewMode::SpanDetail => {
                self.detail_scroll = 0;
            }
        }
    }

    /// Jump to bottom
    pub fn jump_to_bottom(&mut self) {
        match self.view_mode {
            TraceViewMode::Sessions => {
                if !self.sessions.is_empty() {
                    self.selected_session_idx = self.sessions.len() - 1;
                    self.ensure_session_visible(20);
                }
            }
            TraceViewMode::Spans => {
                if !self.spans.is_empty() {
                    self.selected_span_idx = self.spans.len() - 1;
                    self.ensure_span_visible(20);
                }
            }
            TraceViewMode::SpanDetail => {
                // Scroll to end - will be clamped in render
                self.detail_scroll = usize::MAX / 2;
            }
        }
    }

    /// Page down
    pub fn page_down(&mut self, page_size: usize) {
        match self.view_mode {
            TraceViewMode::Sessions => {
                self.selected_session_idx = (self.selected_session_idx + page_size)
                    .min(self.sessions.len().saturating_sub(1));
                self.ensure_session_visible(20);
            }
            TraceViewMode::Spans => {
                self.selected_span_idx =
                    (self.selected_span_idx + page_size).min(self.spans.len().saturating_sub(1));
                self.ensure_span_visible(20);
            }
            TraceViewMode::SpanDetail => {
                self.detail_scroll += page_size;
            }
        }
    }

    /// Page up
    pub fn page_up(&mut self, page_size: usize) {
        match self.view_mode {
            TraceViewMode::Sessions => {
                self.selected_session_idx = self.selected_session_idx.saturating_sub(page_size);
                self.ensure_session_visible(20);
            }
            TraceViewMode::Spans => {
                self.selected_span_idx = self.selected_span_idx.saturating_sub(page_size);
                self.ensure_span_visible(20);
            }
            TraceViewMode::SpanDetail => {
                self.detail_scroll = self.detail_scroll.saturating_sub(page_size);
            }
        }
    }

    // --- Actions ---

    /// Expand selected session to show spans
    pub fn expand_session(&mut self) -> Option<String> {
        if let Some(session) = self.sessions.get(self.selected_session_idx) {
            self.expanded_session = Some(session.session_id.clone());
            self.view_mode = TraceViewMode::Spans;
            return Some(session.session_id.clone());
        }
        None
    }

    /// Go back to sessions view
    pub fn collapse_to_sessions(&mut self) {
        self.expanded_session = None;
        self.view_mode = TraceViewMode::Sessions;
        self.spans.clear();
    }

    /// Show detail for selected span
    pub fn show_span_detail(&mut self) -> Option<i32> {
        if let Some(span) = self.spans.get(self.selected_span_idx) {
            self.view_mode = TraceViewMode::SpanDetail;
            self.detail_tab = DetailTab::Thinking;
            self.detail_scroll = 0;
            return Some(span.id);
        }
        None
    }

    /// Go back from detail to spans view
    pub fn back_from_detail(&mut self) {
        self.view_mode = TraceViewMode::Spans;
        self.detail_content.clear();
    }

    /// Handle escape key
    pub fn handle_escape(&mut self) {
        match self.view_mode {
            TraceViewMode::SpanDetail => self.back_from_detail(),
            TraceViewMode::Spans => self.collapse_to_sessions(),
            TraceViewMode::Sessions => {} // Can't go back further
        }
    }

    /// Cycle detail tab
    pub fn next_detail_tab(&mut self) {
        self.detail_tab = match self.detail_tab {
            DetailTab::Thinking => DetailTab::Response,
            DetailTab::Response => DetailTab::Tools,
            DetailTab::Tools => DetailTab::Nodes,
            DetailTab::Nodes => DetailTab::Thinking,
        };
        self.detail_scroll = 0;
    }

    /// Previous detail tab
    pub fn prev_detail_tab(&mut self) {
        self.detail_tab = match self.detail_tab {
            DetailTab::Thinking => DetailTab::Nodes,
            DetailTab::Response => DetailTab::Thinking,
            DetailTab::Tools => DetailTab::Response,
            DetailTab::Nodes => DetailTab::Tools,
        };
        self.detail_scroll = 0;
    }

    /// Toggle detail panel in spans view
    pub fn toggle_detail(&mut self) {
        self.show_detail = !self.show_detail;
    }

    // --- Getters ---

    /// Get selected session
    pub fn selected_session(&self) -> Option<&TraceSession> {
        self.sessions.get(self.selected_session_idx)
    }

    /// Get selected span
    pub fn selected_span(&self) -> Option<&TraceSpan> {
        self.spans.get(self.selected_span_idx)
    }

    /// Get content for current detail tab
    pub fn current_tab_content(&self) -> String {
        // Nodes tab is handled separately in draw_span_detail
        let content_type = match self.detail_tab {
            DetailTab::Thinking => "thinking",
            DetailTab::Response => "response",
            DetailTab::Tools => "tool_input",
            DetailTab::Nodes => return String::new(), // Nodes rendered separately
        };

        self.detail_content
            .iter()
            .filter(|c| {
                c.content_type == content_type
                    || (self.detail_tab == DetailTab::Tools && c.content_type == "tool_output")
            })
            .map(|c| {
                if self.detail_tab == DetailTab::Tools {
                    if let Some(ref name) = c.tool_name {
                        format!("=== {} ({}) ===\n{}\n", name, c.content_type, c.content)
                    } else {
                        format!("=== {} ===\n{}\n", c.content_type, c.content)
                    }
                } else {
                    c.content.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    // --- Internal ---

    fn ensure_session_visible(&mut self, visible_items: usize) {
        self.session_scroll = calculate_scroll(
            self.selected_session_idx,
            self.session_scroll,
            visible_items,
        );
    }

    fn ensure_span_visible(&mut self, visible_items: usize) {
        self.span_scroll =
            calculate_scroll(self.selected_span_idx, self.span_scroll, visible_items);
    }
}

// =============================================================================
// View - Rendering
// =============================================================================

/// Draw the trace view
pub fn draw(frame: &mut Frame, state: &TraceState, area: Rect) {
    match state.view_mode {
        TraceViewMode::Sessions => draw_sessions(frame, state, area),
        TraceViewMode::Spans => draw_spans(frame, state, area),
        TraceViewMode::SpanDetail => draw_span_detail(frame, state, area),
    }
}

/// Draw sessions list
fn draw_sessions(frame: &mut Frame, state: &TraceState, area: Rect) {
    let block = Block::default()
        .title(" Trace Sessions ")
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.sessions.is_empty() {
        let help = Paragraph::new(
            "No trace sessions found.\n\nRun `deciduous proxy -- claude` to capture API traffic.",
        )
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
        frame.render_widget(help, inner);
        return;
    }

    // Calculate visible items
    let visible_height = inner.height as usize;
    let start = state.session_scroll;
    let end = (start + visible_height).min(state.sessions.len());

    let items: Vec<ListItem> = state.sessions[start..end]
        .iter()
        .enumerate()
        .map(|(idx, session)| {
            let real_idx = start + idx;
            let is_selected = real_idx == state.selected_session_idx;

            // Format: session_id | time | duration | tokens | linked
            let id_short = &session.session_id[..8.min(session.session_id.len())];
            let time_ago = format_relative_time(&session.started_at);
            let duration = format_duration(&session.started_at, session.ended_at.as_deref());
            let tokens_in = format_tokens(session.total_input_tokens);
            let tokens_out = format_tokens(session.total_output_tokens);
            let linked = if session.linked_node_id.is_some() {
                format!(" → #{}", session.linked_node_id.unwrap())
            } else {
                String::new()
            };

            let line = format!(
                " {} │ {:>6} │ {:>6} │ {}↓ {}↑{}",
                id_short, time_ago, duration, tokens_in, tokens_out, linked
            );

            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else if session.linked_node_id.is_some() {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);

    // Scrollbar
    if state.sessions.len() > visible_height {
        let mut scrollbar_state = ScrollbarState::default()
            .content_length(state.sessions.len())
            .position(state.session_scroll);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(
            scrollbar,
            inner.inner(Margin {
                horizontal: 0,
                vertical: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

/// Draw spans list for expanded session
fn draw_spans(frame: &mut Frame, state: &TraceState, area: Rect) {
    let session_id = state.expanded_session.as_deref().unwrap_or("?");
    let title = format!(" Spans: {} ", &session_id[..8.min(session_id.len())]);

    if state.show_detail {
        // Split view: spans on left, detail on right
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        draw_spans_list(frame, state, chunks[0], &title);
        draw_span_preview(frame, state, chunks[1]);
    } else {
        draw_spans_list(frame, state, area, &title);
    }
}

/// Draw the spans list portion
fn draw_spans_list(frame: &mut Frame, state: &TraceState, area: Rect, title: &str) {
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.spans.is_empty() {
        let help = Paragraph::new("No spans recorded for this session.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(help, inner);
        return;
    }

    let visible_height = inner.height as usize;
    let start = state.span_scroll;
    let end = (start + visible_height).min(state.spans.len());

    let items: Vec<ListItem> = state.spans[start..end]
        .iter()
        .enumerate()
        .map(|(idx, span)| {
            let real_idx = start + idx;
            let is_selected = real_idx == state.selected_span_idx;

            // Format: #seq | model | duration | tokens | tools | nodes
            let model = model_short_name(span.model.as_deref());
            let duration = format_duration_ms(span.duration_ms);
            let tokens_in = span.input_tokens.map(format_tokens).unwrap_or("-".into());
            let tokens_out = span.output_tokens.map(format_tokens).unwrap_or("-".into());
            let tools = span.tool_names.as_deref().unwrap_or("-");
            let tools_short = truncate_str(tools, 15);

            // Get node count for this span
            let node_count = state.node_counts.get(&span.id).copied().unwrap_or(0);
            let nodes_str = if node_count > 0 {
                format!(" +{} nodes", node_count)
            } else {
                String::new()
            };

            let line = format!(
                " #{:<2} │ {:>6} │ {:>6} │ {}↓ {}↑ │ {}{}",
                span.sequence_num, model, duration, tokens_in, tokens_out, tools_short, nodes_str
            );

            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else if node_count > 0 {
                // Highlight spans that created nodes
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Draw span preview (thinking/response preview)
fn draw_span_preview(frame: &mut Frame, state: &TraceState, area: Rect) {
    let block = Block::default()
        .title(" Preview ")
        .title_style(Style::default().fg(Color::Magenta))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(span) = state.selected_span() {
        let mut lines = vec![];

        // User preview
        if let Some(ref user) = span.user_preview {
            lines.push(Line::from(vec![
                Span::styled("User: ", Style::default().fg(Color::Cyan)),
                Span::raw(truncate_str(user, 200)),
            ]));
            lines.push(Line::from(""));
        }

        // Thinking preview
        if let Some(ref thinking) = span.thinking_preview {
            lines.push(Line::from(vec![Span::styled(
                "Thinking: ",
                Style::default().fg(Color::Yellow),
            )]));
            for line in thinking.lines().take(5) {
                lines.push(Line::from(format!("  {}", truncate_str(line, 60))));
            }
            lines.push(Line::from(""));
        }

        // Response preview
        if let Some(ref response) = span.response_preview {
            lines.push(Line::from(vec![Span::styled(
                "Response: ",
                Style::default().fg(Color::Green),
            )]));
            for line in response.lines().take(5) {
                lines.push(Line::from(format!("  {}", truncate_str(line, 60))));
            }
        }

        let para = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::White));
        frame.render_widget(para, inner);
    }
}

/// Draw full span detail (modal-style)
fn draw_span_detail(frame: &mut Frame, state: &TraceState, area: Rect) {
    // Tab bar at top
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    // Draw tab bar
    draw_detail_tabs(frame, state, chunks[0]);

    // Content area
    let content = state.current_tab_content();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(chunks[1]);
    frame.render_widget(block, chunks[1]);

    // Special handling for Nodes tab
    if state.detail_tab == DetailTab::Nodes {
        if state.detail_nodes.is_empty() {
            let para = Paragraph::new("No nodes created during this span")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            frame.render_widget(para, inner);
        } else {
            // Render nodes list
            let lines: Vec<Line> = state
                .detail_nodes
                .iter()
                .map(|node| {
                    let type_color = match node.node_type.as_str() {
                        "goal" => Color::Yellow,
                        "action" => Color::Blue,
                        "outcome" => Color::Green,
                        "decision" => Color::Magenta,
                        "observation" => Color::Cyan,
                        _ => Color::White,
                    };
                    Line::from(vec![
                        Span::styled(
                            format!("#{:<3} ", node.id),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(
                            format!("[{}] ", node.node_type),
                            Style::default().fg(type_color),
                        ),
                        Span::raw(&node.title),
                    ])
                })
                .collect();
            let para = Paragraph::new(lines).style(Style::default().fg(Color::White));
            frame.render_widget(para, inner);
        }
        return;
    }

    if content.is_empty() {
        let msg = match state.detail_tab {
            DetailTab::Thinking => "No thinking content",
            DetailTab::Response => "No response content",
            DetailTab::Tools => "No tool calls",
            DetailTab::Nodes => "No nodes created during this span",
        };
        let para = Paragraph::new(msg)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(para, inner);
        return;
    }

    let lines: Vec<Line> = content
        .lines()
        .skip(state.detail_scroll)
        .take(inner.height as usize)
        .map(|l| Line::from(l.to_string()))
        .collect();

    let para = Paragraph::new(lines)
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: false });
    frame.render_widget(para, inner);
}

/// Draw detail tab bar
fn draw_detail_tabs(frame: &mut Frame, state: &TraceState, area: Rect) {
    let tabs = [
        ("Thinking", DetailTab::Thinking),
        ("Response", DetailTab::Response),
        ("Tools", DetailTab::Tools),
        ("Nodes", DetailTab::Nodes),
    ];

    let tab_spans: Vec<Span> = tabs
        .iter()
        .map(|(name, tab)| {
            let style = if *tab == state.detail_tab {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Span::styled(format!(" {} ", name), style)
        })
        .collect();

    let mut line_spans = vec![Span::raw(" ")];
    for (i, tab_span) in tab_spans.into_iter().enumerate() {
        line_spans.push(tab_span);
        if i < tabs.len() - 1 {
            line_spans.push(Span::raw(" │ "));
        }
    }
    line_spans.push(Span::styled(
        "  Tab: switch │ Esc: back",
        Style::default().fg(Color::DarkGray),
    ));

    let line = Line::from(line_spans);
    let para = Paragraph::new(vec![Line::from(""), line]).alignment(Alignment::Left);

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));

    frame.render_widget(para, block.inner(area));
    frame.render_widget(block, area);
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(15000), "15k");
    }

    #[test]
    fn test_format_duration_ms() {
        assert_eq!(format_duration_ms(Some(500)), "500ms");
        assert_eq!(format_duration_ms(Some(1500)), "1.5s");
        assert_eq!(format_duration_ms(None), "-");
    }

    #[test]
    fn test_model_short_name() {
        assert_eq!(
            model_short_name(Some("claude-3-5-sonnet-20241022")),
            "sonnet"
        );
        assert_eq!(model_short_name(Some("claude-opus-4")), "opus");
        assert_eq!(model_short_name(Some("claude-3-5-haiku-20241022")), "haiku");
        assert_eq!(model_short_name(None), "-");
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 8), "hello...");
    }

    #[test]
    fn test_trace_state_navigation() {
        let mut state = TraceState::new();
        state.sessions = vec![
            TraceSession {
                id: 1,
                session_id: "session1".to_string(),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                ended_at: None,
                working_dir: None,
                git_branch: None,
                command: None,
                summary: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                total_cache_read: 0,
                total_cache_write: 0,
                linked_node_id: None,
                linked_change_id: None,
            },
            TraceSession {
                id: 2,
                session_id: "session2".to_string(),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                ended_at: None,
                working_dir: None,
                git_branch: None,
                command: None,
                summary: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                total_cache_read: 0,
                total_cache_write: 0,
                linked_node_id: None,
                linked_change_id: None,
            },
        ];

        assert_eq!(state.selected_session_idx, 0);
        state.move_down();
        assert_eq!(state.selected_session_idx, 1);
        state.move_down();
        assert_eq!(state.selected_session_idx, 1); // Can't go past end
        state.move_up();
        assert_eq!(state.selected_session_idx, 0);
    }
}
