//! Application state for the TUI

use std::path::{Path, PathBuf};
use std::time::Instant;

use crossterm::event::{MouseEvent, MouseEventKind, MouseButton};
use ratatui::style::Color;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::easy::HighlightLines;

use crate::{Database, DecisionGraph, DecisionNode, DecisionEdge};
use super::types;
use super::views::roadmap::RoadmapState;

// Lazy static syntax highlighting resources
lazy_static::lazy_static! {
    static ref PS: SyntaxSet = SyntaxSet::load_defaults_newlines();
    static ref TS: ThemeSet = ThemeSet::load_defaults();
}

/// Convert syntect color to ratatui color
fn syntect_to_ratatui_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

/// Current view mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Timeline,
    Dag,
    Roadmap,
}

/// Current input focus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Detail,
    Search,
    FilePicker,
    Help,
    Modal,
}

/// Modal content types
#[derive(Debug, Clone)]
pub enum ModalContent {
    Commit {
        hash: String,
        node_title: String,
        commit_message: String,
        diff_lines: Vec<StyledDiffLine>,  // Pre-rendered diff lines
        files: Vec<String>,
    },
    NodeDetail { node_id: i32 },
    GoalStory { goal_id: i32 },
    FilePreview { path: String, content: String },
    FileDiff { path: String, diff: String },
}

/// Pre-rendered diff line with styling info
#[derive(Debug, Clone)]
pub struct StyledDiffLine {
    pub line_type: DiffLineType,
    pub content: String,
    /// Pre-computed styled spans for syntax highlighting (computed once at modal open)
    pub styled_spans: Vec<(ratatui::style::Color, String)>,
}

#[derive(Debug, Clone, Copy)]
pub enum DiffLineType {
    Header,      // diff, index, +++, ---
    Hunk,        // @@
    Added,       // +
    Removed,     // -
    Context,     // space
    Other,
}

/// Scrollable modal state
#[derive(Debug, Clone, Default)]
pub struct ModalScroll {
    pub offset: usize,
    pub total_lines: usize,
}

/// Which section of split modal is focused
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModalSection {
    #[default]
    Top,
    Bottom,
}

/// State for commit modal with split sections
#[derive(Debug, Clone, Default)]
pub struct CommitModalState {
    pub section: ModalSection,
    pub diff_scroll: usize,
    pub diff_total_lines: usize,
}

/// Input mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    Command,
    BranchSearch,
}

/// File picker state for multi-select
#[derive(Debug, Clone)]
pub struct FilePicker {
    pub files: Vec<String>,
    pub selected: Vec<bool>,
    pub cursor: usize,
}

impl FilePicker {
    pub fn new(files: Vec<String>) -> Self {
        let len = files.len();
        Self {
            files,
            selected: vec![false; len],
            cursor: 0,
        }
    }

    pub fn toggle_current(&mut self) {
        if !self.files.is_empty() {
            self.selected[self.cursor] = !self.selected[self.cursor];
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor + 1 < self.files.len() {
            self.cursor += 1;
        }
    }

    pub fn get_selected_files(&self) -> Vec<String> {
        self.files
            .iter()
            .zip(self.selected.iter())
            .filter(|(_, &sel)| sel)
            .map(|(f, _)| f.clone())
            .collect()
    }
}

/// Main application state
pub struct App {
    // Database
    db: Database,
    db_path: PathBuf,

    // Graph data
    pub graph: DecisionGraph,
    pub filtered_nodes: Vec<DecisionNode>,

    // View state
    pub current_view: View,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub reverse_order: bool,  // true = chronological (oldest first), false = newest first

    // Detail panel
    pub detail_expanded: bool,
    pub detail_scroll: usize,

    // Filters
    pub type_filter: Option<String>,
    pub branch_filter: Option<String>,
    pub search_query: String,
    pub branch_search_query: String,
    pub branch_search_matches: Vec<String>,
    pub branch_search_index: usize,

    // UI state
    pub focus: Focus,
    pub mode: Mode,
    pub file_picker: Option<FilePicker>,
    pub show_help: bool,

    // Viewport
    pub viewport_width: u16,
    pub viewport_height: u16,

    // DAG view state
    pub dag_offset_x: i32,
    pub dag_offset_y: i32,
    pub dag_zoom: f32,

