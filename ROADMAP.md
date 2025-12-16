# Deciduous Roadmap

## Completed
<!-- roadmap:section id="62dccf3f-c4ce-4359-bfab-5098f91d06bf" -->

### v0.8.x - Prompt Tracking & Template Sync (December 2024)
<!-- roadmap:section id="75e906b8-d00a-4d6a-8c9c-a3745cd272e8" -->
- [x] `--prompt` flag for capturing user prompts on nodes
  <!-- roadmap:item id="49a306e7-3ff1-4247-963a-a8b31990ae23" outcome_change_id="" -->
- [x] Prompt display in TUI detail panel with word-wrap
  <!-- roadmap:item id="95b65b2c-6d2e-43f5-a54b-e736c62d8c80" outcome_change_id="" -->
- [x] Template sync fix - init.rs templates match actual command files
  <!-- roadmap:item id="bd84fad1-0558-4d1a-a4cc-248e0fbeda9f" outcome_change_id="" -->
- [x] `deciduous update` no longer destroys custom content
  <!-- roadmap:item id="989c5423-f24d-4c0b-a2b8-958047b7a956" outcome_change_id="" -->

### v0.7.x - TUI Graph Viewer (December 2024)
<!-- roadmap:section id="8f3cbcbb-9718-47c5-8fe3-15df64909782" -->
- [x] `deciduous tui` command for terminal-based graph visualization
  <!-- roadmap:item id="9dcc20df-7192-4d9e-9633-eee2a477e1d1" outcome_change_id="" -->
- [x] Timeline view with vim-style navigation (j/k/gg/G)
  <!-- roadmap:item id="fcf17d38-eac8-4678-a976-f5e893c8bc28" outcome_change_id="" -->
- [x] Detail panel with node info, connections, files
  <!-- roadmap:item id="af512893-4564-49d0-bb36-b0900a598c32" outcome_change_id="" -->
- [x] File browser mode (F to toggle, n/N to navigate)
  <!-- roadmap:item id="36a7f8aa-379e-4d25-b573-920fc8cbb1c5" outcome_change_id="" -->
- [x] File preview with syntax highlighting (p key)
  <!-- roadmap:item id="5f6ecb8b-2c44-455c-84ef-32360584535a" outcome_change_id="" -->
- [x] File diff viewer with syntax + diff coloring (d key)
  <!-- roadmap:item id="d3981cc9-6835-4800-a29d-42252e9f8ae5" outcome_change_id="" -->
- [x] Commit modal with split view (O key)
  <!-- roadmap:item id="9c7b4b60-70cb-4486-9c4a-98594e0339aa" outcome_change_id="" -->
- [x] Branch filtering (b to cycle, B for fuzzy search)
  <!-- roadmap:item id="87e638f6-d9da-4fb0-bee0-58a800b104c5" outcome_change_id="" -->
- [x] Timeline order toggle (R for reverse chronological)
  <!-- roadmap:item id="1491241c-ea4d-4f87-ab29-dedcaa09df9b" outcome_change_id="" -->
- [x] Search (/ to filter by title/description)
  <!-- roadmap:item id="947350b1-c333-4185-a0e4-61dc8a25c1ef" outcome_change_id="" -->
- [x] Goal story view (s to show goal hierarchy)
  <!-- roadmap:item id="eef35aee-317f-45db-933b-518687db2e3d" outcome_change_id="" -->

### v0.6.0 - Multi-User Graph Sync (December 2024)
<!-- roadmap:section id="6513a80e-81f6-4ef2-9397-ea31a55fce2c" -->
- [x] jj-inspired dual-ID model with `change_id` (UUID) for globally unique nodes
  <!-- roadmap:item id="841eea6a-e2ba-43d6-83de-dc10c54a0210" outcome_change_id="" -->
- [x] `deciduous diff export` - export nodes as shareable JSON patches
  <!-- roadmap:item id="db57f760-4f4c-4dcb-8e04-12b156a022ed" outcome_change_id="" -->
- [x] `deciduous diff apply` - apply patches from teammates (idempotent)
  <!-- roadmap:item id="84ba22bf-d7ed-4b34-908a-c63268f48c69" outcome_change_id="" -->
- [x] `deciduous diff status` - list available patches
  <!-- roadmap:item id="0916da3d-e102-4fc4-ac23-2e84f37c46f1" outcome_change_id="" -->
- [x] `deciduous migrate` - add change_id columns for sync
  <!-- roadmap:item id="61d4ebea-bc8e-4b8b-8b44-c2f594f59361" outcome_change_id="" -->
- [x] Auto-migration on database open
  <!-- roadmap:item id="63ba4a20-bb00-4b8f-88c7-67e83cb32b9b" outcome_change_id="" -->
- [x] Bootstrapped templates include multi-user sync documentation
  <!-- roadmap:item id="0ca98b46-c0db-44f8-bd28-3ab810b5bc36" outcome_change_id="" -->

### Legacy Cleanup
<!-- roadmap:section id="fa5eb545-34aa-4b32-b668-fb389a7dcc3a" -->
- [x] All templates use `deciduous` binary (no more `losselot` references)
  <!-- roadmap:item id="ad1b89da-f3a2-4de3-adb2-832cd7315586" outcome_change_id="" -->
- [x] Live graph URLs point to correct project
  <!-- roadmap:item id="d7929609-6548-4cf4-8e41-744c84e04243" outcome_change_id="" -->
- [x] `src/init.rs` uses installed binary path
  <!-- roadmap:item id="a458d914-5062-45ad-9af5-ad76123f3064" outcome_change_id="" -->

### Code Tracking & File Associations (Partial)
<!-- roadmap:section id="1373ac9c-2a3a-4592-af6d-e037f8a57140" -->
- [x] `--files` flag for associating files with nodes
  <!-- roadmap:item id="293ad4e5-6b7b-4ae5-8fa5-db013ee16279" outcome_change_id="" -->
- [x] File browser in TUI detail panel
  <!-- roadmap:item id="20ce3884-d2c7-43f0-a1db-54ab1b826efb" outcome_change_id="" -->

---

## In Progress / High Priority
<!-- roadmap:section id="280ac55f-4143-4a59-ab16-3213806b820d" -->

### Subagent System for Codebase Domains
<!-- roadmap:section id="a1b2c3d4-5e6f-7890-abcd-ef1234567890" -->
*Specialized agents for each part of the deciduous codebase*
- [ ] **Set up domain-specific subagents**
  <!-- roadmap:item id="d4e5f6a7-b8c9-0d1e-2f3a-4b5c6d7e8f9a" outcome_change_id="" -->
  - Each domain gets a specialized agent with focused context and tools
  - Agents can work in parallel on different parts of the codebase
  - Reduces context overhead by scoping to relevant files/patterns
- [ ] **Rust Core Agent** (`src/` excluding `src/tui/`)
  <!-- roadmap:item id="e5f6a7b8-c9d0-1e2f-3a4b-5c6d7e8f9a0b" outcome_change_id="" -->
  - CLI commands, database layer, export/sync logic
  - Diesel ORM, SQLite, command dispatch
  - Focus: `src/main.rs`, `src/db.rs`, `src/lib.rs`, `src/serve.rs`
- [ ] **TUI Agent** (`src/tui/`)
  <!-- roadmap:item id="f6a7b8c9-d0e1-2f3a-4b5c-6d7e8f9a0b1c" outcome_change_id="" -->
  - Terminal UI with Ratatui
  - Views, modals, navigation, keybindings
  - Focus: `src/tui/app.rs`, `src/tui/views/`, `src/tui/widgets/`
- [ ] **Web Viewer Agent** (`web/`)
  <!-- roadmap:item id="a7b8c9d0-e1f2-3a4b-5c6d-7e8f9a0b1c2d" outcome_change_id="" -->
  - React + TypeScript + D3/Dagre
  - Components, hooks, views, styling
  - Focus: `web/src/views/`, `web/src/components/`, `web/src/hooks/`
- [ ] **Tooling/Templates Agent** (`.claude/`, `.windsurf/`, `CLAUDE.md`, `AGENTS.md`)
  <!-- roadmap:item id="b8c9d0e1-f2a3-4b5c-6d7e-8f9a0b1c2d3e" outcome_change_id="" -->
  - Editor integrations, slash commands, rules
  - Template sync with `src/init.rs`
  - Focus: `.claude/commands/`, `.windsurf/rules/`, tooling docs
- [ ] **Documentation Agent** (`docs/`, `README.md`, `ROADMAP.md`)
  <!-- roadmap:item id="c9d0e1f2-a3b4-5c6d-7e8f-9a0b1c2d3e4f" outcome_change_id="" -->
  - User-facing docs, guides, roadmap management
  - GitHub Pages content
  - Focus: `docs/`, `README.md`, `ROADMAP.md`, `CHANGELOG.md`
