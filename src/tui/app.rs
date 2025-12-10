//! Application state for the TUI

use std::path::{Path, PathBuf};
use std::time::Instant;

use crossterm::event::{MouseEvent, MouseEventKind, MouseButton};

use crate::{Database, DecisionGraph, DecisionNode, DecisionEdge};
use super::types;

/// Current view mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Timeline,
    Dag,
}

/// Current input focus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Detail,
    Search,
    FilePicker,
    Help,
}

/// Input mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    Command,
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

    // Detail panel
    pub detail_expanded: bool,
    pub detail_scroll: usize,

    // Filters
    pub type_filter: Option<String>,
    pub branch_filter: Option<String>,
    pub search_query: String,

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
            detail_expanded: true,
            detail_scroll: 0,
            type_filter: None,
            branch_filter: None,
            search_query: String::new(),
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
    pub fn apply_filters(&mut self) {
        let mut nodes: Vec<DecisionNode> = self.graph.nodes.clone();

        // Type filter
        if let Some(ref type_filter) = self.type_filter {
            nodes.retain(|n| &n.node_type == type_filter);
        }

        // Branch filter
        if let Some(ref branch_filter) = self.branch_filter {
            nodes.retain(|n| {
                types::get_branch(n)
                    .map(|b| b == *branch_filter)
                    .unwrap_or(false)
            });
        }

        // Search filter
        if !self.search_query.is_empty() {
            let query = self.search_query.to_lowercase();
            nodes.retain(|n| {
                n.title.to_lowercase().contains(&query)
                    || n.description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query))
                        .unwrap_or(false)
            });
        }

        // Sort by created_at descending
        nodes.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        self.filtered_nodes = nodes;

        // Adjust selection if needed
        if self.selected_index >= self.filtered_nodes.len() && !self.filtered_nodes.is_empty() {
            self.selected_index = self.filtered_nodes.len() - 1;
        }
    }

    /// Get currently selected node
    pub fn selected_node(&self) -> Option<&DecisionNode> {
        self.filtered_nodes.get(self.selected_index)
    }

    /// Get edges for a node
    pub fn get_node_edges(&self, node_id: i32) -> (Vec<&DecisionEdge>, Vec<&DecisionEdge>) {
        let incoming: Vec<_> = self.graph.edges.iter().filter(|e| e.to_node_id == node_id).collect();
        let outgoing: Vec<_> = self.graph.edges.iter().filter(|e| e.from_node_id == node_id).collect();
        (incoming, outgoing)
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

    // Navigation methods
    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.ensure_visible();
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.filtered_nodes.len() {
            self.selected_index += 1;
            self.ensure_visible();
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
            View::Timeline => View::Dag,
            View::Dag => View::Timeline,
        };
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

    /// Open files in editor
    pub fn open_files(&mut self, files: Vec<String>) {
        if files.is_empty() {
            self.set_status("No files to open".to_string());
            return;
        }

        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

        // We need to temporarily leave the TUI to open the editor
        // This is handled by the caller
        self.set_status(format!("Opening {} file(s) in {}", files.len(), editor));
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
}