    // Refresh indicator
    pub refresh_shown_at: Option<Instant>,

    // Vim-style 'g' prefix tracking
    pub pending_g: bool,

    // Status message
    pub status_message: Option<(String, Instant)>,

    // Modal
    pub modal: Option<ModalContent>,
    pub modal_scroll: ModalScroll,
    pub commit_modal: CommitModalState,

    // Detail panel file browser
    pub detail_file_index: usize,
    pub detail_in_files: bool,  // true when navigating files section

    // Pending files to open in editor (set by app, consumed by main loop)
    pub pending_editor_files: Option<Vec<String>>,

    // Roadmap view state
    pub roadmap_state: RoadmapState,
}

impl App {
    pub fn new(db_path: Option<PathBuf>) -> Result<Self, Box<dyn std::error::Error>> {
        let db = if let Some(path) = &db_path {
            std::env::set_var("DECIDUOUS_DB_PATH", path);
            Database::open()?
        } else {
            Database::open()?
        };

        let actual_path = Database::db_path();
        let graph = db.get_graph()?;
        let filtered_nodes = graph.nodes.clone();

        // Sort by created_at descending (newest first)
        let mut filtered_nodes = filtered_nodes;
        filtered_nodes.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(Self {
            db,
            db_path: actual_path,
            graph,
            filtered_nodes,
            current_view: View::Timeline,
            selected_index: 0,
            scroll_offset: 0,
            reverse_order: false,  // Default: newest first
            detail_expanded: true,
            detail_scroll: 0,
            type_filter: None,
            branch_filter: None,
            search_query: String::new(),
            branch_search_query: String::new(),
            branch_search_matches: vec![],
            branch_search_index: 0,
            focus: Focus::List,
            mode: Mode::Normal,
            file_picker: None,
            show_help: false,
            viewport_width: 80,
            viewport_height: 24,
            dag_offset_x: 0,
            dag_offset_y: 0,
            dag_zoom: 1.0,
            refresh_shown_at: None,
            pending_g: false,
            status_message: None,
            modal: None,
            modal_scroll: ModalScroll::default(),
            commit_modal: CommitModalState::default(),
            detail_file_index: 0,
            detail_in_files: false,
            pending_editor_files: None,
            roadmap_state: RoadmapState::new(),
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Reload the graph from database
    pub fn reload_graph(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.graph = self.db.get_graph()?;
        self.apply_filters();
        Ok(())
    }

    /// Show the refresh indicator
    pub fn show_refresh_indicator(&mut self) {
        self.refresh_shown_at = Some(Instant::now());
    }

    /// Periodic tick for animations
    pub fn tick(&mut self) {
        // Clear refresh indicator after 2 seconds
        if let Some(shown_at) = self.refresh_shown_at {
            if shown_at.elapsed().as_secs() >= 2 {
                self.refresh_shown_at = None;
            }
        }

        // Clear status message after 3 seconds
        if let Some((_, shown_at)) = &self.status_message {
            if shown_at.elapsed().as_secs() >= 3 {
                self.status_message = None;
            }
        }
    }

    /// Apply current filters to the node list
    /// Delegates to pure function in state.rs
    pub fn apply_filters(&mut self) {
        self.filtered_nodes = super::state::apply_all_filters(
            &self.graph.nodes,
            self.type_filter.as_deref(),
            self.branch_filter.as_deref(),
            &self.search_query,
            self.reverse_order,
        );

        // Adjust selection if needed
        if self.selected_index >= self.filtered_nodes.len() && !self.filtered_nodes.is_empty() {
            self.selected_index = self.filtered_nodes.len() - 1;
        }
    }

    /// Get currently selected node
    pub fn selected_node(&self) -> Option<&DecisionNode> {
        self.filtered_nodes.get(self.selected_index)
    }

    /// Get edges for a node (incoming, outgoing)
    /// Delegates to pure functions in types.rs
    pub fn get_node_edges(&self, node_id: i32) -> (Vec<&DecisionEdge>, Vec<&DecisionEdge>) {
        (
            types::get_incoming_edges(node_id, &self.graph.edges),
            types::get_outgoing_edges(node_id, &self.graph.edges),
        )
    }

    /// Get node by ID
    pub fn get_node_by_id(&self, id: i32) -> Option<&DecisionNode> {
        self.graph.nodes.iter().find(|n| n.id == id)
    }

    /// Parse metadata JSON and extract confidence
    /// Delegates to types::get_confidence for consistency
    pub fn get_confidence(node: &DecisionNode) -> Option<i32> {
        types::get_confidence(node)
    }

    /// Parse metadata and extract commit hash
    /// Delegates to types::get_commit for consistency
    pub fn get_commit(node: &DecisionNode) -> Option<String> {
        types::get_commit(node)
    }

    /// Parse metadata and extract files
    /// Delegates to types::get_files for consistency
    pub fn get_files(node: &DecisionNode) -> Vec<String> {
        types::get_files(node)
    }

    /// Parse metadata and extract branch
    /// Delegates to types::get_branch for consistency
    pub fn get_branch(node: &DecisionNode) -> Option<String> {
        types::get_branch(node)
    }

    /// Parse metadata and extract prompt
    /// Delegates to types::get_prompt for consistency
    pub fn get_prompt(node: &DecisionNode) -> Option<String> {
        types::get_prompt(node)
    }

    // Navigation methods
    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.ensure_visible();
            self.reset_file_browser(); // Reset file index when changing nodes
        }
    }

    /// Reset file browser state when changing nodes
    fn reset_file_browser(&mut self) {
        self.detail_file_index = 0;
        self.detail_in_files = false;
    }

    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.filtered_nodes.len() {
            self.selected_index += 1;
            self.ensure_visible();
            self.reset_file_browser(); // Reset file index when changing nodes
        }
    }

