# Deciduous Roadmap

## Completed

### v0.6.0 - Multi-User Graph Sync (December 2024)
- [x] jj-inspired dual-ID model with `change_id` (UUID) for globally unique nodes
- [x] `deciduous diff export` - export nodes as shareable JSON patches
- [x] `deciduous diff apply` - apply patches from teammates (idempotent)
- [x] `deciduous diff status` - list available patches
- [x] `deciduous migrate` - add change_id columns for sync
- [x] Auto-migration on database open
- [x] Bootstrapped templates include multi-user sync documentation

### Legacy Cleanup (Complete)
- [x] All templates use `deciduous` binary (no more `losselot` references)
- [x] Live graph URLs point to correct project
- [x] `src/init.rs` uses installed binary path

---

## Backlog

### Future Enhancements
- [ ] Support for additional editors (Cursor, Copilot, etc.)
- [ ] `deciduous init --all` to bootstrap for all supported editors at once

### Context Recovery (Critical)
- [ ] **Make compaction restore actually reliably work**
  - `/context` command should fully restore working state after context loss
  - Query decision graph for recent goals, decisions, actions in progress
  - Show what was being worked on, what's complete, what's pending
  - Include recent git activity, uncommitted changes, branch state
  - Pull in relevant prompts from history if available
  - Goal: new session can pick up exactly where the last one left off

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
- [x] `deciduous tui` command for terminal-based graph visualization
- [x] Timeline view with vim-style navigation (j/k/gg/G)
- [x] Detail panel with node info, connections, files
- [x] File browser mode (F to toggle, n/N to navigate)
- [x] File preview with syntax highlighting (p key)
- [x] File diff viewer with syntax + diff coloring (d key)
- [x] Commit modal with split view (O key)
  - Top section: commit hash, node, files, message
  - Bottom section: scrollable diff with j/k navigation
- [x] Branch filtering (b to cycle, B for fuzzy search)
- [x] Timeline order toggle (R for reverse chronological)
- [x] Search (/ to filter by title/description)
- [x] Goal story view (s to show goal hierarchy)
- [ ] **Phase 2: Multi-goal chains**
  - Navigate between related goals
  - Show how goals connect and depend on each other
- [ ] DAG view improvements (currently disabled)
  - Better hierarchical layout algorithm
  - Zoom and pan controls

### Roadmap Manager Tool
- [ ] **Link roadmap items to work**
  - Roadmap items should link to decision graph nodes
  - Track progress through the graph (goal -> decisions -> outcomes)
  - Show completion status based on linked outcomes
- [ ] **Work item integration** (future)
  - Design with external ticketing in mind (JIRA, Linear, GitHub Issues)
  - Roadmap items could link to external tickets
  - Sync status bidirectionally
  - Keep this extensible for enterprise workflows
- [ ] **CLI commands**
  - `deciduous roadmap list` - show roadmap items and their status
  - `deciduous roadmap link <item> <node-id>` - link to graph node
  - `deciduous roadmap status` - show completion based on linked nodes

### Release Tooling
- [ ] **Automated release notes generation**
  - `deciduous release-notes --from v0.7.0 --to v0.8.0`
  - Query graph for all goals/outcomes between versions
  - Group by feature area (TUI, sync, export, etc.)
  - Generate markdown summary of what changed and why
- [ ] **PR tagging for releases**
  - Tag PRs that landed in a release
  - `deciduous release tag-prs v0.8.0` - adds label to all PRs in release
  - Link PRs back to decision graph nodes
- [ ] **Release summary from decision graph**
  - Walk the graph to find all completed goals since last release
  - Show the decision chain that led to each feature
  - Include key prompts/feedback that drove changes
  - Generate changelog with decision context

### TUI PR Review Mode
- [ ] **GitHub PR integration in TUI**
  - Pull in PR comments from GitHub API
  - Show file-level and line-level comments alongside code
  - Browse commits in PR context with associated comments
  - Mark comments as resolved/addressed from TUI
  - Reply to comments directly from TUI
- [ ] **Code review workflow**
  - Navigate between commented locations
  - Jump from decision node to related PR/commit comments
  - See review status and approval state

### Type Unification (TUI + Web)
- [ ] **Shared type definitions**
  - Unify TUI types (src/tui/types.rs) with web types (web/src/types/graph.ts)
  - Single source of truth for node/edge structures
  - Consider code generation or shared schema
- [ ] **Port TUI features to web viewer**
  - Timeline view with vim-style navigation
  - Commit modal with split-view diff
  - Branch filtering and fuzzy search
  - Goal story view
- [ ] **Parallel development workflow**
  - Changes to one should auto-update the other
  - Shared test fixtures for both platforms
  - Document the type mapping

### TUI Architecture Refactor
- [ ] **Functional core, imperative shell**
  - Extract pure functions from app.rs for all state transformations
  - Move all I/O to thin imperative shell at edges
  - State transitions should be pure: `fn update(state: App, event: Event) -> App`
- [ ] **Comprehensive test coverage**
  - Unit tests for all pure state transformation functions
  - Test navigation logic without terminal
  - Test modal state machines
  - Test filtering and search logic
  - Property-based tests for state invariants
