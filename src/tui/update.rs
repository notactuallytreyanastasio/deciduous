//! TEA Update Function
//!
//! This module contains the pure update function that processes messages.
//! Following The Elm Architecture (TEA):
//!
//! ```text
//! update : Msg -> Model -> (Model, Cmd)
//! ```
//!
//! The update function:
//! - Takes a message and current model
//! - Returns new model and optional side-effect commands
//! - Is a pure function (no I/O, no mutation)
//! - Can be easily tested in isolation
//!
//! Side effects (I/O) are represented as Commands that the runtime executes.

use super::msg::{Msg, ViewKind};
use super::state;

/// Commands that need to be executed by the runtime (imperative shell)
#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    /// No command
    None,
    /// Multiple commands to execute
    Batch(Vec<Cmd>),
    /// Quit the application
    Quit,
    /// Reload graph from database
    ReloadGraph,
    /// Open files in external editor
    OpenFiles(Vec<String>),
    /// Set status message
    SetStatus(String),
    /// Copy text to clipboard
    CopyToClipboard(String),
}

impl Cmd {
    /// Create a batch of commands
    pub fn batch(cmds: Vec<Cmd>) -> Cmd {
        // Filter out None commands
        let mut cmds: Vec<Cmd> = cmds
            .into_iter()
            .filter(|c| !matches!(c, Cmd::None))
            .collect();
        match cmds.len() {
            0 => Cmd::None,
            1 => cmds.pop().unwrap_or(Cmd::None), // Safe: len == 1 guarantees element
            _ => Cmd::Batch(cmds),
        }
    }

    /// Check if this is a quit command
    pub fn is_quit(&self) -> bool {
        matches!(self, Cmd::Quit)
    }
}

/// Minimal model state for pure update function testing
/// This mirrors the essential state from App without I/O dependencies
#[derive(Debug, Clone)]
pub struct Model {
    // View state
    pub current_view: ViewKind,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub item_count: usize,
    pub visible_items: usize,

    // Filters
    pub type_filter: Option<String>,
    pub branch_filter: Option<String>,
    pub search_query: String,
    pub available_branches: Vec<String>,

    // Modals
    pub help_open: bool,
    pub prompt_modal_open: bool,
    pub branch_search_open: bool,
    pub modal_scroll: usize,

    // Detail panel
    pub detail_panel_visible: bool,
    pub detail_scroll: usize,

    // File browser
    pub file_browser_open: bool,
    pub file_browser_index: usize,

    // Goal story
    pub goal_story_open: bool,

    // Misc
    pub status_message: Option<String>,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            current_view: ViewKind::Timeline,
            selected_index: 0,
            scroll_offset: 0,
            item_count: 0,
            visible_items: 20,
            type_filter: None,
            branch_filter: None,
            search_query: String::new(),
            available_branches: Vec::new(),
            help_open: false,
            prompt_modal_open: false,
            branch_search_open: false,
            modal_scroll: 0,
            detail_panel_visible: true,
            detail_scroll: 0,
            file_browser_open: false,
            file_browser_index: 0,
            goal_story_open: false,
            status_message: None,
        }
    }
}