    pub fn jump_to_top(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn jump_to_bottom(&mut self) {
        if !self.filtered_nodes.is_empty() {
            self.selected_index = self.filtered_nodes.len() - 1;
            self.ensure_visible();
        }
    }

    pub fn page_down(&mut self) {
        let page_size = (self.viewport_height as usize).saturating_sub(6);
        self.selected_index = (self.selected_index + page_size).min(self.filtered_nodes.len().saturating_sub(1));
        self.ensure_visible();
    }

    pub fn page_up(&mut self) {
        let page_size = (self.viewport_height as usize).saturating_sub(6);
        self.selected_index = self.selected_index.saturating_sub(page_size);
        self.ensure_visible();
    }

    fn ensure_visible(&mut self) {
        let visible_height = (self.viewport_height as usize).saturating_sub(6);
        let item_height = 3; // Each node takes ~3 lines
        let visible_items = visible_height / item_height;

        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + visible_items {
            self.scroll_offset = self.selected_index.saturating_sub(visible_items - 1);
        }
    }

    pub fn toggle_view(&mut self) {
        self.current_view = match self.current_view {
            View::Timeline => {
                // Load roadmap items when switching to roadmap view
                self.load_roadmap_items();
                View::Roadmap
            }
            View::Roadmap => View::Timeline,
            View::Dag => View::Timeline, // DAG view disabled
        };
    }

    /// Load roadmap items from database
    pub fn load_roadmap_items(&mut self) {
        match self.db.get_all_roadmap_items() {
            Ok(items) => {
                self.roadmap_state.set_items(items);
            }
            Err(e) => {
                self.set_status(format!("Failed to load roadmap: {}", e));
            }
        }
    }

    /// Toggle checkbox state for a roadmap item
    pub fn toggle_roadmap_checkbox(&mut self, item_id: i32, new_state: &str) -> Result<(), String> {
        self.db
            .update_roadmap_item_checkbox(item_id, new_state)
            .map_err(|e| e.to_string())
    }

    pub fn toggle_detail(&mut self) {
        self.detail_expanded = !self.detail_expanded;
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    pub fn handle_mouse(&mut self, event: MouseEvent) {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // TODO: Implement click detection based on areas
            }
            MouseEventKind::ScrollDown => {
                self.move_down();
            }
            MouseEventKind::ScrollUp => {
                self.move_up();
            }
            _ => {}
        }
    }

    pub fn set_status(&mut self, message: String) {
        self.status_message = Some((message, Instant::now()));
    }

    /// Open files in editor - stores files to open, caller handles actual opening
    pub fn open_files(&mut self, files: Vec<String>) {
        if files.is_empty() {
            self.set_status("No files to open".to_string());
            return;
        }
        self.pending_editor_files = Some(files);
    }

