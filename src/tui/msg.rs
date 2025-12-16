//! TEA Message Types for the TUI
//!
//! This module defines the Msg enum representing all possible user actions.
//! Following The Elm Architecture (TEA), messages are:
//! - Data describing what happened (not how to handle it)
//! - The only way to trigger state changes
//! - Processed by a single update function
//!
//! Key principles:
//! - Messages are just data (no behavior)
//! - Messages are exhaustive (every user action has a message)
//! - Messages are named by what happened, not what will happen

use crossterm::event::{KeyCode, KeyModifiers, MouseEvent};

/// All possible messages/actions in the TUI
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    // === Navigation ===
    /// Move selection up by one
    MoveUp,
    /// Move selection down by one
    MoveDown,
    /// Move selection up by page
    PageUp,
    /// Move selection down by page
    PageDown,
    /// Jump to first item
    JumpToTop,
    /// Jump to last item
    JumpToBottom,
    /// Select item by index (for mouse clicks)
    SelectIndex(usize),

    // === View Switching ===
    /// Cycle to next view (Tab)
    NextView,
    /// Cycle to previous view (Shift+Tab)
    PrevView,
    /// Switch to specific view
    SwitchToView(ViewKind),

    // === Filtering ===
    /// Cycle through type filters
    CycleTypeFilter,
    /// Cycle through branch filters
    CycleBranchFilter,
    /// Open branch search modal
    OpenBranchSearch,
    /// Update search query
    SetSearchQuery(String),
    /// Clear all filters
    ClearFilters,

    // === Search Modal ===
    /// Add character to search input
    SearchInput(char),
    /// Remove character from search input
    SearchBackspace,
    /// Confirm search and close modal
    SearchConfirm,
    /// Cancel search and close modal
    SearchCancel,

    // === Detail Panel ===
    /// Toggle detail panel visibility
    ToggleDetailPanel,
    /// Scroll detail panel up
    DetailScrollUp,
    /// Scroll detail panel down
    DetailScrollDown,

    // === Modals ===
    /// Toggle help modal
    ToggleHelp,
    /// Open prompt modal for current node
    OpenPromptModal,
    /// Close any open modal
    CloseModal,
    /// Scroll modal content up
    ModalScrollUp,
    /// Scroll modal content down
    ModalScrollDown,

    // === File Browser ===
    /// Toggle file browser visibility
    ToggleFileBrowser,
    /// Navigate into selected directory
    FileBrowserEnter,
    /// Navigate to parent directory
    FileBrowserBack,
    /// Expand/collapse file tree node
    FileBrowserToggle,
    /// Preview selected file
    PreviewFile,
    /// Show diff for selected file
    ShowFileDiff,

    // === Goal Story ===
    /// Toggle goal story view
    ToggleGoalStory,
    /// Expand/collapse goal in story view
    GoalStoryToggle,

    // === Actions ===
    /// Open associated files in editor
    OpenFiles,
    /// Refresh graph from database
    RefreshGraph,
    /// Copy current node info to clipboard
    CopyToClipboard,

    // === Lifecycle ===
    /// Quit the application
    Quit,
    /// Tick event (for animations/updates)
    Tick,
    /// Window resized
    Resize(u16, u16),
    /// Mouse event
    Mouse(MouseEvent),

    // === Internal ===
    /// No operation (for unhandled keys)
    Noop,
}

/// View types in the TUI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    Timeline,
    Dag,
    Graph,
}

impl ViewKind {
    pub fn next(self) -> Self {
        match self {
            ViewKind::Timeline => ViewKind::Dag,
            ViewKind::Dag => ViewKind::Graph,
            ViewKind::Graph => ViewKind::Timeline,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            ViewKind::Timeline => ViewKind::Graph,
            ViewKind::Dag => ViewKind::Timeline,
            ViewKind::Graph => ViewKind::Dag,
        }
    }
}