- [ ] **TEA pattern enforcement**
  - Strict Model/Update/View separation
  - No side effects in view functions (already done, verify)
  - Event handlers return new state, don't mutate
  - Extract reusable update functions

### TUI UX Polish
- [ ] **Keyboard shortcut audit and redesign**
  - Analyze all current shortcuts for intuitiveness
  - Ensure shortcuts are discoverable and memorable
  - Consider user expectations from similar tools (vim, lazygit, ranger)
  - Group related actions with similar key patterns
- [ ] **Visual discoverability**
  - Add context-sensitive help hints in footer
  - Show available actions for current context
  - Highlight keyboard shortcuts in help overlay
  - Consider modal indicator showing current mode prominently
- [ ] **Onboarding experience**
  - First-run tutorial or guided tour
  - Progressive disclosure of advanced features
  - Cheat sheet generation (`deciduous tui --help-keys`)
- [ ] **Settings system**
  - `.deciduous/config.toml` for user preferences
  - Timeline order default (newest-first vs oldest-first)
  - Editor preference (`$EDITOR` fallback chain)
  - Color theme selection
  - Key binding customization
  - Database path configuration

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

### Claude Code Hooks Integration
- [ ] Explore using Claude Code hooks to guarantee workflow behavior
  - Claude Code supports hooks that run on various events (tool calls, messages, etc.)
  - Could enforce decision graph logging more reliably than instructions alone
- [ ] **Pre-tool hooks**: Before file edits
  - Auto-log `action` node when Claude is about to modify files
  - Capture which files are being changed
  - Block edits if no active goal exists in the graph
- [ ] **Post-tool hooks**: After successful operations
  - Auto-log `outcome` nodes after code changes succeed
  - Auto-link outcomes to their parent actions
  - Trigger `deciduous sync` after significant changes
- [ ] **Pre-commit hooks**: Before git commits
  - Verify graph integrity (no orphan outcomes/actions)
  - Require at least one goal node for the current session
  - Auto-add commit hash to recent action/outcome nodes
- [ ] **Session start hooks**: On conversation begin
  - Auto-run `/context` equivalent
  - Load graph state and surface pending decisions
  - Warn if there are disconnected nodes needing attention
- [ ] **Prompt submit hooks**: When user sends a message
  - Detect feature requests and auto-create goal nodes
  - Capture original prompt in node metadata
- [ ] Hook configuration in `.deciduous/hooks.toml`
  - Enable/disable specific hooks
  - Configure strictness (warn vs block)
  - Custom hook scripts for project-specific needs
- [ ] Document hook setup in Claude Code settings
  - Integration guide for `.claude/settings.json`
  - Example hook configurations for different workflow styles

### Git.log File Reliability
- [ ] Investigate why git.log file isn't always being created/updated
  - User reported missing git.log during session
  - This file should always exist and track all git commands run
- [ ] Ensure git.log is created on `deciduous init`
- [ ] Add git.log writing to all git-related operations
- [ ] Consider moving git.log into `.deciduous/` directory for better organization
- [ ] Add `deciduous log` command to view git.log contents
- [ ] Document git.log purpose and location in tooling files

### DuckDB for OLAP Analytics
- [ ] Add DuckDB as optional analytical backend for decision graph queries
  - SQLite is great for OLTP (single-project, real-time logging)
  - DuckDB excels at OLAP (cross-project analytics, time-series queries, aggregations)
- [ ] Use cases for analytical queries:
  - **Cross-project patterns**: "What decision patterns lead to successful outcomes across all my projects?"
  - **Time-series analysis**: "How has my decision-making evolved over the past 6 months?"
  - **Confidence calibration**: "Are my high-confidence decisions actually more successful?"
  - **Path analysis**: "What's the average depth of decision trees that lead to good outcomes?"
  - **Bottleneck detection**: "Which decision types take longest to resolve?"
- [ ] Implementation ideas:
  - Export SQLite graphs to Parquet files for DuckDB ingestion
  - `deciduous export --parquet` for analytical snapshots
  - `deciduous analytics` subcommand for running OLAP queries
  - Optional: federated queries across multiple project databases
- [ ] Potential analytical views:
  - Decision funnel analysis (goal → decision → action → outcome conversion)
  - Confidence vs outcome correlation matrix
  - Branch/feature complexity metrics
  - Session productivity heatmaps
  - Node type distribution over time
- [ ] Visualization integration:
  - Export to formats compatible with BI tools (Metabase, Superset, etc.)
  - Built-in charts in TUI or web viewer
  - `deciduous report` to generate analytical summaries

### Documentation Restructure
- [ ] Rethink the `docs/` folder organization
  - Currently: GitHub Pages viewer lives here
  - Problem: Also contains ad-hoc design docs (like MULTI_USER_SYNC.md)
  - Consider: Separate `docs/` (user-facing) from `design/` (internal design docs)
  - Consider: Move viewer to dedicated folder
- [ ] Consolidate documentation
  - Single source of truth in CLAUDE.md (the canonical reference)
  - .claude/commands/ and .windsurf/rules/ derive from CLAUDE.md patterns
  - README.md stays user-facing (installation, quick start)
- [ ] Auto-generate tooling docs
  - `deciduous docs` command to output markdown documentation
  - Include all commands, options, examples
  - Keep README.md and tooling files in sync automatically