    /// Check if we have pending files to open in editor
    pub fn take_pending_editor_files(&mut self) -> Option<Vec<String>> {
        self.pending_editor_files.take()
    }

    /// Show file picker for multi-file selection
    pub fn show_file_picker(&mut self, files: Vec<String>) {
        if files.len() == 1 {
            // Single file - open directly
            self.open_files(files);
        } else if !files.is_empty() {
            self.file_picker = Some(FilePicker::new(files));
            self.focus = Focus::FilePicker;
        }
    }

    /// Cycle through type filters
    pub fn cycle_type_filter(&mut self) {
        let types = ["goal", "decision", "option", "action", "outcome", "observation"];
        self.type_filter = match &self.type_filter {
            None => Some(types[0].to_string()),
            Some(current) => {
                let idx = types.iter().position(|t| t == current);
                match idx {
                    Some(i) if i + 1 < types.len() => Some(types[i + 1].to_string()),
                    _ => None,
                }
            }
        };
        self.apply_filters();
    }

    /// Toggle timeline order (chronological vs reverse-chronological)
    pub fn toggle_order(&mut self) {
        self.reverse_order = !self.reverse_order;
        self.apply_filters();
        if self.reverse_order {
            self.set_status("Timeline: Chronological (oldest first)".to_string());
        } else {
            self.set_status("Timeline: Reverse-chronological (newest first)".to_string());
        }
    }

    // DAG navigation
    pub fn dag_pan(&mut self, dx: i32, dy: i32) {
        self.dag_offset_x += dx * 5;
        self.dag_offset_y += dy * 5;
    }

    pub fn dag_zoom_in(&mut self) {
        self.dag_zoom = (self.dag_zoom * 1.2).min(3.0);
    }

    pub fn dag_zoom_out(&mut self) {
        self.dag_zoom = (self.dag_zoom / 1.2).max(0.3);
    }

    pub fn dag_reset_zoom(&mut self) {
        self.dag_zoom = 1.0;
        self.dag_offset_x = 0;
        self.dag_offset_y = 0;
    }

    /// Show commit modal for current node
    pub fn show_commit_modal(&mut self) {
        if let Some(node) = self.selected_node() {
            if let Some(commit) = types::get_commit(node) {
                // Get commit message (without diff)
                let commit_message = std::process::Command::new("git")
                    .args(["log", "-1", "--format=%B", &commit])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_else(|e| format!("Failed to get commit message: {}", e));

                // Get full diff
                let diff_output = std::process::Command::new("git")
                    .args(["show", "--format=", "-p", &commit])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                    .unwrap_or_else(|e| format!("Failed to get diff: {}", e));

                // Get list of changed files
                let files: Vec<String> = std::process::Command::new("git")
                    .args(["show", "--format=", "--name-only", &commit])
                    .output()
                    .map(|o| {
                        String::from_utf8_lossy(&o.stdout)
                            .lines()
                            .filter(|l| !l.is_empty())
                            .map(|l| l.to_string())
                            .collect()
                    })
                    .unwrap_or_default();

                // Pre-process diff lines with syntax highlighting (computed once, used many times)
                // Use base16-mocha.dark for better visibility on dark terminals
                let theme = &TS.themes["base16-mocha.dark"];
                let mut current_file: Option<String> = None;
                let mut highlighter: Option<HighlightLines> = None;

                let diff_lines: Vec<StyledDiffLine> = diff_output
                    .lines()
                    .map(|line| {
                        // Classify line type
                        let line_type = if line.starts_with("@@") {
                            DiffLineType::Hunk
                        } else if line.starts_with("diff ") || line.starts_with("index ")
                            || line.starts_with("+++") || line.starts_with("---") {
                            DiffLineType::Header
                        } else if line.starts_with('+') {
                            DiffLineType::Added
                        } else if line.starts_with('-') {
                            DiffLineType::Removed
                        } else if line.starts_with(' ') {
                            DiffLineType::Context
                        } else {
                            DiffLineType::Other
                        };

                        // Track current file for syntax detection
                        if let Some(stripped) = line.strip_prefix("+++ b/") {
                            current_file = Some(stripped.to_string());
                            // Create highlighter for this file type
                            if let Some(ref path) = current_file {
                                if let Some(syntax) = PS.find_syntax_for_file(path).ok().flatten() {
                                    highlighter = Some(HighlightLines::new(syntax, theme));
                                } else {
                                    highlighter = None;
                                }
                            }
                        }

                        // Compute styled spans for content lines
                        let styled_spans = if matches!(line_type, DiffLineType::Added | DiffLineType::Removed | DiffLineType::Context) {
                            // Strip the leading +/- or space for highlighting
                            let code_content = if line.len() > 1 { &line[1..] } else { "" };

                            if let Some(ref mut hl) = highlighter {
                                // Use syntax highlighting
                                if let Ok(ranges) = hl.highlight_line(code_content, &PS) {
                                    ranges.iter().map(|(style, text)| {
                                        let color = syntect_to_ratatui_color(style.foreground);
                                        (color, text.to_string())
                                    }).collect()
                                } else {
                                    vec![(Color::White, code_content.to_string())]
                                }
                            } else {
                                vec![(Color::White, code_content.to_string())]
                            }
                        } else {
                            // Headers/hunks - no syntax highlighting needed
                            vec![]
                        };

                        StyledDiffLine {
                            line_type,
                            content: line.to_string(),
                            styled_spans,
                        }
                    })
                    .collect();

                let diff_line_count = diff_lines.len();

                self.modal = Some(ModalContent::Commit {
                    hash: commit,
                    node_title: node.title.clone(),
                    commit_message,
                    diff_lines,
                    files,
                });
                self.modal_scroll = ModalScroll::default();
                self.commit_modal = CommitModalState {
                    section: ModalSection::Top,
                    diff_scroll: 0,
                    diff_total_lines: diff_line_count,
                };
                self.focus = Focus::Modal;
            } else {
                self.set_status("No commit associated with this node".to_string());
            }
        }
    }