- [ ] **CI/CD Agent** (`scripts/`, `.github/`)
  <!-- roadmap:item id="d0e1f2a3-b4c5-6d7e-8f9a-0b1c2d3e4f5a" outcome_change_id="" -->
  - Build scripts, GitHub Actions, release automation
  - Pre-commit hooks, validation
  - Focus: `scripts/`, `.github/workflows/`, `Cargo.toml`
- [x] **Fix `/context` command name collision** (renamed to `/recover`)
  <!-- roadmap:item id="e1f2a3b4-c5d6-7e8f-9a0b-1c2d3e4f5a6b" outcome_change_id="" -->
  - Renamed `.claude/commands/context.md` to `recover.md`
  - Also renamed in `.windsurf/rules/` and `.opencode/command/`
  - Updated CLAUDE.md, README.md, and tooling docs

### Narrative User Guide
<!-- roadmap:section id="c31c2378-9b07-4fa1-9ecc-860890108931" -->
*A real, end-to-end tutorial that walks through an actual use case*
- [ ] **Narrative-driven tutorial** (like the jj tutorial)
  <!-- roadmap:item id="a6cb0ced-0b6b-406c-ba46-ca72bb7fb8fb" outcome_change_id="" -->
  - Not a reference manual—a story
  - Follow a real feature from inception to merged PR
  - Show the decision graph evolving alongside the code
  - Include the missteps, dead ends, and course corrections
- [ ] **Cover the full workflow live**
  <!-- roadmap:item id="ea4475f8-425c-425a-9613-c1551f033f96" outcome_change_id="" -->
  - Start with `deciduous init` in a fresh project
  - Log goals, make decisions, reject options, implement actions
  - Show context recovery after a simulated "session break"
  - Generate PR writeup from the graph
  - Deploy to GitHub Pages
- [ ] **Dual perspective: human and AI**
  <!-- roadmap:item id="836bbffe-fabf-4165-8579-6696279e61ec" outcome_change_id="" -->
  - Show the human developer using the TUI to review decisions
  - Show the AI querying the graph to recover context
  - Demonstrate handoff between sessions
- [ ] **Publish as a dedicated guide**
  <!-- roadmap:item id="1c43dd40-e03e-4ec6-be32-6bb3f2bb7611" outcome_change_id="" -->
  - Standalone page on the GH Pages site
  - Could also be an asciinema + written hybrid
  - Link prominently from README

### Real OSS Project Demo - Bootstrap from History
<!-- roadmap:section id="g2h3i4j5-k6l7-8m9n-0o1p-2q3r4s5t6u7v" -->
*Show how to add deciduous to an existing project and build decision history from what already exists*
- [ ] **Demo: Add deciduous to a real open source project**
  <!-- roadmap:item id="h3i4j5k6-l7m8-9n0o-1p2q-3r4s5t6u7v8w" outcome_change_id="" -->
  - Pick a well-known OSS repo (maybe a Rust project for dogfooding)
  - Initialize deciduous and build a decision graph from scratch
  - Document the process as a replicable guide
- [ ] **Import decision history from git commits**
  <!-- roadmap:item id="i4j5k6l7-m8n9-0o1p-2q3r-4s5t6u7v8w9x" outcome_change_id="" -->
  - Parse commit messages for decision-related keywords
  - Auto-generate goal/action/outcome nodes from commit patterns
  - Link nodes to commits automatically
  - `deciduous import git --since "6 months ago"` or similar
  - Heuristics: "feat:" → goal, "fix:" → action+outcome, "refactor:" → decision
- [ ] **Import decision history from PRs**
  <!-- roadmap:item id="j5k6l7m8-n9o0-1p2q-3r4s-5t6u7v8w9x0y" outcome_change_id="" -->
  - Fetch PR titles, descriptions, and comments via GitHub API
  - Extract goals from PR titles/descriptions
  - Extract decisions from PR discussions and review comments
  - Link PRs to generated nodes
  - `deciduous import github-prs --repo owner/repo --since "2024-01-01"`
- [ ] **Import from GitHub Issues**
  <!-- roadmap:item id="k6l7m8n9-o0p1-2q3r-4s5t-6u7v8w9x0y1z" outcome_change_id="" -->
  - Issues often capture goals and decisions
  - Parse issue labels (bug, feature, enhancement) to determine node types
  - Link closed issues to outcome nodes
  - `deciduous import github-issues --repo owner/repo`
- [ ] **Interactive bootstrap wizard**
  <!-- roadmap:item id="l7m8n9o0-p1q2-3r4s-5t6u-7v8w9x0y1z2a" outcome_change_id="" -->
  - `deciduous bootstrap` command for guided setup
  - Walk through recent commits/PRs and ask user to categorize
  - "This commit looks like a feature - create a goal node? [Y/n]"
  - Build initial graph interactively with human validation
  - Option for fully automatic mode with sensible defaults
- [ ] **Publish case study**
  <!-- roadmap:item id="m8n9o0p1-q2r3-4s5t-6u7v-8w9x0y1z2a3b" outcome_change_id="" -->
  - Write up the experience of adding deciduous to an existing project
  - Show before/after: navigating codebase without vs with decision graph
  - Include screenshots of TUI and web viewer exploring imported history
  - Highlight insights discovered from visualizing historical decisions

### Automated Graph Sync Workflow
<!-- roadmap:section id="ce8a9922-4b5b-498e-8467-175eb1600f6f" -->
*Multi-user state management should be seamless*
- [ ] **Default to diff/patch workflow for multi-user state management**
  <!-- roadmap:item id="f91ec3e1-6bcf-4b32-a985-7bf31d82b66d" outcome_change_id="" -->
  - With multiple users coming, graph database sync should be automated by the AI
  - AI should automatically export patches when committing decision-related work
  - AI should automatically apply patches when pulling/syncing
  - Remove the manual friction of remembering to export/apply
- [ ] **Auto-export on commit**
  <!-- roadmap:item id="bb7d6ee1-5ef7-471a-b679-357767acfbbb" outcome_change_id="" -->
  - Before any commit that touched decision-related code, auto-run `deciduous diff export`
  - Use branch-specific patch files (e.g., `.deciduous/patches/$(whoami)-$(branch).json`)
  - Include patch file in the commit automatically
- [ ] **Auto-apply on pull**
  <!-- roadmap:item id="be5d6235-15d9-48bd-a756-787e0ee2a116" outcome_change_id="" -->
  - After `git pull`, detect new `.deciduous/patches/*.json` files
  - Automatically apply them (idempotent - safe to re-apply)
  - Report: "Applied 3 patches from teammates: alice-feature.json, bob-fix.json, carol-refactor.json"
- [ ] **Git hooks for automation**
  <!-- roadmap:item id="b52f95ab-cdfc-4b20-82e0-e2c951148c63" outcome_change_id="" -->
  - `post-commit` hook: auto-export current branch's decisions
  - `post-merge` hook: auto-apply any new patches
  - `pre-push` hook: ensure graph is synced and patches are committed
- [ ] **Claude/Windsurf workflow updates**
  <!-- roadmap:item id="2f9ee8ee-b6a9-440e-8231-00b10b6dd085" outcome_change_id="" -->
  - Update tooling templates to include auto-sync behavior
  - AI should do this automatically, not wait for user to ask
  - "Before pushing, I'll sync the decision graph..." should be default behavior
- [ ] **Conflict resolution**
  <!-- roadmap:item id="60ea1123-20f1-4c37-81ac-e959a9682227" outcome_change_id="" -->
  - When patches have conflicting nodes, use change_id to merge intelligently
  - Prefer latest timestamp when edges conflict
  - Interactive resolution mode if needed: `deciduous diff resolve`

### GitHub Pages Site Fixes
*Get the hosted web viewer working properly*
- [ ] Get GH Pages to not require CLI / configure auth for static hosting
- [ ] Get parity between GitHub Pages site and local React app
- [ ] Make sure all this builds and works + is tested manually for real workflows
- [ ] **Fix GH Pages showing stale graph data** - local DB has all nodes but docs/graph-data.json isn't synced/pushed; need automated workflow to keep GH Pages current

### Context Recovery (Critical)
<!-- roadmap:section id="2b2cd677-f9cd-42c8-ae0a-df0a1fe077b9" -->
*Make compaction restore actually work reliably*
- [ ] `/recover` command fully restores working state after context loss
  <!-- roadmap:item id="2b362072-8187-44bf-b2c1-de6b738f981b" outcome_change_id="" -->
- [ ] Query decision graph for recent goals, decisions, actions in progress
  <!-- roadmap:item id="7283b036-9dd0-4464-b681-51b236b9dfd9" outcome_change_id="" -->
