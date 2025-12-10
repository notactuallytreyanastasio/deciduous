# Deciduous Roadmap

## Backlog

### Clean up legacy "losselot" references
- [ ] Update `.claude/commands/decision.md` to use `deciduous` binary instead of `./target/release/losselot`
- [ ] Update `.claude/commands/context.md` to use `deciduous` binary instead of `./target/release/losselot`
- [ ] Update `CLAUDE.md` to remove any losselot-specific references
- [ ] Ensure templates in `src/init.rs` use installed binary (`deciduous`) rather than build paths
- [ ] Update live graph URLs to point to correct project

### Future Enhancements
- [ ] Support for additional editors (Cursor, Copilot, etc.)
- [ ] `deciduous init --all` to bootstrap for all supported editors at once

### Commit-Centric Graph Navigation
- [ ] More robust per-commit tooling
  - Link nodes to commits more reliably
  - Auto-detect commit context when logging actions/outcomes
  - `deciduous add action "..." --commit HEAD` should be the default flow
- [ ] Commit-centric browsing in the web viewer
  - Browse graph starting from commits (not just nodes)
  - Show which nodes are associated with each commit
  - Timeline view organized by commit history
  - Click a commit → see the decision subgraph around it
- [ ] `deciduous log` command to show commits with their linked nodes
- [ ] Integration with `git log` to annotate commits with decision context

### TUI Graph Viewer
- [ ] `deciduous tui` command for terminal-based graph visualization
- [ ] **Phase 1: Per-goal view**
  - Show single goal's decision tree/flow
  - `deciduous tui --goal 128` to focus on one goal chain
  - Visualize: goal → decisions → options (chosen/rejected) → actions → outcomes
- [ ] **Phase 2: Multi-goal chains**
  - Navigate between related goals
  - Show how goals connect and depend on each other

**Display challenges & ideas:**
- Nodes have lots of text - need compact representations
- Options to explore:
  - **Collapsible nodes**: Show title only, expand on select for description/metadata
  - **Truncation with preview**: `[goal] Add editor-spec...` → full text in side panel
  - **Fish-eye/focus+context**: Selected node full size, neighbors shrink
  - **Breadcrumb trail**: Show path to current node, collapse siblings
  - **Split view**: Tree on left, detail panel on right (like ranger/nnn)
  - **Vim-style navigation**: j/k to move, Enter to expand, q to quit

**Rust TUI libraries to consider:**
- `ratatui` (successor to tui-rs) - most popular, very flexible
- `cursive` - higher-level, dialog-based
- `crossterm` - low-level backend (works with ratatui)
- `tui-tree-widget` - tree display component for ratatui
- `tui-textarea` - for any input needs

**Inspiration:**
- `lazygit` - great example of complex TUI with panels
- `gitui` - another git TUI with tree views
- `diskonaut` - treemap visualization in terminal
- `btm`/`bottom` - system monitor with good layout patterns
