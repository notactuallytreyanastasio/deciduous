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

### LLM Critique & Analysis
- [ ] `deciduous critique --goal <id>` - Have an LLM analyze a goal's decision chain
  - Review decisions made, options chosen/rejected
  - Identify potential blind spots or unconsidered alternatives
  - Evaluate confidence levels vs actual outcomes
  - Suggest improvements for future similar decisions
- [ ] Multi-model critique comparison
  - `deciduous critique --goal <id> --models claude,gpt4,gemini`
  - See how different models evaluate the same decision chain
  - Highlight where models agree/disagree on quality
- [ ] Critique storage - save critiques as special nodes linked to goals

### LLM Benchmarking Framework
- [ ] **Goal-based benchmarking**: Use the same goal/task across multiple LLMs
  - Define a goal with acceptance criteria
  - Run each model on the same task
  - Compare: decisions made, paths taken, outcomes achieved
- [ ] `deciduous benchmark` command
  - `deciduous benchmark --task "Implement feature X" --models claude,gpt4,gemini,llama`
  - Each model gets isolated graph namespace
  - Automated or human evaluation of outcomes
- [ ] Metrics to capture:
  - Decision quality (did they consider good options?)
  - Path efficiency (how direct was the route to outcome?)
  - Confidence calibration (were high-confidence decisions correct?)
  - Recovery ability (how did they handle setbacks?)
  - Graph structure (complexity, dead ends, backtracking)
- [ ] Benchmark reports
  - Side-by-side comparison of decision graphs
  - Aggregate stats across multiple benchmark runs
  - Export to shareable format for publishing results
- [ ] Reproducible benchmark suites
  - Define standard tasks with known-good solutions
  - Version-controlled benchmark definitions
  - CI integration for regression testing model capabilities

### Code Tracking & File Associations
- [ ] Associate nodes with code changes
  - `deciduous add action "Implementing X" --files src/foo.rs,src/bar.rs`
  - Track which files were touched for each action/outcome
  - Store file paths and optionally line ranges
- [ ] **Web UI: "View Code" button on nodes**
  - Click a node → see associated files
  - Show git diff for the commit linked to that node
  - Quick navigation to file locations
- [ ] Update `.claude/commands/` and `.windsurf/rules/` templates
  - Include instructions to log file associations
  - `deciduous add action "..." --files <changed-files>`
- [ ] `deciduous files <node-id>` command to list associated files
- [ ] Reverse lookup: `deciduous nodes --file src/foo.rs` to find nodes touching a file

### Prompt Tracking
- [ ] Capture prompts alongside decisions
  - Store the exact user prompt that triggered a goal/decision
  - Link prompts to their resulting decision chains
- [ ] `deciduous add goal "Title" --prompt "User's original request"`
- [ ] Prompt history view in web UI
  - See what prompt led to each decision chain
  - Search/filter by prompt content
- [ ] Prompt templates
  - Save effective prompts for reuse
  - Share prompt patterns that lead to good decision trees
- [ ] Prompt → Outcome analysis
  - Correlate prompt patterns with successful outcomes
  - Identify which prompt styles lead to better decisions
- [ ] **View prompts in web UI**
  - Display the original prompt in node detail panel
  - Filter/search nodes by prompt content
  - "Copy prompt" button for reuse

### Git Integration & Pre-commit Hook Awareness
- [ ] Inspect and respect pre-commit hooks
  - Detect `.git/hooks/pre-commit` or `.husky/` hooks
  - Parse hook contents to understand what checks run
  - Warn users if hooks might reject commits (linting, formatting, tests)
- [ ] Pre-flight commit validation
  - `deciduous commit --dry-run` to simulate what hooks would do
  - Show which checks would pass/fail before actual commit
- [ ] Auto-fix integration
  - If hooks run formatters (prettier, rustfmt), detect and apply fixes
  - Re-stage auto-fixed files before commit
- [ ] Hook-aware templates
  - Update `.claude/commands/` and `.windsurf/rules/` to mention pre-commit awareness
  - Instruct LLMs to check for hooks before committing