    /// Close the modal
    pub fn close_modal(&mut self) {
        self.modal = None;
        self.modal_scroll = ModalScroll::default();
        self.focus = Focus::List;
    }

    /// Scroll modal up
    pub fn modal_scroll_up(&mut self, amount: usize) {
        self.modal_scroll.offset = self.modal_scroll.offset.saturating_sub(amount);
    }

    /// Scroll modal down
    pub fn modal_scroll_down(&mut self, amount: usize) {
        let max_scroll = self.modal_scroll.total_lines.saturating_sub(10); // Leave some visible
        self.modal_scroll.offset = (self.modal_scroll.offset + amount).min(max_scroll);
    }

    /// Navigate commit modal - move focus or scroll within section
    pub fn commit_modal_down(&mut self, amount: usize) {
        match self.commit_modal.section {
            ModalSection::Top => {
                // Move from top to bottom section
                self.commit_modal.section = ModalSection::Bottom;
            }
            ModalSection::Bottom => {
                // Scroll the diff section
                let max_scroll = self.commit_modal.diff_total_lines.saturating_sub(10);
                self.commit_modal.diff_scroll = (self.commit_modal.diff_scroll + amount).min(max_scroll);
            }
        }
    }

    /// Navigate commit modal up - scroll or move focus
    pub fn commit_modal_up(&mut self, amount: usize) {
        match self.commit_modal.section {
            ModalSection::Top => {
                // Already at top, do nothing
            }
            ModalSection::Bottom => {
                if self.commit_modal.diff_scroll == 0 {
                    // At top of diff, move focus back to top section
                    self.commit_modal.section = ModalSection::Top;
                } else {
                    // Scroll diff up
                    self.commit_modal.diff_scroll = self.commit_modal.diff_scroll.saturating_sub(amount);
                }
            }
        }
    }

    /// Page down in commit modal diff section
    pub fn commit_modal_page_down(&mut self, amount: usize) {
        self.commit_modal.section = ModalSection::Bottom;
        let max_scroll = self.commit_modal.diff_total_lines.saturating_sub(10);
        self.commit_modal.diff_scroll = (self.commit_modal.diff_scroll + amount).min(max_scroll);
    }