/// Convert a key event to a message
///
/// This is a pure function - no side effects, just pattern matching.
/// The result is a message that describes what the user intended.
pub fn key_to_msg(
    code: KeyCode,
    modifiers: KeyModifiers,
    modal_open: bool,
    search_active: bool,
) -> Msg {
    // Handle search mode first
    if search_active {
        return match code {
            KeyCode::Enter => Msg::SearchConfirm,
            KeyCode::Esc => Msg::SearchCancel,
            KeyCode::Backspace => Msg::SearchBackspace,
            KeyCode::Char(c) => Msg::SearchInput(c),
            _ => Msg::Noop,
        };
    }

    // Handle modal mode
    if modal_open {
        return match code {
            KeyCode::Esc | KeyCode::Char('q') => Msg::CloseModal,
            KeyCode::Char('j') | KeyCode::Down => Msg::ModalScrollDown,
            KeyCode::Char('k') | KeyCode::Up => Msg::ModalScrollUp,
            KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => Msg::ModalScrollDown,
            KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => Msg::ModalScrollUp,
            _ => Msg::Noop,
        };
    }

    // Normal mode
    match code {
        // Quit
        KeyCode::Char('q') => Msg::Quit,
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => Msg::Quit,

        // Navigation
        KeyCode::Char('j') | KeyCode::Down => Msg::MoveDown,
        KeyCode::Char('k') | KeyCode::Up => Msg::MoveUp,
        KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => Msg::PageDown,
        KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => Msg::PageUp,
        KeyCode::Char('g') => Msg::JumpToTop, // Simplified - real vim needs 'gg'
        KeyCode::Char('G') => Msg::JumpToBottom,
        KeyCode::PageDown => Msg::PageDown,
        KeyCode::PageUp => Msg::PageUp,
        KeyCode::Home => Msg::JumpToTop,
        KeyCode::End => Msg::JumpToBottom,

        // View switching
        KeyCode::Tab => {
            if modifiers.contains(KeyModifiers::SHIFT) {
                Msg::PrevView
            } else {
                Msg::NextView
            }
        }
        KeyCode::Char('1') => Msg::SwitchToView(ViewKind::Timeline),
        KeyCode::Char('2') => Msg::SwitchToView(ViewKind::Dag),
        KeyCode::Char('3') => Msg::SwitchToView(ViewKind::Graph),

        // Filtering
        KeyCode::Char('t') => Msg::CycleTypeFilter,
        KeyCode::Char('b') => Msg::CycleBranchFilter,
        KeyCode::Char('B') => Msg::OpenBranchSearch,
        KeyCode::Char('/') => Msg::OpenBranchSearch, // Also opens search

        // Detail panel
        KeyCode::Char('l') | KeyCode::Right => Msg::DetailScrollDown, // In detail context
        KeyCode::Char('h') | KeyCode::Left => Msg::DetailScrollUp,    // In detail context
        KeyCode::Enter => Msg::ToggleDetailPanel,

        // Modals
        KeyCode::Char('?') => Msg::ToggleHelp,
        KeyCode::Char('P') => Msg::OpenPromptModal,
        KeyCode::Esc => Msg::CloseModal,

        // File browser
        KeyCode::Char('F') => Msg::ToggleFileBrowser,
        KeyCode::Char('p') => Msg::PreviewFile,

        // Goal story
        KeyCode::Char('s') => Msg::ToggleGoalStory,

        // Actions
        KeyCode::Char('o') => Msg::OpenFiles,
        KeyCode::Char('r') => Msg::RefreshGraph,
        KeyCode::Char('y') => Msg::CopyToClipboard,

        _ => Msg::Noop,
    }
}

/// Check if a message should cause the app to quit
pub fn is_quit(msg: &Msg) -> bool {
    matches!(msg, Msg::Quit)
}

/// Check if a message is a navigation action
pub fn is_navigation(msg: &Msg) -> bool {
    matches!(
        msg,
        Msg::MoveUp
            | Msg::MoveDown
            | Msg::PageUp
            | Msg::PageDown
            | Msg::JumpToTop
            | Msg::JumpToBottom
            | Msg::SelectIndex(_)
    )
}