- [ ] **Sort nodes by recency** - most recently updated nodes first
  <!-- roadmap:item id="9b330bb7-2322-4c4d-9090-61b78690e7de" outcome_change_id="" -->
  - Use `updated_at` timestamp for sorting
  - Show N most recent chains (like DAG view recency filtering)
  - Recent activity is most relevant for context recovery
- [ ] Show what was being worked on, what's complete, what's pending
  <!-- roadmap:item id="7e303824-1933-4b67-90f4-b346f1854d89" outcome_change_id="" -->
- [ ] Include recent git activity, uncommitted changes, branch state
  <!-- roadmap:item id="b4aba41a-1011-49f7-a048-e76429d51a9a" outcome_change_id="" -->
- [ ] Pull in relevant prompts from history if available
  <!-- roadmap:item id="50c25904-82ee-4bde-878a-38b672891d25" outcome_change_id="" -->
- [ ] Goal: new session can pick up exactly where the last one left off
  <!-- roadmap:item id="ecfe8c53-c74d-484f-a5b1-f0bde257c3f1" outcome_change_id="" -->

### Story View Componentization
<!-- roadmap:section id="a7f3e891-2c4d-4b5f-9d8a-7e6f1a3c5b2d" -->
*Transform the Story page into a reusable, structured narrative layer over decision graphs*
- [ ] **Componentize Story page elements**
  <!-- roadmap:item id="b8e4f902-3d5e-4c6a-ae9b-8f7g2b4d6c3e" outcome_change_id="" -->
  - Break down Story page into discrete, reusable components
  - Each section (timeline events, milestones, narrative blocks) becomes a component
  - Components should be independently renderable and composable
- [ ] **Define Story data contract/schema**
  <!-- roadmap:item id="c9f5g013-4e6f-5d7b-bf0c-9g8h3c5e7d4f" outcome_change_id="" -->
  - Create a well-defined JSON/TypeScript schema for Story structure
  - Include: timeline events, milestones, phases, narrative blocks, metadata
  - Schema should be versioned and extensible
  - Consider compatibility with external systems (portable format)
- [ ] **Rich context over decision nodes/edges**
  <!-- roadmap:item id="d0g6h124-5f7g-6e8c-cg1d-0h9i4d6f8e5g" outcome_change_id="" -->
  - Allow Story components to be attached to decision graph nodes as batches
  - Stories become a higher-level narrative layer over the raw decision graph
  - Link Story sections to specific goals, decisions, and outcomes
  - Enable querying: "Show me the story for goal #42"
- [ ] **Story as documented institutional memory**
  <!-- roadmap:item id="e1h7i235-6g8h-7f9d-dh2e-1i0j5e7g9f6h" outcome_change_id="" -->
  - Stories capture the "why" and context that raw nodes miss
  - Can be used for onboarding, PR descriptions, release notes
  - Export stories as standalone narratives or embed in docs
- [ ] **Portable format for external use**
  <!-- roadmap:item id="f2i8j346-7h9i-8g0e-ei3f-2j1k6f8h0g7i" outcome_change_id="" -->
  - Story format should work beyond deciduous (potential export targets)
  - Consider: Markdown export, JSON API, embed in other tools
  - Keep schema simple enough to map to other narrative systems

---

## Backlog - Core Features
<!-- roadmap:section id="6f761f06-3a84-475a-9b85-1ee70edf1a6c" -->

### Commit-Centric Graph Navigation
<!-- roadmap:section id="dc855dbb-f5eb-4f20-b84c-b40996be83c8" -->
- [ ] More robust per-commit tooling
  <!-- roadmap:item id="29772243-3c4a-4ec2-8c00-d55ede4692f5" outcome_change_id="" -->
  - Link nodes to commits more reliably
  - Auto-detect commit context when logging actions/outcomes
  - `deciduous add action "..." --commit HEAD` should be the default flow
- [ ] Commit-centric browsing in the web viewer
  <!-- roadmap:item id="799c5ff5-7c68-496a-bafb-491e799e9eea" outcome_change_id="" -->
  - Browse graph starting from commits (not just nodes)
  - Show which nodes are associated with each commit
  - Timeline view organized by commit history
  - Click a commit → see the decision subgraph around it
- [ ] `deciduous log` command to show commits with their linked nodes
  <!-- roadmap:item id="dbd31158-3eae-4b8c-9aad-fb7af734793b" outcome_change_id="" -->
- [ ] Integration with `git log` to annotate commits with decision context
  <!-- roadmap:item id="53b0faaf-c764-48b1-acf6-12141af6226c" outcome_change_id="" -->

### Web Viewer - URL State & Sharing (Completed - PR #105)
<!-- roadmap:section id="b055d90e-d3f2-4d51-919c-934440e6cfdd" -->
- [x] **Query param encoding for shareable state**
  <!-- roadmap:item id="bc03b884-5bb3-4dc1-bf5c-e09387558cfa" outcome_change_id="" -->
  - Encode ALL view parameters in URL query string
  - Branch filter, selected node, view type, expanded chains, etc.
  - Copy URL → share with teammate → they see exact same view
  - Support deep linking to specific nodes: `?node=42&branch=feature-x`
  - Persist state across page refreshes
  - "Copy link" button in UI for easy sharing

### Web Viewer - DAG + Timeline Split View
<!-- roadmap:section id="296b450f-8a01-4b7f-8095-b51fdab3161c" -->
- [ ] **Split view showing DAG and Timeline together**
  <!-- roadmap:item id="beae4f3a-b8f6-484f-be01-09fbb93e1724" outcome_change_id="" -->
  - DAG view on one side, Timeline on the other
  - Selecting a node group in DAG highlights corresponding timeline entries
  - Swap panels in/out - view decision flow and log simultaneously
  - Useful for reviewing decision history while seeing structure
  - Configurable split direction (horizontal/vertical)

### Web Viewer - PR Integration
<!-- roadmap:section id="d48c64dc-d640-49f4-9620-4ca7c9165266" -->
- [ ] **Link PRs to branch-filtered decision graph views**
  <!-- roadmap:item id="333557e3-6445-4091-b8cc-790c1810654f" outcome_change_id="" -->
  - Click a PR link → opens deciduous web viewer filtered to that branch's nodes
  - URL format: `deciduous.site/graph?branch=feature-foo` or similar
  - PR description can embed a link that auto-filters the DAG to just that branch
  - Users can explore the decision reasoning alongside the PR diff
  - Integrate with GitHub PR template to auto-include graph link
- [ ] **GitHub App / Action for PR annotations**
  <!-- roadmap:item id="4b5bb7b8-bbba-47b6-b575-3c9513a9760a" outcome_change_id="" -->
  - Automatically comment on PRs with link to branch-filtered graph
  - Badge showing number of decision nodes in the branch
  - One-click navigation from PR → decision graph explorer
- [x] **DAG recency filtering** (December 2025)
  <!-- roadmap:item id="e965d0ed-aa75-4416-81bd-ae99153a718a" outcome_change_id="" -->
  - Default view shows only 4 most recently active goal chains
  - Chains sorted by most recent node update time within the chain
  - "Show more" and "Show all" controls to expand view
  - Rust utility functions for TUI (`build_chains`, `sort_chains_by_recency`, `get_recent_chains`)

### TUI Enhancements
<!-- roadmap:section id="cea0dada-171d-4616-88f4-4ab5e3c89da5" -->
- [ ] **Phase 2: Multi-goal chains**
  <!-- roadmap:item id="d33d22f2-2fe8-4d31-a838-05a3e8bfbe2a" outcome_change_id="" -->
  - Navigate between related goals
  - Show how goals connect and depend on each other
- [ ] **DAG view improvements** (currently disabled)
  <!-- roadmap:item id="44c18057-632d-416f-baed-0dc238c7a64e" outcome_change_id="" -->
  - Better hierarchical layout algorithm
  - Zoom and pan controls
- [ ] **Settings system** (partially done)
  <!-- roadmap:item id="cf1eedb1-405c-46f5-a733-be8509bc0714" outcome_change_id="" -->
  - [x] `.deciduous/config.toml` exists with branch settings
  - [ ] Timeline order default (newest-first vs oldest-first)
  - [ ] Editor preference (`$EDITOR` fallback chain)
  - [ ] Color theme selection
  - [ ] Key binding customization
  - [ ] Database path configuration
- [ ] **Keyboard shortcut audit and redesign**
  <!-- roadmap:item id="a74c31c3-2e58-4f56-8e5c-0c0c36d184a9" outcome_change_id="" -->
  - Analyze all current shortcuts for intuitiveness
  - Ensure shortcuts are discoverable and memorable
  - Consider user expectations from similar tools (vim, lazygit, ranger)
  - Group related actions with similar key patterns