    /// Page up in commit modal diff section
    pub fn commit_modal_page_up(&mut self, amount: usize) {
        self.commit_modal.diff_scroll = self.commit_modal.diff_scroll.saturating_sub(amount);
        if self.commit_modal.diff_scroll == 0 {
            self.commit_modal.section = ModalSection::Top;
        }
    }

    /// Jump to top of commit modal
    pub fn commit_modal_top(&mut self) {
        self.commit_modal.section = ModalSection::Top;
        self.commit_modal.diff_scroll = 0;
    }

    /// Jump to bottom of commit modal diff
    pub fn commit_modal_bottom(&mut self) {
        self.commit_modal.section = ModalSection::Bottom;
        self.commit_modal.diff_scroll = self.commit_modal.diff_total_lines.saturating_sub(10);
    }

    /// Get the current file path from modal (if it's a file modal)
    pub fn get_modal_file_path(&self) -> Option<String> {
        match &self.modal {
            Some(ModalContent::FilePreview { path, .. }) => Some(path.clone()),
            Some(ModalContent::FileDiff { path, .. }) => Some(path.clone()),
            _ => None,
        }
    }

    /// Open the file shown in the current modal
    pub fn open_modal_file(&mut self) {
        if let Some(path) = self.get_modal_file_path() {
            self.pending_editor_files = Some(vec![path]);
        }
    }

    /// Get unique branches from the graph
    pub fn get_unique_branches(&self) -> Vec<String> {
        types::get_unique_branches(&self.graph.nodes)
    }

    /// Cycle through branch filters
    pub fn cycle_branch_filter(&mut self) {
        let branches = self.get_unique_branches();
        if branches.is_empty() {
            self.set_status("No branches found".to_string());
            return;
        }

        self.branch_filter = match &self.branch_filter {
            None => Some(branches[0].clone()),
            Some(current) => {
                let idx = branches.iter().position(|b| b == current);
                match idx {
                    Some(i) if i + 1 < branches.len() => Some(branches[i + 1].clone()),
                    _ => None, // Cycle back to "all"
                }
            }
        };

        if let Some(ref branch) = self.branch_filter {
            self.set_status(format!("Filter: {}", branch));
        } else {
            self.set_status("Filter: All branches".to_string());
        }

        self.apply_filters();
    }

    /// Enter branch search mode
    pub fn enter_branch_search(&mut self) {
        self.mode = Mode::BranchSearch;
        self.branch_search_query.clear();
        self.branch_search_matches = self.get_unique_branches();
        self.branch_search_index = 0;
    }

    /// Update branch search matches based on query
    pub fn update_branch_search(&mut self) {
        let all_branches = self.get_unique_branches();
        let query = self.branch_search_query.to_lowercase();

        if query.is_empty() {
            self.branch_search_matches = all_branches;
        } else {
            self.branch_search_matches = all_branches
                .into_iter()
                .filter(|b| b.to_lowercase().contains(&query))
                .collect();
        }
        self.branch_search_index = 0;
    }

    /// Select the current branch from search results
    pub fn select_branch_from_search(&mut self) {
        if !self.branch_search_matches.is_empty() {
            let selected = self.branch_search_matches[self.branch_search_index].clone();
            self.branch_filter = Some(selected.clone());
            self.apply_filters();
            self.set_status(format!("Branch filter: {}", selected));
        }
        self.mode = Mode::Normal;
        self.branch_search_query.clear();
    }

    /// Move to next match in branch search
    pub fn branch_search_next(&mut self) {
        if !self.branch_search_matches.is_empty() {
            self.branch_search_index = (self.branch_search_index + 1) % self.branch_search_matches.len();
        }
    }

    /// Move to previous match in branch search
    pub fn branch_search_prev(&mut self) {
        if !self.branch_search_matches.is_empty() {
            if self.branch_search_index == 0 {
                self.branch_search_index = self.branch_search_matches.len() - 1;
            } else {
                self.branch_search_index -= 1;
            }
        }
    }

    /// Show story for selected goal (or find parent goal if on another node type)
    pub fn show_goal_story(&mut self) {
        if let Some(node) = self.selected_node() {
            if node.node_type == "goal" {
                self.modal = Some(ModalContent::GoalStory { goal_id: node.id });
                self.focus = Focus::Modal;
            } else {
                // Try to find the root goal by traversing up
                if let Some(goal_id) = self.find_root_goal(node.id) {
                    self.modal = Some(ModalContent::GoalStory { goal_id });
                    self.focus = Focus::Modal;
                } else {
                    self.set_status("No parent goal found for this node".to_string());
                }
            }
        }
    }