/// Check if a message affects filtering
pub fn is_filter_change(msg: &Msg) -> bool {
    matches!(
        msg,
        Msg::CycleTypeFilter
            | Msg::CycleBranchFilter
            | Msg::SetSearchQuery(_)
            | Msg::ClearFilters
            | Msg::SearchConfirm
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_kind_cycle() {
        assert_eq!(ViewKind::Timeline.next(), ViewKind::Dag);
        assert_eq!(ViewKind::Dag.next(), ViewKind::Graph);
        assert_eq!(ViewKind::Graph.next(), ViewKind::Timeline);

        assert_eq!(ViewKind::Timeline.prev(), ViewKind::Graph);
        assert_eq!(ViewKind::Graph.prev(), ViewKind::Dag);
    }

    #[test]
    fn test_key_to_msg_navigation() {
        assert_eq!(
            key_to_msg(KeyCode::Char('j'), KeyModifiers::NONE, false, false),
            Msg::MoveDown
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('k'), KeyModifiers::NONE, false, false),
            Msg::MoveUp
        );
        assert_eq!(
            key_to_msg(KeyCode::Down, KeyModifiers::NONE, false, false),
            Msg::MoveDown
        );
        assert_eq!(
            key_to_msg(KeyCode::Up, KeyModifiers::NONE, false, false),
            Msg::MoveUp
        );
    }

    #[test]
    fn test_key_to_msg_quit() {
        assert_eq!(
            key_to_msg(KeyCode::Char('q'), KeyModifiers::NONE, false, false),
            Msg::Quit
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('c'), KeyModifiers::CONTROL, false, false),
            Msg::Quit
        );
    }

    #[test]
    fn test_key_to_msg_search_mode() {
        assert_eq!(
            key_to_msg(KeyCode::Char('a'), KeyModifiers::NONE, false, true),
            Msg::SearchInput('a')
        );
        assert_eq!(
            key_to_msg(KeyCode::Enter, KeyModifiers::NONE, false, true),
            Msg::SearchConfirm
        );
        assert_eq!(
            key_to_msg(KeyCode::Esc, KeyModifiers::NONE, false, true),
            Msg::SearchCancel
        );
        assert_eq!(
            key_to_msg(KeyCode::Backspace, KeyModifiers::NONE, false, true),
            Msg::SearchBackspace
        );
    }

    #[test]
    fn test_key_to_msg_modal_mode() {
        assert_eq!(
            key_to_msg(KeyCode::Char('j'), KeyModifiers::NONE, true, false),
            Msg::ModalScrollDown
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('k'), KeyModifiers::NONE, true, false),
            Msg::ModalScrollUp
        );
        assert_eq!(
            key_to_msg(KeyCode::Esc, KeyModifiers::NONE, true, false),
            Msg::CloseModal
        );
    }

    #[test]
    fn test_is_quit() {
        assert!(is_quit(&Msg::Quit));
        assert!(!is_quit(&Msg::MoveDown));
    }

    #[test]
    fn test_is_navigation() {
        assert!(is_navigation(&Msg::MoveUp));
        assert!(is_navigation(&Msg::MoveDown));
        assert!(is_navigation(&Msg::PageUp));
        assert!(!is_navigation(&Msg::Quit));
        assert!(!is_navigation(&Msg::ToggleHelp));
    }

    #[test]
    fn test_is_filter_change() {
        assert!(is_filter_change(&Msg::CycleTypeFilter));
        assert!(is_filter_change(&Msg::CycleBranchFilter));
        assert!(!is_filter_change(&Msg::MoveUp));
    }

    #[test]
    fn test_key_to_msg_actions() {
        // Test action keys - these trigger side effects
        assert_eq!(
            key_to_msg(KeyCode::Char('o'), KeyModifiers::NONE, false, false),
            Msg::OpenFiles
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('r'), KeyModifiers::NONE, false, false),
            Msg::RefreshGraph
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('y'), KeyModifiers::NONE, false, false),
            Msg::CopyToClipboard
        );
    }

    #[test]
    fn test_key_to_msg_view_switching() {
        assert_eq!(
            key_to_msg(KeyCode::Tab, KeyModifiers::NONE, false, false),
            Msg::NextView
        );
        assert_eq!(
            key_to_msg(KeyCode::Tab, KeyModifiers::SHIFT, false, false),
            Msg::PrevView
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('1'), KeyModifiers::NONE, false, false),
            Msg::SwitchToView(ViewKind::Timeline)
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('2'), KeyModifiers::NONE, false, false),
            Msg::SwitchToView(ViewKind::Dag)
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('3'), KeyModifiers::NONE, false, false),
            Msg::SwitchToView(ViewKind::Graph)
        );
    }

    #[test]
    fn test_key_to_msg_filtering() {
        assert_eq!(
            key_to_msg(KeyCode::Char('t'), KeyModifiers::NONE, false, false),
            Msg::CycleTypeFilter
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('b'), KeyModifiers::NONE, false, false),
            Msg::CycleBranchFilter
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('B'), KeyModifiers::NONE, false, false),
            Msg::OpenBranchSearch
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('/'), KeyModifiers::NONE, false, false),
            Msg::OpenBranchSearch
        );
    }

    #[test]
    fn test_key_to_msg_modals() {
        assert_eq!(
            key_to_msg(KeyCode::Char('?'), KeyModifiers::NONE, false, false),
            Msg::ToggleHelp
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('P'), KeyModifiers::NONE, false, false),
            Msg::OpenPromptModal
        );
        assert_eq!(
            key_to_msg(KeyCode::Esc, KeyModifiers::NONE, false, false),
            Msg::CloseModal
        );
    }

    #[test]
    fn test_key_to_msg_file_browser() {
        assert_eq!(
            key_to_msg(KeyCode::Char('F'), KeyModifiers::NONE, false, false),
            Msg::ToggleFileBrowser
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('p'), KeyModifiers::NONE, false, false),
            Msg::PreviewFile
        );
    }

    #[test]
    fn test_key_to_msg_detail_panel() {
        assert_eq!(
            key_to_msg(KeyCode::Enter, KeyModifiers::NONE, false, false),
            Msg::ToggleDetailPanel
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('l'), KeyModifiers::NONE, false, false),
            Msg::DetailScrollDown
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('h'), KeyModifiers::NONE, false, false),
            Msg::DetailScrollUp
        );
    }

    #[test]
    fn test_key_to_msg_page_navigation() {
        assert_eq!(
            key_to_msg(KeyCode::Char('d'), KeyModifiers::CONTROL, false, false),
            Msg::PageDown
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('u'), KeyModifiers::CONTROL, false, false),
            Msg::PageUp
        );
        assert_eq!(
            key_to_msg(KeyCode::PageDown, KeyModifiers::NONE, false, false),
            Msg::PageDown
        );
        assert_eq!(
            key_to_msg(KeyCode::PageUp, KeyModifiers::NONE, false, false),
            Msg::PageUp
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('g'), KeyModifiers::NONE, false, false),
            Msg::JumpToTop
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('G'), KeyModifiers::NONE, false, false),
            Msg::JumpToBottom
        );
        assert_eq!(
            key_to_msg(KeyCode::Home, KeyModifiers::NONE, false, false),
            Msg::JumpToTop
        );
        assert_eq!(
            key_to_msg(KeyCode::End, KeyModifiers::NONE, false, false),
            Msg::JumpToBottom
        );
    }

    #[test]
    fn test_key_to_msg_goal_story() {
        assert_eq!(
            key_to_msg(KeyCode::Char('s'), KeyModifiers::NONE, false, false),
            Msg::ToggleGoalStory
        );
    }

    #[test]
    fn test_key_to_msg_unhandled() {
        // Keys that aren't mapped should return Noop
        assert_eq!(
            key_to_msg(KeyCode::Char('z'), KeyModifiers::NONE, false, false),
            Msg::Noop
        );
        assert_eq!(
            key_to_msg(KeyCode::Char('x'), KeyModifiers::NONE, false, false),
            Msg::Noop
        );
    }
}