- [ ] **Visual discoverability**
  <!-- roadmap:item id="703721a8-28ae-48a9-a075-d5aaf2b76fe6" outcome_change_id="" -->
  - Add context-sensitive help hints in footer
  - Show available actions for current context
  - Highlight keyboard shortcuts in help overlay
  - Consider modal indicator showing current mode prominently
- [ ] **Onboarding experience**
  <!-- roadmap:item id="d19154c1-78b4-474b-82a9-fae7b689c7b2" outcome_change_id="" -->
  - First-run tutorial or guided tour
  - Progressive disclosure of advanced features
  - Cheat sheet generation (`deciduous tui --help-keys`)

### Code Tracking & File Associations (Extended)
<!-- roadmap:section id="d14cb3b4-26cd-4299-9732-9f995d850c39" -->
- [ ] **Web UI: "View Code" button on nodes**
  <!-- roadmap:item id="9895465d-aa14-41d9-873f-b3995ccd9d25" outcome_change_id="" -->
  - Click a node → see associated files
  - Show git diff for the commit linked to that node
  - Quick navigation to file locations
- [ ] Update `.claude/commands/` and `.windsurf/rules/` templates
  <!-- roadmap:item id="0757d5ad-4cb9-4016-84dc-4e97d203ccbf" outcome_change_id="" -->
  - Include instructions to log file associations
  - `deciduous add action "..." --files <changed-files>`
- [ ] `deciduous files <node-id>` command to list associated files
  <!-- roadmap:item id="4181f592-a468-410d-9187-94c9877fa91d" outcome_change_id="" -->
- [ ] Reverse lookup: `deciduous nodes --file src/foo.rs` to find nodes touching a file
  <!-- roadmap:item id="e38f8aa9-e58c-4c3e-8dc5-059887b6c619" outcome_change_id="" -->
- [ ] **Enforce file linking**
  <!-- roadmap:item id="7a956d55-fe1e-4ad2-b5fe-5e462f18d8b5" outcome_change_id="" -->
  - Remind/warn when creating action nodes without `--files`
  - Hook integration: auto-detect staged files when logging actions
  - Template instructions emphasizing file association importance
  - "You modified 5 files but didn't link any to this node" warnings
- [ ] **`deciduous audit --associate-files`** (like `--associate-commits`)
  <!-- roadmap:item id="bab97170-86e5-4bd7-934a-fb0fb209e5f9" outcome_change_id="" -->
  - Retroactively link files to nodes based on:
    - Commit associations (node has commit → get files from that commit)
    - Time correlation (files changed near node creation time)
    - Title/description keyword matching (node mentions "auth" → find auth files)
  - Interactive mode: show node + candidate files, let user confirm
  - Batch mode: auto-associate from linked commits (high confidence)
  - `--dry-run` to preview associations
- [ ] **File coverage report**
  <!-- roadmap:item id="155d5dbd-a6d9-429c-a7cc-cd0559489da5" outcome_change_id="" -->
  - `deciduous audit --file-coverage`
  - Show which files have decision graph coverage
  - Identify "dark spots" - frequently changed files with no linked nodes
  - Help ensure important code has documented reasoning

### Prompt Tracking (Extended)
<!-- roadmap:section id="575de692-fcd7-4b61-bf72-a9f173e79ad9" -->
- [x] **View prompts in web UI**
  <!-- roadmap:item id="a4952702-76cb-432a-b63e-c9cc1e0cf97f" outcome_change_id="" -->
  - Display the original prompt in node detail panel
- [ ] Prompt history view in web UI
  <!-- roadmap:item id="23f2302b-adeb-4605-9856-24e023589be8" outcome_change_id="" -->
  - See what prompt led to each decision chain
  - Search/filter by prompt content
- [ ] Prompt templates
  <!-- roadmap:item id="a325348b-845f-40e8-ae64-131be5e0f756" outcome_change_id="" -->
  - Save effective prompts for reuse
  - Share prompt patterns that lead to good decision trees
- [ ] Prompt → Outcome analysis
  <!-- roadmap:item id="188fb079-5786-4938-8bf9-2d6c5206b110" outcome_change_id="" -->
  - Correlate prompt patterns with successful outcomes
  - Identify which prompt styles lead to better decisions
- [ ] **Extended prompt features in web UI**
  <!-- roadmap:item id="6b5ba829-7104-486d-9067-e300bb4a6770" outcome_change_id="" -->
  - Filter/search nodes by prompt content
  - "Copy prompt" button for reuse

---

## Backlog - Tooling & Automation
<!-- roadmap:section id="d790198d-adb4-4a71-9504-43aa89c7c002" -->

### Installation & PATH Handling
<!-- roadmap:section id="f1a2b3c4-d5e6-7f8a-9b0c-1d2e3f4a5b6c" -->
*Graceful handling of common installation issues*
- [ ] **Detect and suggest PATH fixes for `~/.cargo/bin`**
  <!-- roadmap:item id="a2b3c4d5-e6f7-8a9b-0c1d-2e3f4a5b6c7d" outcome_change_id="" -->
  - After `cargo install deciduous`, binary is in `~/.cargo/bin`
  - Many users don't have this in PATH
  - Detect when `deciduous` command fails due to PATH issues
  - Provide clear instructions for adding to PATH:
    - bash/zsh: `export PATH="$HOME/.cargo/bin:$PATH"` in `.bashrc`/`.zshrc`
    - fish: `set -gx PATH $HOME/.cargo/bin $PATH` in `config.fish`
  - Consider: post-install script that checks and warns
  - Consider: `cargo install` wrapper that checks PATH after install
- [ ] **Graceful error messages for common issues**
  <!-- roadmap:item id="b3c4d5e6-f7a8-9b0c-1d2e-3f4a5b6c7d8e" outcome_change_id="" -->
  - "command not found: deciduous" → suggest PATH fix
  - Database not found → suggest `deciduous init`
  - Permission errors → suggest fix
  - Missing dependencies (graphviz for --png) → suggest install command

### Release Tooling
<!-- roadmap:section id="8588ceb0-d117-4cc4-a46b-45c10b927bd5" -->
- [ ] **Automated release notes generation**
  <!-- roadmap:item id="fd513dd0-bbda-4ada-9ef6-3e86920fab00" outcome_change_id="" -->
  - `deciduous release-notes --from v0.7.0 --to v0.8.0`
  - Query graph for all goals/outcomes between versions
  - Group by feature area (TUI, sync, export, etc.)
  - Generate markdown summary of what changed and why
- [ ] **PR tagging for releases**
  <!-- roadmap:item id="5a80c569-db6b-4270-ab55-fdfe399d7c95" outcome_change_id="" -->
  - Tag PRs that landed in a release
  - `deciduous release tag-prs v0.8.0` - adds label to all PRs in release
  - Link PRs back to decision graph nodes
- [ ] **Release summary from decision graph**
  <!-- roadmap:item id="973894dc-06e9-4145-aabc-79df155ac657" outcome_change_id="" -->
  - Walk the graph to find all completed goals since last release
  - Show the decision chain that led to each feature
  - Include key prompts/feedback that drove changes
  - Generate changelog with decision context
- [ ] **GitHub Actions CI/CD for releases**
  <!-- roadmap:item id="b7374793-b889-45b0-a671-9c5aca0f952f" outcome_change_id="" -->
  - Automate the entire release pipeline via GitHub Actions
  - Trigger: push tag matching `v*` pattern (e.g., `v0.9.0`)
  - Pipeline steps:
    1. Run `cargo test` and `cargo clippy`
    2. Build release binaries for multiple platforms (linux, macos, windows)
    3. Create GitHub Release with auto-generated notes
    4. Publish to crates.io (`cargo publish`)
    5. Upload platform binaries as release assets
  - Workflow file: `.github/workflows/release.yml`
  - Optional: Separate workflow for nightly/canary builds
  - Consider: cargo-dist or release-plz for Rust-specific release automation
- [ ] **Pre-built binary distribution for all platforms**
  <!-- roadmap:item id="c4d5e6f7-a8b9-0c1d-2e3f-4a5b6c7d8e9f" outcome_change_id="" -->
  - Build and distribute pre-compiled binaries so users don't need Rust/cargo
  - **Target platforms:**
    - Linux x86_64 (glibc and musl for maximum compatibility)
    - Linux aarch64 (ARM64 for Raspberry Pi, cloud instances)
    - macOS x86_64 (Intel Macs)
    - macOS aarch64 (Apple Silicon M1/M2/M3)
    - Windows x86_64 (.exe)
  - **Distribution channels:**
    - GitHub Releases with platform-specific tarballs/zips
    - Install script: `curl -fsSL https://deciduous.dev/install.sh | sh`
    - Homebrew tap for macOS: `brew install deciduous/tap/deciduous`
    - Consider: Chocolatey for Windows, apt/yum repos for Linux
  - **Binary naming convention:** `deciduous-{version}-{os}-{arch}.{ext}`
  - **Verification:** SHA256 checksums and optional GPG signatures
  - **Self-update:** `deciduous update` to fetch latest binary for current platform