/// The core update function - processes a message and returns new state + commands
///
/// This is a PURE FUNCTION:
/// - No I/O
/// - No mutation (returns new model)
/// - Deterministic (same inputs = same outputs)
/// - Easy to test
pub fn update(msg: Msg, model: Model) -> (Model, Cmd) {
    match msg {
        // === Lifecycle ===
        Msg::Quit => (model, Cmd::Quit),

        Msg::Tick => (model, Cmd::None),

        Msg::Resize(_, _) => (model, Cmd::None), // UI handles this

        Msg::Noop => (model, Cmd::None),

        // === Navigation ===
        Msg::MoveUp => {
            let new_selected = state::move_selection_up(model.selected_index);
            let new_scroll = state::calculate_scroll_offset(
                new_selected,
                model.scroll_offset,
                model.visible_items,
            );
            (
                Model {
                    selected_index: new_selected,
                    scroll_offset: new_scroll,
                    ..model
                },
                Cmd::None,
            )
        }

        Msg::MoveDown => {
            let new_selected = state::move_selection_down(model.selected_index, model.item_count);
            let new_scroll = state::calculate_scroll_offset(
                new_selected,
                model.scroll_offset,
                model.visible_items,
            );
            (
                Model {
                    selected_index: new_selected,
                    scroll_offset: new_scroll,
                    ..model
                },
                Cmd::None,
            )
        }

        Msg::PageUp => {
            let new_selected = state::page_up(model.selected_index, model.visible_items);
            let new_scroll = state::calculate_scroll_offset(
                new_selected,
                model.scroll_offset,
                model.visible_items,
            );
            (
                Model {
                    selected_index: new_selected,
                    scroll_offset: new_scroll,
                    ..model
                },
                Cmd::None,
            )
        }

        Msg::PageDown => {
            let new_selected =
                state::page_down(model.selected_index, model.visible_items, model.item_count);
            let new_scroll = state::calculate_scroll_offset(
                new_selected,
                model.scroll_offset,
                model.visible_items,
            );
            (
                Model {
                    selected_index: new_selected,
                    scroll_offset: new_scroll,
                    ..model
                },
                Cmd::None,
            )
        }

        Msg::JumpToTop => (
            Model {
                selected_index: 0,
                scroll_offset: 0,
                ..model
            },
            Cmd::None,
        ),

        Msg::JumpToBottom => {
            let new_selected = if model.item_count > 0 {
                model.item_count - 1
            } else {
                0
            };
            let new_scroll = state::calculate_scroll_offset(
                new_selected,
                model.scroll_offset,
                model.visible_items,
            );
            (
                Model {
                    selected_index: new_selected,
                    scroll_offset: new_scroll,
                    ..model
                },
                Cmd::None,
            )
        }

        Msg::SelectIndex(idx) => {
            let clamped = state::clamp_selection(idx, model.item_count);
            (
                Model {
                    selected_index: clamped,
                    ..model
                },
                Cmd::None,
            )
        }

        // === View Switching ===
        Msg::NextView => (
            Model {
                current_view: model.current_view.next(),
                selected_index: 0,
                scroll_offset: 0,
                ..model
            },
            Cmd::None,
        ),

        Msg::PrevView => (
            Model {
                current_view: model.current_view.prev(),
                selected_index: 0,
                scroll_offset: 0,
                ..model
            },
            Cmd::None,
        ),

        Msg::SwitchToView(view) => (
            Model {
                current_view: view,
                selected_index: 0,
                scroll_offset: 0,
                ..model
            },
            Cmd::None,
        ),

        // === Filtering ===
        Msg::CycleTypeFilter => {
            let new_filter = state::cycle_type_filter(model.type_filter.as_deref());
            (
                Model {
                    type_filter: new_filter,
                    selected_index: 0,
                    scroll_offset: 0,
                    ..model
                },
                Cmd::None,
            )
        }

        Msg::CycleBranchFilter => {
            let new_filter = state::cycle_branch_filter(
                model.branch_filter.as_deref(),
                &model.available_branches,
            );
            (
                Model {
                    branch_filter: new_filter,
                    selected_index: 0,
                    scroll_offset: 0,
                    ..model
                },
                Cmd::None,
            )
        }

        Msg::OpenBranchSearch => (
            Model {
                branch_search_open: true,
                search_query: String::new(),
                ..model
            },
            Cmd::None,
        ),

        Msg::SetSearchQuery(query) => (
            Model {
                search_query: query,
                selected_index: 0,
                scroll_offset: 0,
                ..model
            },
            Cmd::None,
        ),

        Msg::ClearFilters => (
            Model {
                type_filter: None,
                branch_filter: None,
                search_query: String::new(),
                selected_index: 0,
                scroll_offset: 0,
                ..model
            },
            Cmd::None,
        ),

        // === Search Modal ===
        Msg::SearchInput(c) => {
            let mut new_query = model.search_query.clone();
            new_query.push(c);
            (
                Model {
                    search_query: new_query,
                    ..model
                },
                Cmd::None,
            )
        }

        Msg::SearchBackspace => {
            let mut new_query = model.search_query.clone();
            new_query.pop();
            (
                Model {
                    search_query: new_query,
                    ..model
                },
                Cmd::None,
            )
        }

        Msg::SearchConfirm => (
            Model {
                branch_search_open: false,
                selected_index: 0,
                scroll_offset: 0,
                ..model
            },
            Cmd::None,
        ),

        Msg::SearchCancel => (
            Model {
                branch_search_open: false,
                search_query: String::new(),
                ..model
            },
            Cmd::None,
        ),

        // === Detail Panel ===
        Msg::ToggleDetailPanel => (
            Model {
                detail_panel_visible: !model.detail_panel_visible,
                ..model
            },
            Cmd::None,
        ),

        Msg::DetailScrollUp => (
            Model {
                detail_scroll: model.detail_scroll.saturating_sub(1),
                ..model
            },
            Cmd::None,
        ),

        Msg::DetailScrollDown => (
            Model {
                detail_scroll: model.detail_scroll + 1,
                ..model
            },
            Cmd::None,
        ),

        // === Modals ===
        Msg::ToggleHelp => (
            Model {
                help_open: !model.help_open,
                modal_scroll: 0,
                ..model
            },
            Cmd::None,
        ),

        Msg::OpenPromptModal => (
            Model {
                prompt_modal_open: true,
                modal_scroll: 0,
                ..model
            },
            Cmd::None,
        ),

        Msg::CloseModal => (
            Model {
                help_open: false,
                prompt_modal_open: false,
                branch_search_open: false,
                modal_scroll: 0,
                ..model
            },
            Cmd::None,
        ),

        Msg::ModalScrollUp => (
            Model {
                modal_scroll: model.modal_scroll.saturating_sub(3),
                ..model
            },
            Cmd::None,
        ),

        Msg::ModalScrollDown => (
            Model {
                modal_scroll: model.modal_scroll + 3,
                ..model
            },
            Cmd::None,
        ),

        // === File Browser ===
        Msg::ToggleFileBrowser => (
            Model {
                file_browser_open: !model.file_browser_open,
                file_browser_index: 0,
                ..model
            },
            Cmd::None,
        ),

        Msg::FileBrowserEnter | Msg::FileBrowserBack | Msg::FileBrowserToggle => {
            // These need access to file system state - handled by imperative shell
            (model, Cmd::None)
        }

        Msg::PreviewFile | Msg::ShowFileDiff => {
            // These need file content - handled by imperative shell
            (model, Cmd::None)
        }

        // === Goal Story ===
        Msg::ToggleGoalStory => (
            Model {
                goal_story_open: !model.goal_story_open,
                ..model
            },
            Cmd::None,
        ),

        Msg::GoalStoryToggle => {
            // Needs tree state - handled by imperative shell
            (model, Cmd::None)
        }

        // === Actions ===
        Msg::OpenFiles => {
            // The actual file list comes from the selected node - imperative shell handles this
            (model, Cmd::SetStatus("Opening files...".to_string()))
        }

        Msg::RefreshGraph => (model, Cmd::ReloadGraph),

        Msg::CopyToClipboard => {
            // The actual content comes from selected node - imperative shell handles this
            (model, Cmd::SetStatus("Copied to clipboard".to_string()))
        }

        // === Mouse ===
        Msg::Mouse(_) => {
            // Mouse events need screen coordinates - handled by imperative shell
            (model, Cmd::None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_with_items(count: usize) -> Model {
        Model {
            item_count: count,
            visible_items: 10,
            ..Default::default()
        }
    }

    // === Navigation Tests ===

    #[test]
    fn test_move_down() {
        let model = model_with_items(5);
        let (new_model, cmd) = update(Msg::MoveDown, model);
        assert_eq!(new_model.selected_index, 1);
        assert_eq!(cmd, Cmd::None);
    }

    #[test]
    fn test_move_down_at_end() {
        let mut model = model_with_items(5);
        model.selected_index = 4;
        let (new_model, _) = update(Msg::MoveDown, model);
        assert_eq!(new_model.selected_index, 4); // Stays at end
    }

    #[test]
    fn test_move_up() {
        let mut model = model_with_items(5);
        model.selected_index = 2;
        let (new_model, cmd) = update(Msg::MoveUp, model);
        assert_eq!(new_model.selected_index, 1);
        assert_eq!(cmd, Cmd::None);
    }

    #[test]
    fn test_move_up_at_start() {
        let model = model_with_items(5);
        let (new_model, _) = update(Msg::MoveUp, model);
        assert_eq!(new_model.selected_index, 0); // Stays at start
    }

    #[test]
    fn test_jump_to_top() {
        let mut model = model_with_items(100);
        model.selected_index = 50;
        model.scroll_offset = 40;
        let (new_model, _) = update(Msg::JumpToTop, model);
        assert_eq!(new_model.selected_index, 0);
        assert_eq!(new_model.scroll_offset, 0);
    }

    #[test]
    fn test_jump_to_bottom() {
        let model = model_with_items(100);
        let (new_model, _) = update(Msg::JumpToBottom, model);
        assert_eq!(new_model.selected_index, 99);
    }

    // === View Switching Tests ===

    #[test]
    fn test_next_view() {
        let model = Model::default();
        assert_eq!(model.current_view, ViewKind::Timeline);

        let (new_model, _) = update(Msg::NextView, model);
        assert_eq!(new_model.current_view, ViewKind::Dag);
        assert_eq!(new_model.selected_index, 0); // Reset on view change
    }

    #[test]
    fn test_switch_to_view() {
        let model = Model::default();
        let (new_model, _) = update(Msg::SwitchToView(ViewKind::Graph), model);
        assert_eq!(new_model.current_view, ViewKind::Graph);
    }

    // === Filter Tests ===

    #[test]
    fn test_cycle_type_filter() {
        let model = Model::default();
        assert_eq!(model.type_filter, None);

        let (new_model, _) = update(Msg::CycleTypeFilter, model);
        assert_eq!(new_model.type_filter, Some("goal".to_string()));
        assert_eq!(new_model.selected_index, 0); // Reset on filter change
    }

    #[test]
    fn test_clear_filters() {
        let model = Model {
            type_filter: Some("goal".to_string()),
            branch_filter: Some("main".to_string()),
            search_query: "test".to_string(),
            selected_index: 5,
            ..Default::default()
        };
        let (new_model, _) = update(Msg::ClearFilters, model);
        assert_eq!(new_model.type_filter, None);
        assert_eq!(new_model.branch_filter, None);
        assert_eq!(new_model.search_query, "");
        assert_eq!(new_model.selected_index, 0);
    }

    // === Modal Tests ===

    #[test]
    fn test_toggle_help() {
        let model = Model::default();
        assert!(!model.help_open);

        let (new_model, _) = update(Msg::ToggleHelp, model);
        assert!(new_model.help_open);
        assert_eq!(new_model.modal_scroll, 0);

        let (new_model, _) = update(Msg::ToggleHelp, new_model);
        assert!(!new_model.help_open);
    }

    #[test]
    fn test_close_modal() {
        let model = Model {
            help_open: true,
            prompt_modal_open: true,
            branch_search_open: true,
            modal_scroll: 10,
            ..Default::default()
        };
        let (new_model, _) = update(Msg::CloseModal, model);
        assert!(!new_model.help_open);
        assert!(!new_model.prompt_modal_open);
        assert!(!new_model.branch_search_open);
        assert_eq!(new_model.modal_scroll, 0);
    }

    // === Search Tests ===

    #[test]
    fn test_search_input() {
        let model = Model::default();
        let (m1, _) = update(Msg::SearchInput('h'), model);
        let (m2, _) = update(Msg::SearchInput('i'), m1);
        assert_eq!(m2.search_query, "hi");
    }

    #[test]
    fn test_search_backspace() {
        let model = Model {
            search_query: "hello".to_string(),
            ..Default::default()
        };
        let (new_model, _) = update(Msg::SearchBackspace, model);
        assert_eq!(new_model.search_query, "hell");
    }

    // === Command Tests ===

    #[test]
    fn test_quit_command() {
        let model = Model::default();
        let (_, cmd) = update(Msg::Quit, model);
        assert!(cmd.is_quit());
    }

    #[test]
    fn test_refresh_command() {
        let model = Model::default();
        let (_, cmd) = update(Msg::RefreshGraph, model);
        assert_eq!(cmd, Cmd::ReloadGraph);
    }

    // === Cmd::batch Tests ===

    #[test]
    fn test_cmd_batch_empty() {
        assert_eq!(Cmd::batch(vec![]), Cmd::None);
    }

    #[test]
    fn test_cmd_batch_single() {
        assert_eq!(Cmd::batch(vec![Cmd::Quit]), Cmd::Quit);
    }

    #[test]
    fn test_cmd_batch_filters_none() {
        let result = Cmd::batch(vec![Cmd::None, Cmd::Quit, Cmd::None]);
        assert_eq!(result, Cmd::Quit);
    }
}