    /// Find the root goal by traversing incoming edges
    /// Delegates to pure function in state.rs
    pub fn find_root_goal(&self, start_id: i32) -> Option<i32> {
        super::state::find_root_goal(start_id, &self.graph.nodes, &self.graph.edges)
    }

    /// Get all descendant nodes from a goal (BFS traversal)
    /// Delegates to pure function in state.rs and enriches with node references
    pub fn get_goal_descendants(&self, goal_id: i32) -> Vec<(i32, &DecisionNode, usize)> {
        super::state::get_descendants(goal_id, &self.graph.nodes, &self.graph.edges)
            .into_iter()
            .filter_map(|(node_id, depth)| {
                self.get_node_by_id(node_id).map(|node| (node_id, node, depth))
            })
            .collect()
    }

    /// Get all goals in the graph
    pub fn get_goals(&self) -> Vec<&DecisionNode> {
        self.graph.nodes.iter().filter(|n| n.node_type == "goal").collect()
    }

    /// Get files for currently selected node
    pub fn get_current_files(&self) -> Vec<String> {
        self.selected_node()
            .map(types::get_files)
            .unwrap_or_default()
    }

    /// Toggle file browser mode in detail panel
    pub fn toggle_file_browser(&mut self) {
        let files = self.get_current_files();
        if files.is_empty() {
            self.set_status("No files for this node".to_string());
            return;
        }
        self.detail_in_files = !self.detail_in_files;
        if self.detail_in_files {
            self.detail_file_index = 0;
            self.set_status(format!("File browser: {}/{}", 1, files.len()));
        }
    }

    /// Move to next file in detail panel
    pub fn next_file(&mut self) {
        let files = self.get_current_files();
        if !files.is_empty() && self.detail_file_index + 1 < files.len() {
            self.detail_file_index += 1;
            self.set_status(format!("File {}/{}: {}", self.detail_file_index + 1, files.len(), files[self.detail_file_index]));
        }
    }

    /// Move to previous file in detail panel
    pub fn prev_file(&mut self) {
        if self.detail_file_index > 0 {
            self.detail_file_index -= 1;
            let files = self.get_current_files();
            self.set_status(format!("File {}/{}: {}", self.detail_file_index + 1, files.len(), files[self.detail_file_index]));
        }
    }

    /// Show file preview modal
    pub fn show_file_preview(&mut self) {
        let files = self.get_current_files();
        if files.is_empty() {
            self.set_status("No files for this node".to_string());
            return;
        }

        let path = &files[self.detail_file_index.min(files.len() - 1)];

        // Read raw file content - UI will handle formatting and syntax highlighting
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| format!("Error reading file: {}", e));

        self.modal = Some(ModalContent::FilePreview {
            path: path.clone(),
            content,
        });
        self.modal_scroll = ModalScroll::default();
        self.focus = Focus::Modal;
    }

    /// Show file diff modal
    pub fn show_file_diff(&mut self) {
        let files = self.get_current_files();
        if files.is_empty() {
            self.set_status("No files for this node".to_string());
            return;
        }

        let path = &files[self.detail_file_index.min(files.len() - 1)];

        // Get git diff for this file
        let diff = std::process::Command::new("git")
            .args(["diff", "HEAD~5..HEAD", "--", path])
            .output()
            .map(|o| {
                let stdout = String::from_utf8_lossy(&o.stdout);
                if stdout.is_empty() {
                    "No recent changes to this file".to_string()
                } else {
                    stdout.to_string()
                }
            })
            .unwrap_or_else(|e| format!("Error running git diff: {}", e));

        self.modal = Some(ModalContent::FileDiff {
            path: path.clone(),
            diff,
        });
        self.focus = Focus::Modal;
    }

    /// Open currently selected file in editor
    pub fn open_current_file(&mut self) {
        let files = self.get_current_files();
        if files.is_empty() {
            self.set_status("No files for this node".to_string());
            return;
        }

        let path = files[self.detail_file_index.min(files.len() - 1)].clone();
        self.open_files(vec![path]);
    }
}