- [ ] **Release checklist automation**
  <!-- roadmap:item id="39e31162-c380-4cfe-8035-4287db092e71" outcome_change_id="" -->
  - `deciduous release prep v0.9.0` - prepare release locally
    - Bump version in Cargo.toml
    - Update CHANGELOG (if exists)
    - Run tests and clippy
    - Generate release notes from decision graph
  - `deciduous release publish v0.9.0` - trigger the release
    - Create and push git tag
    - GitHub Actions takes over from there
  - Fail-safes: block release if tests fail or graph has incomplete goals

### Roadmap Manager Tool
<!-- roadmap:section id="68ed088c-baa4-462b-88eb-4b643a66d070" -->
- [ ] **Link roadmap items to work**
  <!-- roadmap:item id="16c29706-60c6-4d95-9599-0ef63c6a4401" outcome_change_id="" -->
  - Roadmap items should link to decision graph nodes
  - Track progress through the graph (goal -> decisions -> outcomes)
  - Show completion status based on linked outcomes
- [ ] **Work item integration** (future)
  <!-- roadmap:item id="befd5e49-4d39-4e41-a992-cf9c3bc6f4ea" outcome_change_id="" -->
  - Design with external ticketing in mind (JIRA, Linear, GitHub Issues)
  - Roadmap items could link to external tickets
  - Sync status bidirectionally
  - Keep this extensible for enterprise workflows
- [ ] **CLI commands**
  <!-- roadmap:item id="5a340309-f0c6-41f3-a5ef-92bf96f91e3b" outcome_change_id="" -->
  - `deciduous roadmap list` - show roadmap items and their status
  - `deciduous roadmap link <item> <node-id>` - link to graph node
  - `deciduous roadmap status` - show completion based on linked nodes

### Claude Code Hooks Integration
<!-- roadmap:section id="cd6c44c1-f3c7-468d-91c8-7679d4216ebe" -->
- [ ] Explore using Claude Code hooks to guarantee workflow behavior
  <!-- roadmap:item id="42ffb395-a6e0-4f72-9220-01d2500c1c97" outcome_change_id="" -->
  - Claude Code supports hooks that run on various events (tool calls, messages, etc.)
  - Could enforce decision graph logging more reliably than instructions alone
- [ ] **Pre-tool hooks**: Before file edits
  <!-- roadmap:item id="607617aa-3cc9-4255-905b-1286f551431f" outcome_change_id="" -->
  - Auto-log `action` node when Claude is about to modify files
  - Capture which files are being changed
  - Block edits if no active goal exists in the graph
- [ ] **Post-tool hooks**: After successful operations
  <!-- roadmap:item id="c665b039-94c7-4ebf-bcb7-74f6c0eea12d" outcome_change_id="" -->
  - Auto-log `outcome` nodes after code changes succeed
  - Auto-link outcomes to their parent actions
  - Trigger `deciduous sync` after significant changes
- [ ] **Pre-commit hooks**: Before git commits
  <!-- roadmap:item id="ff193f08-1fc8-4396-81c8-810b31e44d90" outcome_change_id="" -->
  - Verify graph integrity (no orphan outcomes/actions)
  - Require at least one goal node for the current session
  - Auto-add commit hash to recent action/outcome nodes
- [ ] **Session start hooks**: On conversation begin
  <!-- roadmap:item id="9b8ef4e0-4596-4404-9edf-7bc3f9d59716" outcome_change_id="" -->
  - Auto-run `/recover` equivalent
  - Load graph state and surface pending decisions
  - Warn if there are disconnected nodes needing attention
- [ ] **Prompt submit hooks**: When user sends a message
  <!-- roadmap:item id="0f10f266-71d2-4d0e-b846-520a1221c1a7" outcome_change_id="" -->
  - Detect feature requests and auto-create goal nodes
  - Capture original prompt in node metadata
- [ ] Hook configuration in `.deciduous/hooks.toml`
  <!-- roadmap:item id="0bdaf5a4-f758-4a8c-98f5-59f118923933" outcome_change_id="" -->
  - Enable/disable specific hooks
  - Configure strictness (warn vs block)
  - Custom hook scripts for project-specific needs
- [ ] Document hook setup in Claude Code settings
  <!-- roadmap:item id="9a35ae83-55f4-4008-8edc-1f90cda65e99" outcome_change_id="" -->
  - Integration guide for `.claude/settings.json`
  - Example hook configurations for different workflow styles

### Editor Memories Integration
<!-- roadmap:section id="6d4a0665-85a3-4cc4-869e-a345dbea1383" -->
- [ ] **Actually save memories correctly at runtime**
  <!-- roadmap:item id="f9db3d48-0358-4233-be99-363bc18b63b0" outcome_change_id="" -->
  - Windsurf creates memories during runtime - need to patch into that flow
  - Currently memories are static; need dynamic memory creation as decisions happen
  - Investigate Windsurf's memory creation API/mechanism
  - Hook into the runtime to persist decision context as memories are created
  - Ensure memories reflect real-time decision graph state
- [ ] **Leverage Claude Code and Windsurf memories**
  <!-- roadmap:item id="bf93ff06-3f27-43fb-8597-16b8b3311338" outcome_change_id="" -->
  - Both editors have "memories" features that persist across sessions
  - Could store decision graph summaries, recent goals, key patterns
  - Auto-populate memories with recent decision context
  - Memories could include: current goals, pending decisions, recent outcomes
- [ ] **Claude Code memories**
  <!-- roadmap:item id="dd388b65-b732-4569-a232-a4accf4e3074" outcome_change_id="" -->
  - Detect and update `.claude/memories/` or equivalent
  - Store project-specific decision patterns
  - "Last session worked on goal #X, consider continuing"
- [ ] **Windsurf memories**
  <!-- roadmap:item id="0a19a41f-7aff-453e-b874-83c497c94327" outcome_change_id="" -->
  - Already have `.windsurf/memories.md` template
  - Enhance with dynamic decision graph summaries
  - Auto-retrieve relevant memories when starting sessions
- [ ] **Memory-graph sync**
  <!-- roadmap:item id="14bb8c81-26c4-483c-96e9-dbbad3a88a2b" outcome_change_id="" -->
  - `deciduous memories export` - export decision summaries to memory format
  - `deciduous memories update` - update editor memories with recent decisions
  - Bidirectional: memories inform context recovery

### Roadmap Skill & TUI Integration
<!-- roadmap:section id="e63eb8ba-c47c-4dd3-b761-7288288d64ee" -->
- [ ] **Claude Code skill for roadmap management**
  <!-- roadmap:item id="6ed19ee8-ddf9-4f3e-aea5-c3773b57b1bc" outcome_change_id="" -->
  - `/roadmap` skill to interact with ROADMAP.md and decision graph
  - Show current roadmap items, their status, linked nodes
  - Add/update roadmap items with decision graph links
- [ ] **TUI roadmap view**
  <!-- roadmap:item id="1c394a1b-4dff-4884-b7d3-09d6e7643187" outcome_change_id="" -->
  - New view in `deciduous tui` for browsing roadmap items
  - Show roadmap items linked to decision graph nodes
  - Filter by status (completed, in progress, backlog)
  - Navigate from roadmap item → linked goal/decision nodes
- [ ] **Roadmap-graph bidirectional links**
  <!-- roadmap:item id="6601e0f4-9a5d-42b9-9f3b-aa3667b06ef2" outcome_change_id="" -->
  - Roadmap items can reference graph nodes by ID
  - Graph nodes can link back to roadmap items
  - Track completion: roadmap item is "done" when linked outcome exists

### Retroactive Commit Association
<!-- roadmap:section id="23faed37-c03f-4ec1-a4d8-35beac9648d5" -->
- [ ] **Audit existing nodes and associate with git commits**
  <!-- roadmap:item id="69eb1514-69e8-44b3-850b-b55a2dd3357d" outcome_change_id="" -->
  - Many early nodes were created without `--commit` flag
  - Cross-reference node `created_at` timestamps with `git log` dates
  - Match node titles/descriptions to commit messages
  - `deciduous audit --associate-commits` command to suggest matches
  - Interactive mode: show node + candidate commits, let user confirm
  - Batch mode: auto-associate high-confidence matches (>90% title similarity)
- [ ] **Backfill script for existing graphs**
  <!-- roadmap:item id="4cbf110a-d71b-4656-8bcf-2c579922239b" outcome_change_id="" -->
  - One-time migration to enrich old nodes with commit data
  - Parse commit messages for keywords matching node titles
  - Use time windows (node created within N minutes of commit)
  - Generate report of associations made
- [ ] **Ongoing commit detection**
  <!-- roadmap:item id="e2073939-c039-49f3-a005-3279f590107d" outcome_change_id="" -->
  - When running `deciduous sync`, detect recent commits without linked nodes
  - Suggest: "Found 3 commits with no linked decisions - want to associate them?"
  - Help maintain commit-node linkage going forward

### Git Integration & Pre-commit Hook Awareness
<!-- roadmap:section id="c08ba750-b673-4eaa-9087-48d45de678c2" -->
- [ ] Inspect and respect pre-commit hooks
  <!-- roadmap:item id="121c9436-9310-4eae-8216-8d80ff4e42ba" outcome_change_id="" -->
  - Detect `.git/hooks/pre-commit` or `.husky/` hooks
  - Parse hook contents to understand what checks run
  - Warn users if hooks might reject commits (linting, formatting, tests)
- [ ] Pre-flight commit validation
  <!-- roadmap:item id="1f46e70a-59e5-4d49-bf54-1ceb2542dd53" outcome_change_id="" -->
  - `deciduous commit --dry-run` to simulate what hooks would do
  - Show which checks would pass/fail before actual commit
- [ ] Auto-fix integration
  <!-- roadmap:item id="1ba867da-d496-4ba1-8536-070682c4ec51" outcome_change_id="" -->
  - If hooks run formatters (prettier, rustfmt), detect and apply fixes
  - Re-stage auto-fixed files before commit
- [ ] Hook-aware templates
  <!-- roadmap:item id="c493b38f-61e6-48f0-a870-ef1bdefe2d1b" outcome_change_id="" -->
  - Update `.claude/commands/` and `.windsurf/rules/` to mention pre-commit awareness
  - Instruct LLMs to check for hooks before committing

### Git.log File Reliability
<!-- roadmap:section id="66d64caf-4af3-494e-b8c5-c04ca26895d7" -->
- [ ] Investigate why git.log file isn't always being created/updated
  <!-- roadmap:item id="493346ee-4047-4678-9950-0c69c0d14e3d" outcome_change_id="" -->
  - User reported missing git.log during session
  - This file should always exist and track all git commands run
- [ ] Ensure git.log is created on `deciduous init`
  <!-- roadmap:item id="0e79d5f2-e1be-4cd6-ad66-4dfae924b1f2" outcome_change_id="" -->
- [ ] Add git.log writing to all git-related operations
  <!-- roadmap:item id="f8af0ccd-0dd8-41c7-bb96-5fef4b46c59f" outcome_change_id="" -->
- [ ] Consider moving git.log into `.deciduous/` directory for better organization
  <!-- roadmap:item id="0115c1b4-a40f-43f4-95cf-7e74f2a6a43e" outcome_change_id="" -->
- [ ] Add `deciduous log` command to view git.log contents
  <!-- roadmap:item id="df2e9d45-e1cd-43af-8793-cfca8645f6bc" outcome_change_id="" -->
- [ ] Document git.log purpose and location in tooling files
  <!-- roadmap:item id="d14b1ccd-5fe3-40a3-a0c7-2f897ce25076" outcome_change_id="" -->

### Template Sync (init.rs ↔ actual files)
<!-- roadmap:section id="f53d0d43-4614-45c5-9afb-ec3d7039991b" -->
- [x] **FIX: `deciduous update` destroys custom content in command files** (v0.8.2)
  <!-- roadmap:item id="3979af69-4ecc-46e4-8fc3-7b3fe6ea075e" outcome_change_id="" -->
  - ~~Currently `update --claude` overwrites `.claude/commands/*.md` completely~~
  - ~~This destroys all custom instructions users have added~~
  - Fixed: init.rs templates now match actual files
- [ ] **Auto-detect when tooling files are out of sync**
  <!-- roadmap:item id="8d020391-0ef7-4fa0-8983-335a6c71004f" outcome_change_id="" -->
  - When `.claude/commands/*.md`, `.windsurf/rules/*.md`, `CLAUDE.md`, or `AGENTS.md` are modified
  - Remind/enforce updating corresponding templates in `src/init.rs`
  - The templates in init.rs should match what's in the actual files
- [ ] **Validation command**
  <!-- roadmap:item id="76d5313d-42e9-4244-8b2b-b4470169f742" outcome_change_id="" -->
  - `deciduous validate-templates` - check if templates match actual files
  - Warn if they've diverged
  - Optionally auto-update init.rs from actual files
- [ ] **Memory/hook for sync**
  <!-- roadmap:item id="853c3c2c-5118-4046-8b1a-6e7c42844ce6" outcome_change_id="" -->
  - Add to Claude Code memories: "If you modify .claude/commands or CLAUDE.md, also update src/init.rs templates"
  - Could be a pre-commit check or a deciduous hook
- [ ] **FULLY AUTOMATE template sync** (Critical)
  <!-- roadmap:item id="bdd46e3e-9b19-48f1-934b-52d1cc76a016" outcome_change_id="" -->
  - **Goal: Never manually sync templates again**
  - Build script or CI step that extracts actual file contents into init.rs
  - Single source of truth: the actual `.claude/`, `.windsurf/`, `CLAUDE.md`, `AGENTS.md` files
  - `src/init.rs` templates are auto-generated from these files
  - Options:
    - `build.rs` script that reads files and generates template constants
    - Pre-commit hook that regenerates init.rs template section
    - CI check that fails if templates don't match actual files
  - Include ALL tooling files:
    - `.claude/commands/decision.md`
    - `.claude/commands/recover.md`
    - `.windsurf/rules/decision.md`
    - `.windsurf/rules/recover.md`
    - `CLAUDE.md`
    - `AGENTS.md`
  - Never have drift between what `deciduous init` creates and what the repo actually uses

---

## Backlog - Architecture & Quality
<!-- roadmap:section id="998babf3-7e02-451b-b027-f08457bfc047" -->

### TUI Architecture Refactor
<!-- roadmap:section id="d12ee466-92fb-4e1f-bb0a-b6b3f5aef0eb" -->
- [ ] **Functional core, imperative shell**
  <!-- roadmap:item id="bda1dd2f-3e17-4d91-bf01-46961590c2f8" outcome_change_id="" -->
  - Extract pure functions from app.rs for all state transformations
  - Move all I/O to thin imperative shell at edges
  - State transitions should be pure: `fn update(state: App, event: Event) -> App`
- [ ] **Comprehensive test coverage**
  <!-- roadmap:item id="7ffc5b1e-e962-469b-afdf-56a7af080a40" outcome_change_id="" -->
  - Unit tests for all pure state transformation functions
  - Test navigation logic without terminal
  - Test modal state machines
  - Test filtering and search logic
  - Property-based tests for state invariants
- [ ] **TEA pattern enforcement**
  <!-- roadmap:item id="65fa51df-d96a-446a-9146-e32505ef0be8" outcome_change_id="" -->
  - Strict Model/Update/View separation
  - No side effects in view functions (already done, verify)
  - Event handlers return new state, don't mutate
  - Extract reusable update functions

### Type Unification (TUI + Web)
<!-- roadmap:section id="866afa76-a663-4f36-ac57-21e69f2474de" -->
- [ ] **Shared type definitions**
  <!-- roadmap:item id="7c51596a-385f-4d83-a4ae-ef8a2667cc12" outcome_change_id="" -->
  - Unify TUI types (src/tui/types.rs) with web types (web/src/types/graph.ts)
  - Single source of truth for node/edge structures
  - Consider code generation or shared schema
- [ ] **Port TUI features to web viewer**
  <!-- roadmap:item id="8f8a7b3a-4d9d-44bb-ae96-055533604c2a" outcome_change_id="" -->
  - Timeline view with vim-style navigation
  - Commit modal with split-view diff
  - Branch filtering and fuzzy search
  - Goal story view
- [ ] **Parallel development workflow**
  <!-- roadmap:item id="fbb41ce4-f252-44f4-aa8c-30f01e84127d" outcome_change_id="" -->
  - Changes to one should auto-update the other
  - Shared test fixtures for both platforms
  - Document the type mapping

### Documentation Restructure
<!-- roadmap:section id="74b5be43-4c7c-4e8c-95e7-6b80d97451f0" -->
- [ ] Rethink the `docs/` folder organization
  <!-- roadmap:item id="d7f198e6-3a9c-4fc9-a181-8afde9eb69de" outcome_change_id="" -->
  - Currently: GitHub Pages viewer lives here
  - Problem: Also contains ad-hoc design docs (like MULTI_USER_SYNC.md)
  - Consider: Separate `docs/` (user-facing) from `design/` (internal design docs)
  - Consider: Move viewer to dedicated folder
- [ ] Consolidate documentation
  <!-- roadmap:item id="0d10551c-88ba-4e74-bffc-6f76ea01f6c6" outcome_change_id="" -->
  - Single source of truth in CLAUDE.md (the canonical reference)
  - .claude/commands/ and .windsurf/rules/ derive from CLAUDE.md patterns
  - README.md stays user-facing (installation, quick start)
- [ ] Auto-generate tooling docs
  <!-- roadmap:item id="86cfe6af-cd29-4cb1-a6de-86461fc53993" outcome_change_id="" -->
  - `deciduous docs` command to output markdown documentation
  - Include all commands, options, examples
  - Keep README.md and tooling files in sync automatically

---

## Backlog - Future Vision
<!-- roadmap:section id="b98b3892-5851-463d-9d2a-e87503d47cd8" -->

### TUI PR Review Mode
<!-- roadmap:section id="fadbdfed-ffbb-48dc-bd7e-668e645ece6a" -->
- [ ] **GitHub PR integration in TUI**
  <!-- roadmap:item id="6707d5b3-0bdd-41c0-bebf-673d4aaf8e20" outcome_change_id="" -->
  - Pull in PR comments from GitHub API
  - Show file-level and line-level comments alongside code
  - Browse commits in PR context with associated comments
  - Mark comments as resolved/addressed from TUI
  - Reply to comments directly from TUI
- [ ] **Code review workflow**
  <!-- roadmap:item id="d77c14ce-2346-40c1-b38e-eaa4fe4ead40" outcome_change_id="" -->
  - Navigate between commented locations
  - Jump from decision node to related PR/commit comments
  - See review status and approval state

### LLM Critique & Analysis
<!-- roadmap:section id="30289abe-f336-4cff-af9f-5696a78d0bf6" -->
- [ ] `deciduous critique --goal <id>` - Have an LLM analyze a goal's decision chain
  <!-- roadmap:item id="c940eee4-473b-47c4-9a31-887ff23d0ece" outcome_change_id="" -->
  - Review decisions made, options chosen/rejected
  - Identify potential blind spots or unconsidered alternatives
  - Evaluate confidence levels vs actual outcomes
  - Suggest improvements for future similar decisions
- [ ] Multi-model critique comparison
  <!-- roadmap:item id="af84c631-682b-4eb0-aad1-68b18019a6b6" outcome_change_id="" -->
  - `deciduous critique --goal <id> --models claude,gpt4,gemini`
  - See how different models evaluate the same decision chain
  - Highlight where models agree/disagree on quality
- [ ] Critique storage - save critiques as special nodes linked to goals
  <!-- roadmap:item id="21b34e7f-494b-4135-95fe-f8b51b8dd857" outcome_change_id="" -->

### LLM Benchmarking Framework
<!-- roadmap:section id="e4d61b19-2be0-4861-85fa-38cc15ff888e" -->
- [ ] **Goal-based benchmarking**: Use the same goal/task across multiple LLMs
  <!-- roadmap:item id="8824ee41-1a7e-44c1-b349-c25313a3e7e5" outcome_change_id="" -->
  - Define a goal with acceptance criteria
  - Run each model on the same task
  - Compare: decisions made, paths taken, outcomes achieved
- [ ] `deciduous benchmark` command
  <!-- roadmap:item id="b88840e4-ad0f-4b2c-b348-a102a477f699" outcome_change_id="" -->
  - `deciduous benchmark --task "Implement feature X" --models claude,gpt4,gemini,llama`
  - Each model gets isolated graph namespace
  - Automated or human evaluation of outcomes
- [ ] Metrics to capture:
  <!-- roadmap:item id="c2a67646-02d5-41c4-8a6c-9be2eeb618b0" outcome_change_id="" -->
  - Decision quality (did they consider good options?)
  - Path efficiency (how direct was the route to outcome?)
  - Confidence calibration (were high-confidence decisions correct?)
  - Recovery ability (how did they handle setbacks?)
  - Graph structure (complexity, dead ends, backtracking)
- [ ] Benchmark reports
  <!-- roadmap:item id="41cd958c-a6c6-4133-bc10-886839952dfd" outcome_change_id="" -->
  - Side-by-side comparison of decision graphs
  - Aggregate stats across multiple benchmark runs
  - Export to shareable format for publishing results
- [ ] Reproducible benchmark suites
  <!-- roadmap:item id="0e9d694e-cd3d-40c6-b442-c7a076d4b2db" outcome_change_id="" -->
  - Define standard tasks with known-good solutions
  - Version-controlled benchmark definitions
  - CI integration for regression testing model capabilities

### DuckDB for OLAP Analytics
<!-- roadmap:section id="46921027-f771-404d-b5b8-e65de9114871" -->
- [ ] Add DuckDB as optional analytical backend for decision graph queries
  <!-- roadmap:item id="ffc67234-8453-4cb9-944e-315485f78683" outcome_change_id="" -->
  - SQLite is great for OLTP (single-project, real-time logging)
  - DuckDB excels at OLAP (cross-project analytics, time-series queries, aggregations)
- [ ] Use cases for analytical queries:
  <!-- roadmap:item id="06aa79f7-b55f-4dcf-8fa9-cb296899abaa" outcome_change_id="" -->
  - **Cross-project patterns**: "What decision patterns lead to successful outcomes across all my projects?"
  - **Time-series analysis**: "How has my decision-making evolved over the past 6 months?"
  - **Confidence calibration**: "Are my high-confidence decisions actually more successful?"
  - **Path analysis**: "What's the average depth of decision trees that lead to good outcomes?"
  - **Bottleneck detection**: "Which decision types take longest to resolve?"
- [ ] Implementation ideas:
  <!-- roadmap:item id="5c072170-da3b-4f45-bb78-ad0292b41b57" outcome_change_id="" -->
  - Export SQLite graphs to Parquet files for DuckDB ingestion
  - `deciduous export --parquet` for analytical snapshots
  - `deciduous analytics` subcommand for running OLAP queries
  - Optional: federated queries across multiple project databases
- [ ] Potential analytical views:
  <!-- roadmap:item id="fa90c90c-694d-43bb-91b6-7ce199a329ad" outcome_change_id="" -->
  - Decision funnel analysis (goal → decision → action → outcome conversion)
  - Confidence vs outcome correlation matrix
  - Branch/feature complexity metrics
  - Session productivity heatmaps
  - Node type distribution over time
- [ ] Visualization integration:
  <!-- roadmap:item id="ecec3c70-2710-4f0d-9180-0090ebd95df2" outcome_change_id="" -->
  - Export to formats compatible with BI tools (Metabase, Superset, etc.)
  - Built-in charts in TUI or web viewer
  - `deciduous report` to generate analytical summaries

---

### Live Graph Diff Viewer
<!-- roadmap:section id="f0d1634e-1453-4c52-9fb6-372e76cb74e3" -->
- [ ] **Real-time graph following**
  <!-- roadmap:item id="65acf702-5b9b-489d-bd77-4216b2780ddf" outcome_change_id="" -->
  - Diff viewer that "follows" the decision graph as nodes are written
  - User sees live updates as AI creates/modifies nodes
  - Stream-style view of graph changes in real-time
  - Could use websocket or file watching to detect changes
  - TUI mode: live refresh every N seconds or on file change
  - Web mode: push updates via SSE or websocket
- [ ] **Visualization of graph deltas**
  <!-- roadmap:item id="dea7bc7f-fb4c-4012-9455-42cba819238e" outcome_change_id="" -->
  - Show what nodes/edges were added, modified, deleted
  - Highlight new nodes with a visual indicator
  - Animate transitions between graph states
  - Timeline scrubber to replay graph evolution
- [ ] **Integration with editor workflows**
  <!-- roadmap:item id="cc42fb18-c304-43dc-8cc2-c3d0d1897b96" outcome_change_id="" -->
  - Side panel in IDE showing live graph updates
  - Works alongside Claude Code, Windsurf, Cursor
  - Non-blocking: viewer runs in separate process/terminal

### Live Graph Editor (Experimental)
<!-- roadmap:section id="727c6fdd-eb2b-467d-b4db-b692739ba9dd" -->
- [ ] **Bidirectional graph editing**
  <!-- roadmap:item id="4262504d-ca47-420e-a4a1-1a5add05ed81" outcome_change_id="" -->
  - Not just viewing - actually edit the graph live
  - Add/modify/delete nodes through the viewer UI
  - Changes sync back to SQLite database
  - Potentially coordinate edits with running AI session
- [ ] **Collaborative editing**
  <!-- roadmap:item id="b31b6da5-b1b5-4bc1-866a-b02a8442981d" outcome_change_id="" -->
  - Multiple users can edit graph simultaneously
  - Operational transform or CRDT for conflict resolution
  - Real-time cursor presence (see where others are editing)
  - This is ambitious but could enable pair programming with AI
- [ ] **Graph manipulation tools**
  <!-- roadmap:item id="9c84fe8a-5a4a-4b5f-9ded-c408f6e4e2ef" outcome_change_id="" -->
  - Drag and drop nodes to reorganize
  - Quick actions: merge nodes, split nodes, reparent
  - Undo/redo support
  - Validation: prevent invalid graph structures

---

### Claude Desktop Integration
<!-- roadmap:section id="d00d0c04-a8cf-4f83-a39e-56bb74159c78" -->
- [ ] **MCP server for Claude Desktop**
  <!-- roadmap:item id="2c8dc247-a368-4075-9c75-3dd9a12a6483" outcome_change_id="" -->
  - Expose deciduous as an MCP (Model Context Protocol) server
  - Allow Claude Desktop users to interact with decision graphs
  - Not everyone uses CLI or Claude Code - meet users where they are
- [ ] **Consider ACP (Agent Client Protocol) integration**
  <!-- roadmap:item id="7b34569e-679e-48f9-9c47-ce5dc124a6eb" outcome_change_id="" -->
  - [Symposium ACP SDK](https://github.com/symposium-dev/symposium-acp) - Rust SDK for ACP
  - ACP standardizes agent-client communication (complementary to MCP)
  - Potential integration approaches:
    - **Proxy component**: Intercept agent communications, auto-log decisions
    - **Base agent**: Implement deciduous as an ACP-compliant agent
    - **Conductor integration**: Decision graph as middleware in proxy chains
  - Benefits:
    - Automatic decision capture without manual logging
    - Works with any ACP-compatible editor/agent
    - Composable architecture - add/remove decision tracking dynamically
- [ ] **Core MCP tools**
  <!-- roadmap:item id="2df1008e-e3c9-48e5-8c9f-6dd15380aaca" outcome_change_id="" -->
  - `add_node` - create goal/decision/action/outcome/observation nodes
  - `link_nodes` - connect nodes with edges
  - `query_graph` - search and filter the decision graph
  - `get_context` - retrieve recent decisions for context recovery
  - `sync_graph` - export/import for sharing
- [ ] **Desktop-friendly workflows**
  <!-- roadmap:item id="75d666c8-574a-4602-8d4f-7d1e0d2e7bdb" outcome_change_id="" -->
  - Decision tracking for non-coding tasks (writing, research, planning)
  - Project management and task breakdown
  - Meeting notes → decision nodes conversion
  - General knowledge work, not just software engineering
- [ ] **Cross-platform graph access**
  <!-- roadmap:item id="e08b4028-69e6-43ff-9a0c-7d175b40f979" outcome_change_id="" -->
  - Same `.deciduous/` database works from CLI, TUI, web, and Desktop
  - Claude Desktop can read graphs created by Claude Code and vice versa
  - Unified decision history across all interaction modes
- [ ] **Resource exposure**
  <!-- roadmap:item id="0e868539-b3b4-4eb6-93ee-a01506f41789" outcome_change_id="" -->
  - Expose graph data as MCP resources
  - Recent nodes, active goals, pending decisions as browsable resources
  - Graph statistics and health metrics
- [ ] **Prompt support**
  <!-- roadmap:item id="5f408020-abb5-4208-bdd7-a4c1ef4e4241" outcome_change_id="" -->
  - MCP prompts for common workflows
  - "Start new goal", "Review decisions", "Context recovery"
  - Guided decision-making templates

---

### Historical Linkages - Documents as Graph Citizens
<!-- roadmap:section id="a1b2c3d4-5e6f-7a8b-9c0d-1e2f3a4b5c6d" -->
*External artifacts become first-class nodes in the decision graph, not just metadata*

This goes beyond simple integrations - documents, release notes, tags, PR bodies, and conversations become **part of the real graph and metadata model**.

- [ ] **Document nodes**
  <!-- roadmap:item id="d1e2f3a4-b5c6-7d8e-9f0a-1b2c3d4e5f6a" outcome_change_id="" -->
  - New node types: `document`, `release`, `conversation`, `pr_body`
  - Documents link to decision nodes they reference
  - Bidirectional: decisions can cite documents, documents can embed decision context
  - Full-text search across document content
  - Version tracking for document evolution

- [ ] **Release notes as graph nodes**
  <!-- roadmap:item id="e2f3a4b5-c6d7-8e9f-0a1b-2c3d4e5f6a7b" outcome_change_id="" -->
  - Each release becomes a node linked to its constituent goals/outcomes
  - `deciduous release create v0.9.0` - auto-generates release node
  - Release node has edges to all completed goals since last release
  - Queryable: "What decisions led to v0.8.0?" → traverse the graph
  - Release changelogs derived from graph traversal, not manual writing

- [ ] **Git tags as graph anchors**
  <!-- roadmap:item id="f3a4b5c6-d7e8-9f0a-1b2c-3d4e5f6a7b8c" outcome_change_id="" -->
  - Tags sync to the graph automatically
  - `v0.8.12` tag creates a snapshot node capturing graph state at that point
  - Time-travel: view graph as it existed at any tagged version
  - Diff tags: "What changed in the graph between v0.8.0 and v0.9.0?"

- [ ] **PR bodies integrated into graph**
  <!-- roadmap:item id="a4b5c6d7-e8f9-0a1b-2c3d-4e5f6a7b8c9d" outcome_change_id="" -->
  - PR description becomes a document node linked to branch's decisions
  - PR comments can reference graph nodes: "This implements decision #42"
  - Auto-populate PR template with links to relevant decision nodes
  - Review comments link back to graph for context
  - Merged PR triggers outcome nodes automatically

- [ ] **Conversation threads as decision context**
  <!-- roadmap:item id="b5c6d7e8-f9a0-1b2c-3d4e-5f6a7b8c9d0e" outcome_change_id="" -->
  - Chat/discussion threads can be attached to decisions
  - "Why did we choose X?" → link to the Slack/Discord/GitHub discussion
  - Conversation summaries as nodes (AI-generated or manual)
  - Preserve the human context that influenced decisions
  - Search discussions by linked decision nodes

- [ ] **Writeups and design docs**
  <!-- roadmap:item id="c6d7e8f9-a0b1-2c3d-4e5f-6a7b8c9d0e1f" outcome_change_id="" -->
  - RFC/ADR documents become graph nodes
  - Design doc links to the decisions it proposes
  - Implementation decisions link back to the design doc
  - Track drift: "Design said X, but we implemented Y because..."
  - `deciduous writeup` output becomes a linkable document node

- [ ] **Cross-repository linkages**
  <!-- roadmap:item id="d7e8f9a0-b1c2-3d4e-5f6a-7b8c9d0e1f2a" outcome_change_id="" -->
  - Link decisions across different projects/repos
  - "Phoenix Framework decision #123 influenced our decision #456"
  - External project graphs can be referenced (read-only links)
  - Useful for documenting dependencies on upstream choices
  - Federation: query across multiple project graphs

- [ ] **Query the extended graph**
  <!-- roadmap:item id="e8f9a0b1-c2d3-4e5f-6a7b-8c9d0e1f2a3b" outcome_change_id="" -->
  - `deciduous query "show all PRs related to goal #42"`
  - `deciduous query "what conversations mentioned auth decisions?"`
  - `deciduous query "release notes that cite this outcome"`
  - Graph traversal includes document nodes seamlessly
  - Filter by document type, date range, author

- [ ] **Import/export document linkages**
  <!-- roadmap:item id="f9a0b1c2-d3e4-5f6a-7b8c-9d0e1f2a3b4c" outcome_change_id="" -->
  - Export decision graph with all linked documents
  - Import external documentation into the graph
  - Markdown files can declare graph links in frontmatter
  - `deciduous import-docs ./docs/*.md` - scan for decision references

---

## Future Enhancements
<!-- roadmap:section id="00400a97-3edd-4b3d-a9b2-8a2ef30aa110" -->
- [ ] Support for additional editors (Cursor, Copilot, etc.)
  <!-- roadmap:item id="e0367f85-b881-4bdb-a7d4-ef09719673d9" outcome_change_id="" -->
- [ ] `deciduous init --all` to bootstrap for all supported editors at once
  <!-- roadmap:item id="0442b8cf-1f85-44e9-9a42-89585d36ac91" outcome_change_id="" -->