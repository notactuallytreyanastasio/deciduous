use chrono::{Local, TimeZone};
use clap::{CommandFactory, Parser, Subcommand};
use colored::Colorize;
use deciduous::github::{ensure_roadmap_label, GitHubClient};
use deciduous::roadmap::{
    generate_issue_body, parse_roadmap, write_roadmap_with_metadata, RoadmapSection,
};
use deciduous::{
    filter_graph_by_ids, generate_pr_writeup, graph_to_dot, parse_node_range, reconcile, Config,
    Database, DotConfig, RecordStore, SyncReport, WriteupConfig,
};
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

#[derive(Parser, Debug)]
#[command(name = "deciduous")]
#[command(
    author,
    version,
    about = "Decision graph tooling for AI-assisted development"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Initialize deciduous in current directory
    ///
    /// Sets up the decision graph database and AI assistant integration.
    /// By default, sets up Claude Code. Use flags to choose assistants.
    Init {
        /// Set up Claude Code integration (.claude/, CLAUDE.md)
        #[arg(long)]
        claude: bool,

        /// Set up OpenCode integration (.opencode/, AGENTS.md)
        #[arg(long)]
        opencode: bool,

        /// Set up Windsurf integration (.windsurf/)
        #[arg(long)]
        windsurf: bool,

        /// Set up both Claude Code and OpenCode
        #[arg(long)]
        both: bool,

        /// Disable automatic version checking (enabled by default)
        #[arg(long)]
        no_auto_update: bool,
    },

    /// Update AI assistant integration files to latest version
    ///
    /// Auto-detects which assistants are installed (.claude/, .opencode/,
    /// .windsurf/) and updates their commands, skills, hooks and the
    /// deciduous section of CLAUDE.md / AGENTS.md.
    ///
    /// Files deciduous wrote are replaced; files someone else wrote are kept
    /// (Markdown gets the new text appended in a marked block). In
    /// .claude/settings.json it only removes the logging hooks earlier
    /// versions installed, keeping your entries and key order. Everything it
    /// changes is first copied to .deciduous/update-backups/<time>/.
    ///
    /// Does NOT touch .deciduous/config.toml or docs/. With a [remote]
    /// configured it also leaves .gitignore, .gitattributes, .git/config and
    /// graph.json alone.
    Update {
        /// Update every deciduous project directly under this directory (and
        /// the directory itself, if it is one), one after another
        #[arg(long, value_name = "DIR")]
        all: Option<PathBuf>,
    },

    /// Check if deciduous integration files need updating
    ///
    /// Compares .deciduous/.version with current binary version.
    /// Exits with code 0 if up to date, 1 if update needed.
    CheckUpdate {},

    /// Toggle automatic version checking
    ///
    /// When enabled, a hook checks crates.io once per 24h and
    /// tells your AI assistant to inform you of new versions.
    AutoUpdate {
        /// "on" to enable, "off" to disable
        toggle: String,
    },

    /// Add a new node to the decision graph
    Add {
        /// Node type: goal, decision, option, action, outcome, observation
        node_type: String,

        /// Title of the node
        title: String,

        /// Optional description
        #[arg(short, long)]
        description: Option<String>,

        /// Confidence level (0-100)
        #[arg(short, long, value_parser = clap::value_parser!(u8).range(0..=100))]
        confidence: Option<u8>,

        /// Git commit to link this node to: HEAD, a branch, a tag, origin/main,
        /// a hash or a prefix of one. Stored as the full hash; a rev git
        /// cannot resolve here is refused.
        #[arg(long)]
        commit: Option<String>,

        /// Prompt that triggered this decision (stored as metadata)
        #[arg(short, long)]
        prompt: Option<String>,

        /// Read prompt from stdin (for multi-line prompts)
        #[arg(long)]
        prompt_stdin: bool,

        /// Files associated with this node (comma-separated)
        #[arg(short, long)]
        files: Option<String>,

        /// Git branch (auto-detected if not specified)
        #[arg(short, long)]
        branch: Option<String>,

        /// Skip auto-detection of git branch
        #[arg(long)]
        no_branch: bool,

        /// Created date (RFC3339 format or "YYYY-MM-DD" or "YYYY-MM-DD HH:MM:SS")
        /// Use this to backdate nodes to past commits
        #[arg(long)]
        date: Option<String>,
    },

    /// Add an edge between nodes
    Link {
        /// Source node: local id, change_id prefix, or server id
        from: String,

        /// Target node: local id, change_id prefix, or server id
        to: String,

        /// Rationale for this connection
        #[arg(short, long)]
        rationale: Option<String>,

        /// Edge type: leads_to, requires, chosen, rejected, blocks, enables,
        /// took_from (this node borrowed from that one, usually across branches)
        #[arg(short = 't', long, default_value = "leads_to")]
        edge_type: String,
    },

    /// Remove an edge between two nodes
    Unlink {
        /// Source node: local id, change_id prefix, or server id
        from: String,

        /// Target node: local id, change_id prefix, or server id
        to: String,

        /// Remove only the edge of this type (required when the two nodes
        /// are joined by more than one)
        #[arg(short = 't', long = "type")]
        edge_type: Option<String>,
    },

    /// Delete a node and all its connected edges
    Delete {
        /// Node to delete: local id, change_id prefix, or server id
        id: String,

        /// Show what would be deleted without actually deleting
        #[arg(long)]
        dry_run: bool,
    },

    /// Update node status
    Status {
        /// Node: local id, change_id prefix, or server id
        id: String,

        /// New status: pending, active, completed, rejected
        status: String,
    },

    /// Update or add a prompt to an existing node
    Prompt {
        /// Node to update: local id, change_id prefix, or server id
        id: String,

        /// The prompt text (omit to read from stdin)
        prompt: Option<String>,
    },

    /// List all nodes
    Nodes {
        /// Filter by git branch
        #[arg(short, long)]
        branch: Option<String>,

        /// Filter by node type (goal, decision, action, etc.)
        #[arg(short = 't', long)]
        node_type: Option<String>,

        /// Filter by theme name
        #[arg(long)]
        theme: Option<String>,
    },

    /// List all edges
    Edges,

    /// Show detailed information about a single node
    Show {
        /// Node to display: local id, change_id prefix, or server id
        id: String,

        /// Show JSON output instead of formatted
        #[arg(long)]
        json: bool,
    },

    /// Export full graph as JSON
    Graph,

    /// Start the graph viewer server (or the multi-graph API daemon with --api)
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value = "3000")]
        port: u16,

        /// Run the multi-graph JSON API daemon instead of the viewer
        #[arg(long)]
        api: bool,

        /// API mode: data directory holding graphs/<id>/deciduous.db
        /// (required; falls back to DECIDUOUS_API_DATA_DIR)
        #[arg(long)]
        data_dir: Option<PathBuf>,

        /// API mode: bearer token required on every request
        /// (falls back to DECIDUOUS_API_TOKEN)
        #[arg(long)]
        token: Option<String>,

        /// API mode: bind address
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
    },

    /// Sync the decision graph: reconcile .deciduous/graph.json with the
    /// local database, in both directions
    ///
    /// Run it after `git pull` to receive teammates' decisions and before
    /// `git push` to make sure yours are written out. The graph file is
    /// created on first run; the pre-0.17 JSONL event log is imported and
    /// removed automatically.
    Sync {
        /// Removed in 1.0.5 with the GitHub Pages export; accepted and ignored
        #[arg(short, long, hide = true)]
        output: Option<PathBuf>,

        /// Report what would change without writing anything.
        /// Exits 1 if anything is pending, so it works as a pre-push check.
        #[arg(long)]
        check: bool,

        /// Removed in 1.0.5 with the GitHub Pages export; accepted and ignored
        #[arg(long, hide = true)]
        no_pages: bool,
    },

    /// Create a database backup
    Backup {
        /// Output path (default: deciduous_backup_<timestamp>.db)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Show recent command log
    Commands {
        /// Number of commands to show
        #[arg(short, long, default_value = "20")]
        limit: i64,
    },

    /// Export graph as DOT format
    Dot {
        /// Output file (default: stdout). Use --auto for branch-specific naming.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Root node IDs to filter (comma-separated, traverses children)
        #[arg(short, long)]
        roots: Option<String>,

        /// Specific node IDs or ranges (e.g., "1-11" or "1,3,5-10")
        #[arg(short, long)]
        nodes: Option<String>,

        /// Generate PNG using graphviz (requires dot command)
        #[arg(long)]
        png: bool,

        /// Auto-generate branch-specific filename in docs/ (e.g., docs/decision-graph-feature-foo.dot)
        #[arg(long)]
        auto: bool,

        /// Graph title
        #[arg(short, long)]
        title: Option<String>,

        /// Graph direction: TB (top-bottom), LR (left-right), BT or RL
        #[arg(long, default_value = "TB", value_parser = ["TB", "LR", "BT", "RL"])]
        rankdir: String,
    },

    /// Generate PR writeup from decision graph
    Writeup {
        /// PR title
        #[arg(short, long)]
        title: Option<String>,

        /// Root node IDs to include (comma-separated, traverses children)
        #[arg(short, long)]
        roots: Option<String>,

        /// Specific node IDs or ranges (e.g., "1-11" or "1,3,5-10")
        #[arg(short = 'n', long)]
        nodes: Option<String>,

        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// PNG filename to embed (auto-detects repo/branch for GitHub URL)
        #[arg(long)]
        png: Option<String>,

        /// Auto-detect PNG from branch name (looks for docs/decision-graph-{branch}.png)
        #[arg(long)]
        auto: bool,

        /// Skip DOT graph section
        #[arg(long)]
        no_dot: bool,

        /// Skip test plan section
        #[arg(long)]
        no_test_plan: bool,
    },

    /// Migrate database to add change_id columns (for multi-user sync)
    Migrate,

    /// Git merge driver for the graph file (registered by init/update/sync):
    /// record-by-record three-way merge of .deciduous/graph.json
    #[command(hide = true)]
    MergeRecord {
        /// Common ancestor version (%O; empty file when both sides added it)
        base: PathBuf,
        /// Our version (%A); the merged result is written here
        ours: PathBuf,
        /// Their version (%B)
        theirs: PathBuf,
    },

    /// Deprecated: replaced by `deciduous sync` (kept so old scripts keep working)
    #[command(hide = true)]
    Events {
        #[command(subcommand)]
        action: EventsAction,
    },

    /// Audit and maintain graph data quality
    Audit {
        /// Associate commits with nodes by matching titles to commit messages
        #[arg(long)]
        associate_commits: bool,

        /// Minimum keyword match score (0-100, default 50)
        #[arg(long, default_value = "50")]
        min_score: u8,

        /// Only show what would be done, don't modify database
        #[arg(long)]
        dry_run: bool,

        /// Auto-apply without confirmation (use with caution)
        #[arg(long)]
        yes: bool,
    },

    /// Show the pulse of the decision graph - active state, gaps, and health
    Pulse {
        /// Filter by git branch
        #[arg(short, long)]
        branch: Option<String>,

        /// Number of recent nodes to show
        #[arg(short, long, default_value = "10")]
        recent: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Show only the summary section
        #[arg(long)]
        summary: bool,
    },

    /// Manage evolution narratives (.deciduous/narratives.md)
    Narratives {
        #[command(subcommand)]
        action: NarrativesAction,
    },

    /// Retroactive graph building - atomic operations for common archaeology patterns
    Archaeology {
        #[command(subcommand)]
        action: ArchaeologyAction,
    },

    /// Manage ROADMAP.md sync with GitHub Issues
    Roadmap {
        #[command(subcommand)]
        action: RoadmapAction,
    },

    /// Manage Claude Code hooks (pre-edit blocks, post-commit reminders)
    Hooks {
        #[command(subcommand)]
        action: HooksAction,
    },

    /// Show Claude Code integration status (hooks, commands, skills)
    Integration {},

    /// Manage OpenCode integration (plugins, commands, AGENTS.md)
    Opencode {
        #[command(subcommand)]
        action: OpencodeAction,
    },

    /// Manage document attachments on decision nodes
    Doc {
        #[command(subcommand)]
        action: DocAction,
    },

    /// Manage theme definitions (create, list, delete)
    Themes {
        #[command(subcommand)]
        action: ThemesAction,
    },

    /// Tag or untag nodes with themes
    Tag {
        #[command(subcommand)]
        action: TagAction,
    },

    /// Start MCP (Model Context Protocol) server on stdin/stdout
    ///
    /// Exposes the full deciduous API as MCP tools for AI assistants.
    /// Configure in your MCP client:
    /// {"command": "deciduous", "args": ["mcp"]}
    Mcp {},

    /// Configure and use a shared graph server
    ///
    /// One Postgres behind an MCP endpoint holds every project's graph, one
    /// workspace per repository. The local database becomes a cache of it.
    /// The token is read from DECIDUOUS_MCP_TOKEN, never from config.
    Remote {
        #[command(subcommand)]
        action: RemoteAction,
    },

    /// Generate shell completions
    Completion {
        /// Shell type: bash, zsh, fish, powershell, elvish
        shell: clap_complete::Shell,
    },

    /// Removed in 1.0.3; exits 0 so hooks installed by 1.0.2 stay silent
    /// until `deciduous update` takes them out.
    /// Demonstration: an Opus lead and four Sonnet workers build one Tetris in
    /// iTerm2 or Ghostty panes. `deciduous demo-swarm --help` for options.
    #[command(name = "demo-swarm", hide = true, disable_help_flag = true)]
    DemoSwarm {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    #[command(name = "log-loop", hide = true)]
    LogLoop {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        _args: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
enum RemoteAction {
    /// Connect this project to a shared graph server, step by step
    ///
    /// Asks whether the graph lives on this machine (PostgreSQL and the server
    /// in Docker, set up if needed) or on a server someone else runs (URL and
    /// token, checked before anything is stored). Then it writes [remote],
    /// stores the token and registers the server with Claude Code. --local or
    /// --url answer the question for scripts.
    Setup {
        /// Use this machine's server, set up with Docker if it is not running
        #[arg(long, conflicts_with = "url")]
        local: bool,

        /// Use the server at this URL (token from DECIDUOUS_MCP_TOKEN or the stored one)
        #[arg(long)]
        url: Option<String>,

        /// Port to publish this machine's server on (default: asked, a free one
        /// suggested). Only used the first time it is set up; after that the
        /// port in ~/.config/deciduous/server/.env stands.
        #[arg(long, conflicts_with = "url")]
        port: Option<u16>,
    },

    /// Store the API token outside every repository (mode 0600)
    ///
    /// Reads the token from stdin so it never lands in shell history.
    Login {
        /// Server URL to verify the token against before storing it
        #[arg(long)]
        url: Option<String>,
    },

    /// Forget the stored token
    Logout,

    /// Point this project at a shared graph server
    Init {
        /// Base URL, e.g. https://example.com/deciduous-mcp
        url: String,

        /// Workspace name. Defaults to the git repository's root directory.
        #[arg(long)]
        workspace: Option<String>,
    },

    /// Show how far the local database has drifted from the server
    Status,

    /// Send the server the writes waiting in .deciduous/remote-log.jsonl
    ///
    /// Every local write is logged as an operation naming only what it
    /// changed; this replays the ones the server has not acknowledged, in
    /// order. The server applies each at most once, so pushing twice is safe.
    Push {
        /// Discard the ops the server rejected (after reading why with
        /// `remote status`)
        #[arg(long, conflicts_with_all = ["seed", "overwrite"])]
        drop_rejected: bool,

        /// Send the rejected ops again (after the cause is fixed, or for an
        /// op this machine set aside because the server failed on it)
        #[arg(long, conflicts_with_all = ["drop_rejected", "seed", "overwrite"])]
        retry_rejected: bool,

        /// Discard one op, waiting or rejected, by the start of its id (as
        /// `remote status` prints it)
        #[arg(long, value_name = "OP_ID", conflicts_with_all = ["drop_rejected", "retry_rejected", "seed", "overwrite"])]
        drop: Option<String>,

        /// Also send nodes, edges and documents the server lacks that no op
        /// covers: history written before this project had a remote. Never
        /// changes a row the server already has.
        #[arg(long, conflicts_with = "overwrite")]
        seed: bool,

        /// Send the whole local graph and replace the server's copy of every
        /// row it already has (the pre-1.0.3 behaviour)
        #[arg(long)]
        overwrite: bool,

        /// Also send what this copy changed and never logged: for each node
        /// `remote status` lists as Different, the fields the server has
        /// not changed since this copy last pulled; and deletes made here
        /// that never became ops (1.0.7, no [remote] yet, a failed append).
        /// A field the server did change is listed, not sent.
        #[arg(long, conflicts_with_all = ["drop_rejected", "overwrite"])]
        repair: bool,

        /// With --repair: also send the fields it lists as changed on the
        /// server, over the server's value
        #[arg(long, requires = "repair")]
        overwrite_server: bool,
    },

    /// Refresh the local database from the server
    Pull,

    /// Configure many repositories at once
    ///
    /// Walks a directory for projects that already have a .deciduous, and
    /// points each at the server. Existing configuration is left alone.
    Adopt {
        /// Directory to walk (default: the current directory)
        #[arg(default_value = ".")]
        root: String,

        /// Server URL
        #[arg(long)]
        url: String,

        /// Show what would change without writing
        #[arg(short = 'n', long)]
        dry_run: bool,
    },

    /// Stream this project's writes live, one line per event, as they happen
    ///
    /// Connects to the server's event socket and prints each node write as
    /// it lands, quoting the node's own title: which branch, what type,
    /// what it says. Updates are shown as updates, not as new nodes. The
    /// connection is re-opened whenever it drops, with a backoff that
    /// starts at one second.
    ///
    /// Verifies the server and token first, the same way `init` does, so a
    /// bad token fails here against a clear message rather than as an
    /// opaque handshake failure. `--url` prints the socket URL instead of
    /// connecting, for any other WebSocket client; it carries the token in
    /// the query string because the handshake cannot carry a header.
    Watch {
        /// Print the socket URL and exit instead of connecting
        #[arg(long)]
        url: bool,

        /// Print the URL plus a ready-to-paste Monitor tool call for Claude Code, and exit
        #[arg(long)]
        claude_code: bool,

        /// Only these node types, comma-separated (e.g. outcome,observation)
        #[arg(long, value_delimiter = ',')]
        types: Vec<String>,

        /// Only this branch; repeat for several
        #[arg(long = "branch")]
        branches: Vec<String>,

        /// Also show edge events (off by default: they are most of the traffic)
        #[arg(long)]
        edges: bool,

        /// Print the raw JSON frame for each event instead of a formatted line
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
enum EventsAction {
    /// Rebuild local database from event logs and checkpoint
    ///
    /// Loads checkpoint (if exists), then replays all events after the checkpoint.
    /// This reconstructs the database from the shared event history.
    Rebuild {
        /// Only show what would be done without modifying the database
        #[arg(long)]
        dry_run: bool,
    },

    /// Create a checkpoint and optionally clear old events
    ///
    /// Checkpoints capture full graph state. Events older than the checkpoint
    /// can be safely deleted to keep the repo size bounded.
    Checkpoint {
        /// Clear event logs after creating checkpoint
        #[arg(long)]
        clear_events: bool,
    },

    /// Show sync status (pending events, last checkpoint, etc.)
    Status,

    /// Initialize event-based sync in this repository
    ///
    /// Creates .deciduous/graph.json and adds it to .gitignore
    /// the local database while tracking the sync directory.
    Init,

    /// Emit an event for a node (for testing/manual sync)
    Emit {
        /// Node to emit an event for: local id, change_id prefix, or server id
        node_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum RoadmapAction {
    /// Initialize roadmap sync (parses ROADMAP.md and adds metadata)
    Init {
        /// Path to ROADMAP.md (default: ROADMAP.md)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Refresh roadmap items (clears and re-parses ROADMAP.md, preserving decision graph)
    Refresh {
        /// Path to ROADMAP.md (default: ROADMAP.md)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Sync ROADMAP.md with GitHub Issues (dry-run by default, use --execute to apply)
    Sync {
        /// Path to ROADMAP.md (default: ROADMAP.md)
        #[arg(short, long)]
        path: Option<PathBuf>,

        /// GitHub repo in owner/repo format (auto-detected from git remote)
        #[arg(short, long)]
        repo: Option<String>,

        /// Actually apply changes (default is dry-run mode)
        #[arg(long)]
        execute: bool,

        /// Create GitHub issues for new sections
        #[arg(long, default_value = "true")]
        create_issues: bool,
    },

    /// List roadmap items with status
    List {
        /// Path to ROADMAP.md (default: ROADMAP.md)
        #[arg(short, long)]
        path: Option<PathBuf>,

        /// Filter by section name
        #[arg(short, long)]
        section: Option<String>,

        /// Show only items with GitHub issues
        #[arg(long)]
        with_issues: bool,

        /// Show only items without GitHub issues
        #[arg(long)]
        without_issues: bool,
    },

    /// Link a roadmap item to a decision graph outcome node
    Link {
        /// Roadmap item change_id or title (partial match)
        item: String,

        /// Outcome node to link: local id, change_id prefix, or server id
        outcome_id: String,
    },

    /// Remove outcome link from a roadmap item
    Unlink {
        /// Roadmap item change_id or title (partial match)
        item: String,
    },

    /// Show sync conflicts
    Conflicts {
        /// Resolve conflicts interactively
        #[arg(long)]
        resolve: bool,
    },

    /// Show sync status summary
    Status {
        /// Path to ROADMAP.md (default: ROADMAP.md)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Audit completion status of roadmap items
    Check {
        /// Path to ROADMAP.md (default: ROADMAP.md)
        #[arg(short, long)]
        path: Option<PathBuf>,

        /// Show only incomplete items
        #[arg(long)]
        incomplete: bool,

        /// Show only complete items
        #[arg(long)]
        complete: bool,
    },
}

#[derive(Subcommand, Debug)]
enum HooksAction {
    /// Install Claude Code hooks from config
    ///
    /// Generates shell scripts in .claude/hooks/ and settings.json
    /// based on hooks defined in .deciduous/config.toml
    Install {},

    /// Show status of configured and installed hooks
    Status {},

    /// Uninstall hooks (remove .claude/hooks/ and clear settings.json)
    Uninstall {},
}

#[derive(Subcommand, Debug)]
enum OpencodeAction {
    /// Install OpenCode integration (plugins, commands, AGENTS.md)
    ///
    /// Creates .opencode/plugin/ with TypeScript plugins,
    /// .opencode/command/ with custom commands,
    /// opencode.json config, and AGENTS.md instructions.
    Install {},

    /// Show status of OpenCode integration
    Status {},

    /// Uninstall OpenCode integration (remove .opencode/)
    Uninstall {},
}

#[derive(Subcommand, Debug)]
enum NarrativesAction {
    /// Initialize narratives.md with active goal titles as sections
    Init {
        /// Output path (default: narratives.md in the project's .deciduous/)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Overwrite existing file
        #[arg(long)]
        force: bool,
    },

    /// Display narratives.md contents
    Show {
        /// Path to narratives.md (default: the one in the project's .deciduous/)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// List all pivot points (revisit nodes) with their full chains
    Pivots {
        /// Filter by git branch
        #[arg(short, long)]
        branch: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
enum ArchaeologyAction {
    /// Create a full pivot chain atomically (replaces 7 manual commands)
    ///
    /// Creates: observation -> revisit -> new_decision, marks old as superseded.
    Pivot {
        /// Existing approach being reconsidered: local id, change_id prefix, or server id
        from_id: String,

        /// Observation text (what was learned that triggers the pivot)
        observation: String,

        /// New approach/decision title
        new_approach: String,

        /// Confidence for the new decision (0-100)
        #[arg(short, long, value_parser = clap::value_parser!(u8).range(0..=100))]
        confidence: Option<u8>,

        /// Reason/rationale for why the old approach failed
        #[arg(short, long)]
        reason: Option<String>,

        /// Only show what would be created, don't modify database
        #[arg(long)]
        dry_run: bool,
    },

    /// Show all nodes sorted chronologically
    Timeline {
        /// Number of most recent nodes to show (0 = all)
        #[arg(short, long, default_value = "0")]
        limit: usize,

        /// Filter by node type
        #[arg(short = 't', long)]
        node_type: Option<String>,

        /// Filter by git branch
        #[arg(short, long)]
        branch: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Mark a node as superseded, optionally cascading to descendants
    Supersede {
        /// Node to mark as superseded: local id, change_id prefix, or server id
        id: String,

        /// Also mark all descendant nodes as superseded
        #[arg(long)]
        cascade: bool,

        /// Only show what would be changed, don't modify database
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug)]
enum DocAction {
    /// Attach a file to a decision graph node
    Attach {
        /// Node: local id, change_id prefix, or server id
        node_id: String,

        /// Path to the file to attach
        file: PathBuf,

        /// Manual description
        #[arg(short, long)]
        description: Option<String>,

        /// Generate AI description using claude CLI
        #[arg(long)]
        ai_describe: bool,
    },

    /// List documents attached to a node (or all nodes)
    List {
        /// Node to list documents for: local id, change_id prefix, or server id (omit for all)
        node_id: Option<String>,

        /// Show detached (removed) documents too
        #[arg(long)]
        include_detached: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Set or update the description of a document
    Describe {
        /// Document ID
        doc_id: i32,

        /// Description text (omit to read from stdin)
        description: Option<String>,

        /// Generate AI description using claude CLI
        #[arg(long)]
        ai: bool,
    },

    /// Detach (soft-delete) a document from its node
    Detach {
        /// Document ID to detach
        doc_id: i32,
    },

    /// Show details of a specific document
    Show {
        /// Document ID
        doc_id: i32,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Open the attached file in the default application
    Open {
        /// Document ID
        doc_id: i32,
    },

    /// Garbage-collect orphaned files (no active document records reference them)
    Gc {
        /// Only show what would be deleted
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug)]
enum ThemesAction {
    /// Create a new theme
    Create {
        /// Theme name (will be lowercase, dash-separated)
        name: String,

        /// Hex color code (e.g., "#ef4444")
        #[arg(short, long, default_value = "#6b7280")]
        color: String,

        /// Theme description
        #[arg(short, long)]
        description: Option<String>,
    },

    /// List all themes
    List,

    /// Delete a theme and remove all node associations
    Delete {
        /// Theme name to delete
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum TagAction {
    /// Add a theme to a node
    Add {
        /// Node: local id, change_id prefix, or server id
        node_id: String,

        /// Theme name
        theme: String,
    },

    /// Remove a theme from a node
    Remove {
        /// Node: local id, change_id prefix, or server id
        node_id: String,

        /// Theme name
        theme: String,
    },

    /// List themes for a node
    List {
        /// Node: local id, change_id prefix, or server id
        node_id: String,
    },

    /// Auto-suggest themes for a node based on keywords and AI
    Suggest {
        /// Node to suggest themes for: local id, change_id prefix, or server id (omit for all untagged nodes)
        node_id: Option<String>,

        /// Apply suggestions without confirmation
        #[arg(long)]
        apply: bool,
    },

    /// Confirm a suggested theme (change from "suggested" to "manual")
    Confirm {
        /// Node: local id, change_id prefix, or server id
        node_id: String,

        /// Theme name to confirm
        theme: String,
    },
}

/// The server's refusals of writes made here: not ops this machine set
/// aside, and not edits it only applied from graph.json (a refusal of one
/// of those means the server is ahead of git).
fn own_refusals(rejected: &[(deciduous::oplog::Op, String)]) -> usize {
    rejected
        .iter()
        .filter(|(op, why)| !deciduous::remote::is_set_aside(why) && !op.from_git())
        .count()
}

/// Says which refused writes were dropped because they are over.
fn print_settled(settled: &[deciduous::oplog::Op]) {
    if settled.is_empty() {
        return;
    }
    println!(
        "  dropped {} refused write(s) that are settled: this copy and the server now agree on what they wrote",
        settled.len()
    );
    for op in settled.iter().take(10) {
        println!("    {}", op.body.describe());
    }
}

/// `std::process::exit`, after sending what this process queued for the
/// server. `exit` skips destructors, so every command that wrote and then
/// failed (a pivot whose third step errors, say) left its first writes in the
/// log unsent, while the same command succeeding sent them.
fn exit(code: i32) -> ! {
    if let Some(log) = deciduous::oplog::take_appended() {
        deciduous::remote::replay_after_write(&log);
    }
    if let Some(dir) = CHECK_SCRATCH.get() {
        let _ = std::fs::remove_dir_all(dir);
    }
    std::process::exit(code)
}

/// The short hash of HEAD when it is detached and git is not in the middle
/// of a merge, rebase, cherry-pick or revert: someone looking at an old
/// commit (`git checkout HEAD~15`, bisect, a CI checkout). None on a branch,
/// outside a repository, or while an operation is in progress, since a
/// rebase is detached too and its graph file does need writing (conflict
/// markers merged, edits kept).
fn detached_head() -> Option<String> {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
    };
    let on_branch = git(&["symbolic-ref", "-q", "HEAD"])?;
    if on_branch.status.success() {
        return None;
    }
    for op in [
        "rebase-merge",
        "rebase-apply",
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
    ] {
        let path = git(&["rev-parse", "--git-path", op])?;
        if !path.status.success() {
            return None;
        }
        let path = String::from_utf8_lossy(&path.stdout).trim().to_string();
        if std::path::Path::new(&path).exists() {
            return None;
        }
    }
    let head = git(&["rev-parse", "--short", "HEAD"])?;
    head.status
        .success()
        .then(|| String::from_utf8_lossy(&head.stdout).trim().to_string())
}

/// `reconcile` with imports applied to the database and every export
/// discarded: it runs against a scratch copy of the graph file, which is
/// dropped afterwards. Imports never write the file, so the real one is left
/// byte for byte as the checked-out commit has it.
fn reconcile_without_exporting(db: &Database, store: &RecordStore) -> Result<SyncReport, String> {
    let dir = check_scratch_dir()?;
    let copy = dir.join("graph.json");
    std::fs::copy(store.path(), &copy).map_err(|e| {
        format!(
            "copying {} to {}: {e}",
            store.path().display(),
            copy.display()
        )
    })?;
    let scratch = RecordStore::open(&copy)
        .ok_or_else(|| format!("{} vanished while it was being read", copy.display()))?;
    let result = reconcile(db, &scratch, false);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Moves the export counts out of `report` (they did not happen) and
/// describes them.
fn take_exports(report: &mut SyncReport) -> Vec<String> {
    let mut out = Vec::new();
    for (n, what) in [
        (std::mem::take(&mut report.nodes_exported), "node(s)"),
        (std::mem::take(&mut report.edges_exported), "edge(s)"),
        (std::mem::take(&mut report.themes_exported), "theme(s)"),
        (std::mem::take(&mut report.tags_exported), "tag(s)"),
    ] {
        if n > 0 {
            out.push(format!("{n} {what}"));
        }
    }
    out
}

/// Where `sync --check` keeps the empty database it compares against when
/// the project has none. Removed by `exit()`, which every path of that
/// command ends in.
static CHECK_SCRATCH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

fn check_scratch_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!(
        "deciduous-sync-check-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("creating a scratch directory {}: {e}", dir.display()))?;
    let _ = CHECK_SCRATCH.set(dir.clone());
    Ok(dir)
}

/// The subgraph `dot` and `writeup` export: `--nodes`, `--roots`, or all
/// of it. A part of either spec that is not an id exits with an error
/// naming it; it used to be dropped, and `--roots zz` printed an empty
/// digraph with exit code 0.
///
/// Each part that is not a range of local ids is a node reference like any
/// other command's (local id, change_id or server id, see
/// [`resolve_node_or_exit`]); `dot --nodes 2d6aff66` used to say it was "not
/// a node id or a range". A range is `a-b` in decimal with `a` shorter than
/// eight digits: `12345678-1234` is the start of a UUID, not a range.
fn cli_export_subgraph(
    db: &Database,
    graph: deciduous::DecisionGraph,
    nodes: Option<String>,
    roots: Option<String>,
) -> deciduous::DecisionGraph {
    let resolve = |spec: &str, ranges: bool| -> String {
        spec.split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(|part| {
                let is_range = ranges
                    && part.split_once('-').is_some_and(|(a, b)| {
                        a.len() < 8
                            && !a.is_empty()
                            && !b.is_empty()
                            && a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit())
                    });
                if is_range {
                    part.to_string()
                } else {
                    resolve_node_or_exit(db, part).to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    let result = if let Some(node_spec) = nodes {
        parse_node_range(&resolve(&node_spec, true))
            .map(|spec| filter_graph_by_ids(&graph, &spec.select(&graph)))
    } else if let Some(root_spec) = roots {
        deciduous::parse_root_ids(&resolve(&root_spec, false))
            .map(|ids| deciduous::filter_graph_from_roots(&graph, &ids))
    } else {
        Ok(graph)
    };
    result.unwrap_or_else(|e| {
        eprintln!("{} {}", "Error:".red(), e);
        exit(1);
    })
}

/// `deciduous serve --api`. Returns the process exit code.
fn run_api_daemon(
    port: u16,
    data_dir: Option<PathBuf>,
    token: Option<String>,
    bind: String,
) -> i32 {
    let token_flag = token.filter(|t| !t.is_empty());
    if token_flag.is_some() {
        eprintln!(
            "{} --token is visible to every user on this machine in `ps`; \
             prefer DECIDUOUS_API_TOKEN",
            "Warning:".yellow()
        );
    }
    // An empty --token (an unset shell variable) falls back to the
    // environment rather than winning over it.
    let token = token_flag.or_else(|| {
        std::env::var("DECIDUOUS_API_TOKEN")
            .ok()
            .filter(|t| !t.is_empty())
    });
    let Some(token) = token else {
        eprintln!(
            "{} API mode needs a bearer token: pass --token or set DECIDUOUS_API_TOKEN",
            "Error:".red()
        );
        return 1;
    };
    // The Authorization header is trimmed before it is compared, so a token
    // with leading or trailing whitespace could never match: the daemon
    // would start and refuse every request.
    if token.trim() != token || token.trim().is_empty() {
        eprintln!(
            "{} the API token has leading or trailing whitespace (or is only whitespace), \
             so no request could ever authenticate with it",
            "Error:".red()
        );
        return 1;
    }
    // An HTTP header value is visible ASCII plus spaces. A client cannot
    // send `tökén` or a token with a newline in it, so a daemon started with
    // one listens and refuses every request.
    if let Some(bad) = token.chars().find(|c| !(c.is_ascii_graphic() || *c == ' ')) {
        eprintln!(
            "{} the API token contains {:?}; a token is sent in an HTTP header, which \
             carries only visible ASCII characters and spaces",
            "Error:".red(),
            bad
        );
        return 1;
    }
    // No default. The old one, `./.deciduous/api-data`, made the cwd look
    // like a project: started in a project's subdirectory it created a second
    // `.deciduous/` there, and every CLI call in that directory then found
    // the new, empty one instead of the project's.
    let data_dir = data_dir.or_else(|| {
        std::env::var("DECIDUOUS_API_DATA_DIR")
            .ok()
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
    });
    let Some(data_dir) = data_dir else {
        eprintln!(
            "{} API mode needs a data directory for its graphs: pass --data-dir <dir> \
             or set DECIDUOUS_API_DATA_DIR",
            "Error:".red()
        );
        return 1;
    };
    let config = deciduous::api::ApiConfig {
        bind: bind.clone(),
        port,
        data_dir: data_dir.clone(),
        token,
    };
    match deciduous::api::ApiServer::bind(config) {
        Ok(server) => {
            println!(
                "{} API daemon on http://{}:{} (graphs in {})",
                "Deciduous".cyan(),
                bind,
                server.port(),
                data_dir.display()
            );
            server.run();
            0
        }
        Err(e) => {
            eprintln!("{} API server error: {}", "Error:".red(), e);
            1
        }
    }
}

fn main() {
    let args = Args::parse();

    // Handle init separately - it doesn't need an existing database
    if let Command::Init {
        claude,
        opencode,
        windsurf,
        both,
        no_auto_update,
    } = args.command
    {
        // Determine which assistants to set up
        let (setup_claude, setup_opencode) = if both {
            (true, true)
        } else if opencode && !claude {
            (false, true)
        } else if claude && !opencode {
            (true, false)
        } else if !claude && !opencode {
            // Default: Claude Code only (backward compatible)
            (true, false)
        } else {
            // Both flags specified
            (true, true)
        };

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        if let Err(e) =
            deciduous::init::init_project(setup_claude, setup_opencode, windsurf, no_auto_update)
                .and_then(|_| deciduous::server::ensure(&cwd, deciduous::server::Caller::Init))
        {
            eprintln!("{} {}", "Error:".red(), e);
            exit(1);
        }
        return;
    }

    // `remote setup` may run in a project with no .deciduous yet.
    if let Command::Remote {
        action: RemoteAction::Setup { local, url, port },
    } = &args.command
    {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let choice = match (local, url) {
            (true, _) => deciduous::server::SetupChoice::Local(*port),
            (false, Some(u)) => deciduous::server::SetupChoice::Url(u.clone()),
            // A port without --local still means this machine: nothing else
            // has a port to choose.
            (false, None) if port.is_some() => deciduous::server::SetupChoice::Local(*port),
            (false, None) => deciduous::server::SetupChoice::Ask,
        };
        if let Err(e) = deciduous::server::setup_wizard(&cwd, choice) {
            eprintln!("{} {}", "Error:".red(), e);
            exit(1);
        }
        return;
    }

    // Handle update separately - it doesn't need an existing database
    // Auto-detects which assistants are installed
    if let Command::Update { all } = &args.command {
        match all {
            None => {
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                if let Err(e) = deciduous::init::update_tooling().and_then(|_| {
                    deciduous::server::ensure(&cwd, deciduous::server::Caller::Update)
                }) {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            }
            Some(root) => {
                let failed = deciduous::init::update_all(root);
                if failed > 0 {
                    exit(1);
                }
            }
        }
        return;
    }

    if let Command::MergeRecord { base, ours, theirs } = &args.command {
        // On failure, exit non-zero. Git then does NOT write conflict
        // markers: it keeps our side in the file untouched and marks it
        // unmerged, so the file parses and looks clean. `deciduous sync`
        // (and `sync --check`) look for that unmerged state and finish the
        // merge from git's own three versions.
        match deciduous::records::merge_record_files_with_notes(base, ours, theirs) {
            Ok((merged, notes)) => {
                for note in notes {
                    eprintln!("deciduous merge-record: {}", note);
                }
                if let Err(e) = std::fs::write(ours, merged) {
                    eprintln!("deciduous merge-record: {}", e);
                    exit(1);
                }
                return;
            }
            Err(e) => {
                eprintln!("deciduous merge-record: {}", e);
                exit(1);
            }
        }
    }

    // Handle check-update separately - just compares versions
    if let Command::CheckUpdate {} = args.command {
        let current_version = env!("CARGO_PKG_VERSION");
        let version_file = std::path::Path::new(".deciduous/.version");

        if !version_file.exists() {
            println!(
                "{} No version file found. Run 'deciduous update' to sync integration files.",
                "Update needed:".yellow()
            );
            exit(1);
        }

        let installed_version = match std::fs::read_to_string(version_file) {
            Ok(v) => v.trim().to_string(),
            Err(_) => {
                println!(
                    "{} Could not read version file. Run 'deciduous update'.",
                    "Update needed:".yellow()
                );
                exit(1);
            }
        };

        if installed_version != current_version {
            println!();
            println!(
                "{}",
                "╔════════════════════════════════════════════════════════════════╗"
                    .yellow()
                    .bold()
            );
            println!(
                "{}",
                "║  DECIDUOUS UPDATE AVAILABLE                                    ║"
                    .yellow()
                    .bold()
            );
            println!(
                "{}",
                "╚════════════════════════════════════════════════════════════════╝"
                    .yellow()
                    .bold()
            );
            println!();
            println!(
                "  Integration files: {}  →  Binary: {}",
                installed_version.red(),
                current_version.green()
            );

            // Show what's new
            let releases =
                deciduous::changelog::get_releases_between(&installed_version, current_version);
            if !releases.is_empty() {
                println!();
                println!("{}", "  What's new:".cyan().bold());
                print!("{}", deciduous::changelog::format_releases(&releases));
            }

            println!();
            println!(
                "  Run {} to update integration files.",
                "deciduous update".cyan().bold()
            );
            println!();
            exit(1);
        }

        println!(
            "{} Integration files are up to date (v{}).",
            "OK:".green(),
            current_version
        );
        return;
    }

    // Handle auto-update toggle (deprecated - version checking is now always-on)
    if let Command::AutoUpdate { toggle: _ } = &args.command {
        println!(
            "{} The 'auto-update' command is deprecated. Version checking is now {}.",
            "Note:".yellow(),
            "always-on".green().bold()
        );
        println!("  Checks crates.io once per 24 hours (non-blocking, rate-limited).");
        println!("  Patch updates show a quiet notification.");
        println!("  Minor/major updates show a prominent banner encouraging upgrade.");
        return;
    }

    // Handle MCP server separately - it manages its own database connection
    if let Command::Mcp {} = args.command {
        if let Err(e) = deciduous::mcp::run_server() {
            eprintln!("{} {}", "Error:".red(), e);
            exit(1);
        }
        return;
    }

    // Removed in 1.0.3. Projects set up by 1.0.2 still run it from
    // settings.json on every tool call until their next `deciduous update`,
    // so it succeeds silently, before Database::open.
    if let Command::LogLoop { .. } = &args.command {
        return;
    }

    // Needs no database or project: it builds its own repository elsewhere.
    if let Command::DemoSwarm { args: swarm_args } = &args.command {
        exit(deciduous::demo_swarm::run(swarm_args));
    }

    // Handle completion separately - doesn't need database
    if let Command::Completion { shell } = args.command {
        clap_complete::generate(
            shell,
            &mut Args::command(),
            "deciduous",
            &mut std::io::stdout(),
        );
        return;
    }

    // The API daemon serves the graphs under its data directory and nothing
    // else. Opening the cwd's database first (as every other command does)
    // created .deciduous/deciduous.db wherever it was started, or opened a
    // parent project's database by walking up.
    if let Command::Serve {
        api: true,
        port,
        data_dir,
        token,
        bind,
    } = args.command
    {
        std::process::exit(run_api_daemon(port, data_dir, token, bind));
    }

    // `sync --check` answers "what would sync do" and changes nothing. With
    // no database yet (a fresh clone), opening one to ask would create it,
    // so the check runs against an empty one in a scratch directory
    // instead, which is exactly what `sync` would start from.
    let opened = match &args.command {
        Command::Sync { check: true, .. } if !Database::db_path().exists() => check_scratch_dir()
            .and_then(|dir| Database::open_at(dir.join("deciduous.db")).map_err(|e| e.to_string())),
        _ => Database::open().map_err(|e| e.to_string()),
    };
    let db = match opened {
        Ok(db) => db,
        Err(e) => {
            eprintln!("{} Failed to open database: {}", "Error:".red(), e);
            exit(1);
        }
    };

    // Declared after `db`, so it drops first: whatever this command queued
    // for the server is sent before the process exits.
    let _replay = deciduous::remote::ReplayOnExit;

    match args.command {
        Command::Init { .. } => unreachable!(),   // Handled above
        Command::Update { .. } => unreachable!(), // Handled above
        Command::MergeRecord { .. } => unreachable!(), // Handled above
        Command::CheckUpdate { .. } => unreachable!(), // Handled above
        Command::AutoUpdate { .. } => unreachable!(), // Handled above
        Command::Add {
            node_type,
            title,
            description,
            confidence,
            commit,
            prompt,
            prompt_stdin,
            files,
            branch,
            no_branch,
            date,
        } => {
            // Warn if observation is missing a description
            if node_type == "observation" && description.is_none() {
                eprintln!(
                    "{} Observations should have both a title and a description (-d \"...\").",
                    "Warning:".yellow(),
                );
                eprintln!(
                    "         Use the title for a {} and -d for the full detail.",
                    "short summary".bold()
                );
            }
            // Handle prompt from stdin if requested
            let effective_prompt = if prompt_stdin {
                use std::io::{self, Read};
                let mut buffer = String::new();
                io::stdin().read_to_string(&mut buffer).ok();
                let trimmed = buffer.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            } else {
                prompt
            };

            // Warn if prompt looks like a summary (too short)
            if let Some(ref p) = effective_prompt {
                if p.len() < 200 {
                    eprintln!(
                        "{} Prompt is only {} chars. This looks like a summary, not a full prompt.",
                        "Warning:".yellow(),
                        p.len()
                    );
                    eprintln!(
                        "         Capture the {} user message for better context recovery.",
                        "verbatim".bold()
                    );
                }
            }
            // Auto-detect branch if not specified and not disabled
            let effective_branch = if no_branch {
                None
            } else {
                branch.or_else(deciduous::get_current_git_branch)
            };

            // Any rev git can resolve (HEAD, a branch, origin/main, a hash
            // prefix) is stored as the full hash of its commit; anything
            // else is refused before the node is written.
            let effective_commit = match commit.as_deref().map(deciduous::resolve_git_commit) {
                None => None,
                Some(Ok(hash)) => Some(hash),
                Some(Err(e)) => {
                    eprintln!("{} --commit: {}. Nothing was written.", "Error:".red(), e);
                    exit(1);
                }
            };

            // Parse date parameter into RFC3339 format
            let effective_date = date.as_ref().map(|d| {
                // Try parsing as RFC3339 first
                if chrono::DateTime::parse_from_rfc3339(d).is_ok() {
                    d.clone()
                }
                // Try "YYYY-MM-DD HH:MM:SS" format
                else if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(d, "%Y-%m-%d %H:%M:%S")
                {
                    chrono::Local.from_local_datetime(&dt).unwrap().to_rfc3339()
                }
                // Try "YYYY-MM-DD" format (set to start of day)
                else if let Ok(date) = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") {
                    let dt = date.and_hms_opt(0, 0, 0).unwrap();
                    chrono::Local.from_local_datetime(&dt).unwrap().to_rfc3339()
                }
                // Anything else is refused. It used to be stored as typed,
                // with a warning, and queued for the server, whose POST /ops
                // refuses a created_at that is not a time: the op stayed
                // rejected in the log for good.
                else {
                    eprintln!(
                        "{} could not read --date \"{}\"; use RFC 3339, \"YYYY-MM-DD HH:MM:SS\" or \"YYYY-MM-DD\". Nothing was written.",
                        "Error:".red(),
                        d
                    );
                    std::process::exit(1);
                }
            });

            match db.create_node_full(
                &node_type,
                &title,
                description.as_deref(),
                confidence,
                effective_commit.as_deref(),
                effective_prompt.as_deref(),
                files.as_deref(),
                effective_branch.as_deref(),
                effective_date.as_deref(),
            ) {
                Ok(id) => {
                    let conf_str = confidence
                        .map(|c| format!(" [confidence: {}%]", c))
                        .unwrap_or_default();
                    let commit_str = effective_commit
                        .as_ref()
                        .map(|c| format!(" [commit: {c}]"))
                        .unwrap_or_default();
                    let prompt_str = effective_prompt
                        .as_ref()
                        .map(|p| format!(" [prompt: {} chars]", p.len()))
                        .unwrap_or_default();
                    let files_str = files
                        .as_ref()
                        .map(|f| format!(" [files: {}]", f))
                        .unwrap_or_default();
                    let branch_str = effective_branch
                        .as_ref()
                        .map(|b| format!(" [branch: {}]", b))
                        .unwrap_or_default();
                    let date_str = effective_date
                        .as_ref()
                        .map(|d| format!(" [date: {}]", d))
                        .unwrap_or_default();
                    println!(
                        "{} node {} (type: {}, title: {}){}{}{}{}{}{}",
                        "Created".green(),
                        id,
                        node_type,
                        title,
                        conf_str,
                        commit_str,
                        prompt_str,
                        files_str,
                        branch_str,
                        date_str
                    );
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            }
        }

        Command::Link {
            from,
            to,
            rationale,
            edge_type,
        } => {
            let from_id = resolve_node_or_exit(&db, &from);
            let to_id = resolve_node_or_exit(&db, &to);
            match db.create_edge(from_id, to_id, &edge_type, rationale.as_deref()) {
                Ok(id) => {
                    println!(
                        "{} edge {} ({} -> {} via {})",
                        "Created".green(),
                        id,
                        from_id,
                        to_id,
                        edge_type
                    );
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            }
        }

        Command::Unlink {
            from,
            to,
            edge_type,
        } => {
            let from_id = resolve_node_or_exit(&db, &from);
            let to_id = resolve_node_or_exit(&db, &to);
            match db.delete_edge(from_id, to_id, edge_type.as_deref()) {
                Ok(removed) => {
                    for e in removed {
                        println!(
                            "{} edge ({} -> {}, {})",
                            "Removed".red(),
                            from_id,
                            to_id,
                            e.edge_type
                        );
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            }
        }

        Command::Delete { id, dry_run } => {
            let id = resolve_node_or_exit(&db, &id);
            match db.delete_node(id, dry_run) {
                Ok(summary) => {
                    if dry_run {
                        println!(
                            "{} Would delete node {} ({}) with {} edge(s)",
                            "Dry run:".yellow(),
                            id,
                            summary.node_title,
                            summary.edges_deleted
                        );
                    } else {
                        println!(
                            "{} node {} ({}) and {} edge(s)",
                            "Deleted".red(),
                            id,
                            summary.node_title,
                            summary.edges_deleted
                        );
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            }
        }

        Command::Status { id, status } => {
            let id = resolve_node_or_exit(&db, &id);
            match db.update_node_status(id, &status) {
                Ok(()) => {
                    println!("{} node {} status to '{}'", "Updated".green(), id, status);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            }
        }

        Command::Prompt { id, prompt } => {
            let id = resolve_node_or_exit(&db, &id);
            // Read prompt from stdin if not provided as argument
            let effective_prompt = match prompt {
                Some(p) => p,
                None => {
                    use std::io::{self, Read};
                    let mut buffer = String::new();
                    io::stdin().read_to_string(&mut buffer).ok();
                    buffer.trim().to_string()
                }
            };

            if effective_prompt.is_empty() {
                eprintln!("{} No prompt provided", "Error:".red());
                exit(1);
            }

            // Warn if prompt looks like a summary
            if effective_prompt.len() < 200 {
                eprintln!(
                    "{} Prompt is only {} chars. This looks like a summary, not a full prompt.",
                    "Warning:".yellow(),
                    effective_prompt.len()
                );
                eprintln!(
                    "         Capture the {} user message for better context recovery.",
                    "verbatim".bold()
                );
            }

            match db.update_node_prompt(id, &effective_prompt) {
                Ok(()) => {
                    println!(
                        "{} node {} prompt ({} chars)",
                        "Updated".green(),
                        id,
                        effective_prompt.len()
                    );
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            }
        }

        Command::Nodes {
            branch,
            node_type,
            theme,
        } => {
            // Pre-compute theme node IDs if filtering by theme
            let theme_node_ids: Option<std::collections::HashSet<i32>> = theme.as_ref().map(|t| {
                db.get_nodes_by_theme(t)
                    .unwrap_or_default()
                    .iter()
                    .map(|n| n.id)
                    .collect()
            });

            match db.get_all_nodes() {
                Ok(nodes) => {
                    // Filter nodes by branch, type, and/or theme
                    let filtered: Vec<_> = nodes
                        .into_iter()
                        .filter(|n| {
                            // Filter by branch if specified
                            let branch_match = match &branch {
                                Some(b) => n.metadata_json.as_ref().is_some_and(|meta| {
                                    serde_json::from_str::<serde_json::Value>(meta)
                                        .ok()
                                        .and_then(|v| {
                                            v.get("branch")
                                                .and_then(|br| br.as_str())
                                                .map(|s| s.to_string())
                                        })
                                        .is_some_and(|node_branch| node_branch == *b)
                                }),
                                None => true,
                            };
                            // Filter by type if specified
                            let type_match = match &node_type {
                                Some(t) => n.node_type == *t,
                                None => true,
                            };
                            // Filter by theme if specified
                            let theme_match = match &theme_node_ids {
                                Some(ids) => ids.contains(&n.id),
                                None => true,
                            };
                            branch_match && type_match && theme_match
                        })
                        .collect();

                    if filtered.is_empty() {
                        if branch.is_some() || node_type.is_some() {
                            println!("No nodes found matching filters.");
                        } else {
                            println!(
                                "No nodes found. Add one with: deciduous add goal \"My goal\""
                            );
                        }
                    } else {
                        let header = match &branch {
                            Some(b) => {
                                format!("Nodes on branch '{}' ({} total):", b, filtered.len())
                            }
                            None => format!("{} nodes:", filtered.len()),
                        };
                        println!("{}", header.cyan());
                        println!(
                            "{:<5} {:<9} {:<12} {:<10} TITLE",
                            "ID", "CHANGE", "TYPE", "STATUS"
                        );
                        println!("{}", "-".repeat(78));
                        for n in filtered {
                            let short_cid = n.change_id.get(..8).unwrap_or(&n.change_id).dimmed();
                            let type_colored = match n.node_type.as_str() {
                                "goal" => n.node_type.yellow(),
                                "decision" => n.node_type.cyan(),
                                "action" => n.node_type.green(),
                                "outcome" => n.node_type.blue(),
                                "observation" => n.node_type.magenta(),
                                "revisit" => n.node_type.truecolor(249, 115, 22), // Orange
                                _ => n.node_type.white(),
                            };
                            if n.node_type == "observation" {
                                if let Some(ref desc) = n.description {
                                    // Character-aware: a byte slice panics on
                                    // multibyte text such as an ellipsis.
                                    let truncated = truncate(desc, 80);
                                    println!(
                                        "{:<5} {:<9} {:<12} {:<10} {}",
                                        n.id, short_cid, type_colored, n.status, n.title
                                    );
                                    println!("{:<39}{}", "", truncated.dimmed());
                                } else {
                                    println!(
                                        "{:<5} {:<9} {:<12} {:<10} {}",
                                        n.id, short_cid, type_colored, n.status, n.title
                                    );
                                }
                            } else {
                                println!(
                                    "{:<5} {:<9} {:<12} {:<10} {}",
                                    n.id, short_cid, type_colored, n.status, n.title
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            }
        }

        Command::Edges => match db.get_all_edges() {
            Ok(edges) => {
                if edges.is_empty() {
                    println!("No edges found. Link nodes with: deciduous link 1 2 -r \"reason\"");
                } else {
                    println!(
                        "{:<5} {:<6} {:<6} {:<12} RATIONALE",
                        "ID", "FROM", "TO", "TYPE"
                    );
                    println!("{}", "-".repeat(70));
                    for e in edges {
                        println!(
                            "{:<5} {:<6} {:<6} {:<12} {}",
                            e.id,
                            e.from_node_id,
                            e.to_node_id,
                            e.edge_type,
                            e.rationale.unwrap_or_default()
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                exit(1);
            }
        },

        Command::Show { id, json } => {
            let id = resolve_node_or_exit(&db, &id);
            match db.get_node(id) {
                Ok(Some(node)) => {
                    if json {
                        // JSON output mode
                        match serde_json::to_string_pretty(&node) {
                            Ok(json_str) => println!("{}", json_str),
                            Err(e) => {
                                eprintln!("{} Serializing node: {}", "Error:".red(), e);
                                exit(1);
                            }
                        }
                    } else {
                        // Formatted output mode
                        let type_colored = match node.node_type.as_str() {
                            "goal" => node.node_type.yellow().bold(),
                            "decision" => node.node_type.cyan().bold(),
                            "action" => node.node_type.green().bold(),
                            "outcome" => node.node_type.blue().bold(),
                            "observation" => node.node_type.magenta().bold(),
                            "option" => node.node_type.white().bold(),
                            "revisit" => node.node_type.truecolor(249, 115, 22).bold(), // Orange
                            _ => node.node_type.white().bold(),
                        };

                        println!();
                        println!(
                            "{} {} {}",
                            "Node".bold(),
                            format!("#{}", id).cyan(),
                            type_colored
                        );
                        println!("{}", "─".repeat(60));
                        println!("{}: {}", "Title".bold(), node.title);

                        if let Some(desc) = &node.description {
                            println!("{}: {}", "Description".bold(), desc);
                        }

                        println!("{}: {}", "Status".bold(), node.status);
                        println!("{}: {}", "Change ID".bold(), node.change_id.dimmed());
                        println!("{}: {}", "Created".bold(), node.created_at);
                        println!("{}: {}", "Updated".bold(), node.updated_at);

                        // Parse metadata
                        if let Some(ref meta_str) = node.metadata_json {
                            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_str) {
                                println!();
                                println!("{}", "Metadata".bold().underline());

                                if let Some(conf) = meta.get("confidence").and_then(|v| v.as_i64())
                                {
                                    let conf_colored = if conf >= 80 {
                                        format!("{}%", conf).green()
                                    } else if conf >= 50 {
                                        format!("{}%", conf).yellow()
                                    } else {
                                        format!("{}%", conf).red()
                                    };
                                    println!("  {}: {}", "Confidence".bold(), conf_colored);
                                }

                                if let Some(branch) = meta.get("branch").and_then(|v| v.as_str()) {
                                    println!("  {}: {}", "Branch".bold(), branch.cyan());
                                }

                                if let Some(commit) = meta.get("commit").and_then(|v| v.as_str()) {
                                    println!("  {}: {}", "Commit".bold(), commit.yellow());
                                }

                                if let Some(files) = meta.get("files").and_then(|v| v.as_array()) {
                                    let file_list: Vec<&str> =
                                        files.iter().filter_map(|f| f.as_str()).collect();
                                    if !file_list.is_empty() {
                                        println!("  {}: {}", "Files".bold(), file_list.join(", "));
                                    }
                                }

                                if let Some(prompt) = meta.get("prompt").and_then(|v| v.as_str()) {
                                    println!();
                                    println!("{}", "Prompt".bold().underline());
                                    // Word-wrap long prompts
                                    for line in prompt.lines() {
                                        println!("  {}", line.italic());
                                    }
                                }
                            }
                        }

                        // Get edges
                        if let Ok(edges) = db.get_all_edges() {
                            let incoming: Vec<_> =
                                edges.iter().filter(|e| e.to_node_id == id).collect();
                            let outgoing: Vec<_> =
                                edges.iter().filter(|e| e.from_node_id == id).collect();

                            if !incoming.is_empty() || !outgoing.is_empty() {
                                println!();
                                println!("{}", "Connections".bold().underline());
                            }

                            if !incoming.is_empty() {
                                println!("  {} ({}):", "Incoming".bold(), incoming.len());
                                for edge in incoming {
                                    let rationale = edge.rationale.as_deref().unwrap_or("");
                                    let edge_type = match edge.edge_type.as_str() {
                                        "chosen" => edge.edge_type.green(),
                                        "rejected" => edge.edge_type.red(),
                                        _ => edge.edge_type.white(),
                                    };
                                    if rationale.is_empty() {
                                        println!(
                                            "    #{} ─[{}]→ here",
                                            edge.from_node_id, edge_type
                                        );
                                    } else {
                                        println!(
                                            "    #{} ─[{}]→ here: {}",
                                            edge.from_node_id,
                                            edge_type,
                                            rationale.dimmed()
                                        );
                                    }
                                }
                            }

                            if !outgoing.is_empty() {
                                println!("  {} ({}):", "Outgoing".bold(), outgoing.len());
                                for edge in outgoing {
                                    let rationale = edge.rationale.as_deref().unwrap_or("");
                                    let edge_type = match edge.edge_type.as_str() {
                                        "chosen" => edge.edge_type.green(),
                                        "rejected" => edge.edge_type.red(),
                                        _ => edge.edge_type.white(),
                                    };
                                    if rationale.is_empty() {
                                        println!("    here ─[{}]→ #{}", edge_type, edge.to_node_id);
                                    } else {
                                        println!(
                                            "    here ─[{}]→ #{}: {}",
                                            edge_type,
                                            edge.to_node_id,
                                            rationale.dimmed()
                                        );
                                    }
                                }
                            }
                        }

                        println!();
                    }
                }
                Ok(None) => {
                    eprintln!("{} Node #{} not found", "Error:".red(), id);
                    exit(1);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            }
        }

        Command::Graph => match db.get_graph() {
            Ok(graph) => match serde_json::to_string_pretty(&graph) {
                Ok(json) => println!("{}", json),
                Err(e) => {
                    eprintln!("{} Serializing graph: {}", "Error:".red(), e);
                    exit(1);
                }
            },
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                exit(1);
            }
        },

        Command::Serve { port, api, .. } => {
            if api {
                unreachable!("serve --api is handled before the database is opened");
            } else {
                println!(
                    "{} Starting graph viewer at http://localhost:{}",
                    "Deciduous".cyan(),
                    port
                );
                if let Err(e) = deciduous::serve::start_graph_server(port) {
                    eprintln!("{} Server error: {}", "Error:".red(), e);
                    exit(1);
                }
            }
        }

        Command::Remote { action } => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

            match action {
                RemoteAction::Setup { .. } => unreachable!(), // Handled before the database opens
                RemoteAction::Login { url } => {
                    // stdin, not an argument: a token passed on the command
                    // line lands in shell history and in the process list,
                    // where every other user on the machine can read it. This
                    // does not stop the terminal echoing what is typed — it
                    // keeps the token out of argv, which is the part that
                    // persists.
                    eprint!("Token (read from stdin, not saved to shell history): ");
                    let mut token = String::new();
                    if std::io::stdin().read_line(&mut token).is_err() {
                        eprintln!("{} could not read the token", "Error:".red());
                        exit(1);
                    }

                    if let Some(url) = url {
                        let mut cfg = Config::load();
                        cfg.remote.url = Some(url.trim_end_matches('/').to_string());
                        std::env::set_var(deciduous::remote::TOKEN_ENV, token.trim());

                        match deciduous::remote::Remote::resolve(&cfg, &cwd).and_then(|r| r.check())
                        {
                            Ok(_) => {}
                            Err(e) => {
                                eprintln!("{} {}", "Error:".red(), e);
                                eprintln!("\nNothing was stored.");
                                exit(1);
                            }
                        }
                    }

                    match deciduous::remote::store_token(&token) {
                        Ok(path) => {
                            println!("{} stored in {}", "Token:".green(), path.display());
                            println!("  mode 0600, outside every repository");
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    }
                }

                RemoteAction::Logout => match deciduous::remote::forget_token() {
                    Ok(true) => println!("{} stored token removed.", "Logged out:".green()),
                    Ok(false) => println!("No stored token to remove."),
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                },

                RemoteAction::Adopt { root, url, dry_run } => {
                    let url = url.trim_end_matches('/').to_string();
                    let root = PathBuf::from(&root);

                    let projects = deciduous::remote::find_projects(&root);
                    if projects.is_empty() {
                        println!(
                            "No projects with a .deciduous directory under {}",
                            root.display()
                        );
                        return;
                    }

                    println!(
                        "{} {} projects under {}\n",
                        if dry_run {
                            "Would configure:"
                        } else {
                            "Configuring:"
                        },
                        projects.len(),
                        root.display()
                    );

                    let mut changed = 0;
                    let mut skipped = 0;
                    for project in &projects {
                        let ws = deciduous::remote::workspace_for(project);
                        let existing = deciduous::remote::read_remote_url(project);

                        // Several directories can share one workspace name:
                        // everything outside a git repository pools into
                        // `scratch`. Printing only the name makes those rows
                        // indistinguishable, so show the directory too.
                        let label = {
                            let dir = project
                                .strip_prefix(&root)
                                .unwrap_or(project)
                                .display()
                                .to_string();
                            let dir = if dir.is_empty() { ".".to_string() } else { dir };
                            if dir.to_lowercase() == ws {
                                ws.clone()
                            } else {
                                format!("{ws}  ({dir})")
                            }
                        };

                        match existing {
                            Some(u) if u == url => {
                                println!("  {:<44} {}", label, "already configured".dimmed());
                                skipped += 1;
                            }
                            Some(u) => {
                                // Repointing a project at a different server
                                // would move where its decisions land; that is
                                // the user's call, not a bulk operation's.
                                println!("  {:<28} {} {}", ws, "points elsewhere:".yellow(), u);
                                skipped += 1;
                            }
                            None => {
                                if dry_run {
                                    println!("  {:<44} would configure", label);
                                } else {
                                    match deciduous::remote::write_remote_url(project, &url) {
                                        Ok(()) => {
                                            println!("  {:<44} {}", label, "configured".green())
                                        }
                                        Err(e) => {
                                            println!("  {:<44} {} {}", label, "failed:".red(), e)
                                        }
                                    }
                                }
                                changed += 1;
                            }
                        }
                    }

                    // "N configured" during a dry run is a lie about work that
                    // did not happen, and checking before touching 90
                    // repositories is this command's entire purpose.
                    println!(
                        "\n{} {}, {} left alone",
                        changed,
                        if dry_run {
                            "would be configured"
                        } else {
                            "configured"
                        },
                        skipped
                    );
                    if !dry_run && changed > 0 {
                        println!("Each project's .deciduous/config.toml now holds the URL. The token stays in the credentials file.");
                    }
                }

                RemoteAction::Init { url, workspace } => {
                    let url = url.trim_end_matches('/').to_string();
                    let ws = workspace
                        .clone()
                        .unwrap_or_else(|| deciduous::remote::workspace_for(&cwd));

                    // Verified before it is written. Saving a URL that does not
                    // answer leaves a project configured to talk to nothing,
                    // and the failure only surfaces on the next real command.
                    let mut cfg = Config::load();
                    cfg.remote.url = Some(url.clone());
                    // Recorded even when derived: the name is decided once,
                    // here, not re-derived from the directory on every call.
                    cfg.remote.workspace = Some(ws.clone());

                    let remote = match deciduous::remote::Remote::resolve(&cfg, &cwd) {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    // Before anything is written: a workspace another
                    // repository owns is refused here, not discovered later
                    // as someone else's nodes in this project's graph.
                    let claim = remote
                        .health()
                        .and_then(|_| remote.claim(workspace.is_some()));
                    let claim = match claim {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            eprintln!("\nNothing was written; the project is unchanged.");
                            exit(1);
                        }
                    };

                    match remote.check() {
                        Ok(counts) => {
                            if let Err(e) = cfg.save_remote() {
                                eprintln!("{} could not write config: {}", "Error:".red(), e);
                                exit(1);
                            }
                            println!("{} {}", "Remote:".green(), url);
                            println!("  workspace: {}", ws.cyan());
                            if let Some(why) = remote.unchecked_note() {
                                println!("  {} {why}", "note:".yellow());
                            } else if claim == "unchecked" {
                                println!(
                                    "  {} the server did not check this repository's claim to {ws}",
                                    "note:".yellow()
                                );
                            } else {
                                println!("  claim: {claim} (by this repository's root commit)");
                            }
                            println!(
                                "  server holds {} nodes, {} edges, {} documents",
                                counts.nodes, counts.edges, counts.documents
                            );

                            // History written before the remote existed has no
                            // ops in the log (the log starts now), so it is
                            // sent once here, insert-only: a row the server
                            // already has is never replaced.
                            let seeded = db
                                .get_graph()
                                .map_err(|e| format!("reading the local graph: {e}"))
                                .and_then(|g| {
                                    serde_json::to_value(&g)
                                        .map_err(|e| format!("serializing the local graph: {e}"))
                                })
                                .and_then(|g| deciduous::remote::push_missing(&remote, &g));
                            if let Ok((_, _, withheld)) = &seeded {
                                deciduous::remote::print_withheld(withheld);
                            }
                            match seeded {
                                Ok((None, _, _)) => {}
                                Ok((Some(r), _, _)) => println!(
                                    "  sent {} local node(s), {} edge(s) the server lacked",
                                    r.nodes.upserted, r.edges.upserted
                                ),
                                Err(e) => {
                                    eprintln!(
                                        "{} the remote is configured, but the local history was not sent: {e}\n\
                                         Send it with `deciduous remote push --seed`.",
                                        "Warning:".yellow()
                                    );
                                }
                            }
                            println!(
                                "\nWritten to .deciduous/config.toml. The token stays in {}.",
                                deciduous::remote::TOKEN_ENV
                            );
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            eprintln!("\nNothing was written; the project is unchanged.");
                            exit(1);
                        }
                    }
                }

                RemoteAction::Status => {
                    // The database's project, which is the cwd's unless
                    // DECIDUOUS_DB_PATH says otherwise.
                    let cfg = match deciduous::remote::config_at(db.data_dir()) {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    };
                    if !cfg.remote.is_configured() {
                        println!(
                            "{} no remote configured for this project.",
                            "Local only:".yellow()
                        );
                        println!("\n    deciduous remote init <url>");
                        return;
                    }

                    let remote = match deciduous::remote::Remote::for_data_dir(db.data_dir()) {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    println!("{} {}", "Remote:".bold(), remote.url);
                    println!("{} {}", "Workspace:".bold(), remote.workspace.cyan());

                    // The queue first: it is this machine's half of any
                    // difference below, and it needs no server to read.
                    let log_state = match db.oplog() {
                        Some(log) => match log.read() {
                            Ok(st) => Some((log, st)),
                            Err(e) => {
                                eprintln!("{} {}", "Error:".red(), e);
                                exit(1);
                            }
                        },
                        None => None,
                    };
                    let (waiting, rejected) = log_state
                        .as_ref()
                        .map(|(_, st)| (st.pending.len(), st.rejected.len()))
                        .unwrap_or((0, 0));
                    let set_aside = log_state.as_ref().map_or(0, |(_, st)| {
                        st.rejected.iter().filter(|(_, a)| a.is_set_aside()).count()
                    });

                    let (nodes, edges) = match (db.get_all_nodes(), db.get_all_edges()) {
                        (Ok(n), Ok(e)) => (n, e),
                        (Err(e), _) | (_, Err(e)) => {
                            eprintln!("{} reading the local graph: {}", "Error:".red(), e);
                            exit(1);
                        }
                    };
                    // Health first, so a down server and a wrong token
                    // read as the different problems they are. Read before
                    // the log is printed, so its count leaves out refusals
                    // that are settled; a server that cannot be read is
                    // reported after the log, which needs no server.
                    let server = remote.health().and_then(|_| {
                        remote
                            .export()
                            .map_err(|e| format!("reached {} but {}", remote.url, e))
                    });
                    // Refusals whose rows now agree: listed, not counted.
                    // The header said "1 rejected" above "In sync" (round-2
                    // verification of chapter 29).
                    let settled: std::collections::HashSet<String> = match (&log_state, &server) {
                        (Some((_, st)), Ok(g)) => {
                            deciduous::remote::settled_refusals(&st.rejected, &nodes, &edges, g)
                                .into_iter()
                                .map(|o| o.op_id)
                                .collect()
                        }
                        _ => Default::default(),
                    };
                    if let Some((log, st)) = &log_state {
                        println!(
                            "{} {} write(s) waiting, {} rejected{}  ({})",
                            "Log:".bold(),
                            waiting,
                            rejected - set_aside - settled.len(),
                            if set_aside > 0 {
                                format!(
                                    ", {set_aside} set aside by this machine \
                                     (`deciduous remote push` sends them again)"
                                )
                            } else {
                                String::new()
                            },
                            log.path().display()
                        );
                        for op in st.pending.iter().take(20) {
                            println!(
                                "  waiting   {}  {}",
                                op.op_id.chars().take(8).collect::<String>(),
                                op.body.describe()
                            );
                        }
                        if st.pending.len() > 20 {
                            println!("  ... and {} more", st.pending.len() - 20);
                        }
                        for (op, ack) in &st.rejected {
                            println!(
                                "  {}  {}  {}  {}",
                                if ack.is_set_aside() {
                                    "set aside".red()
                                } else if settled.contains(&op.op_id) {
                                    "settled ".green()
                                } else {
                                    "rejected".red()
                                },
                                op.op_id.chars().take(8).collect::<String>(),
                                op.body.describe(),
                                ack.reason.as_deref().unwrap_or("").dimmed()
                            );
                        }
                        for u in &st.unreadable {
                            println!("  {}  {}", "unreadable".red(), u.describe());
                        }
                        if st.set_aside > 0 {
                            println!(
                                "  {} {} line(s) that were not log entries are in {}; \
                                 repair and re-append any that is a write, then delete that file",
                                "unreadable".red(),
                                st.set_aside,
                                log.unreadable_path().display()
                            );
                        }
                    }
                    let damaged = log_state
                        .as_ref()
                        .is_some_and(|(_, st)| !st.unreadable.is_empty() || st.set_aside > 0);

                    let server = match server {
                        Ok(g) => g,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    };
                    if let Err(e) = remote.claim(false) {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                    let d = deciduous::remote::content_diff(&nodes, &edges, &server);
                    if !settled.is_empty() {
                        println!(
                            "  {} {} refusal(s) above: this copy and the server now agree on \
                             what they wrote; the next `deciduous remote push` or `pull` drops them",
                            "settled".green(),
                            settled.len()
                        );
                    }
                    let rejected = rejected - settled.len();
                    let docs_here = match db.get_node_documents(None, false) {
                        Ok(docs) => deciduous::remote::documents_only_here(&docs, &server),
                        Err(e) => {
                            eprintln!("{} reading local documents: {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    println!("\n              {:>8}  {:>8}", "local", "server");
                    println!("  nodes       {:>8}  {:>8}", d.local_nodes, d.server_nodes);
                    println!("  edges       {:>8}  {:>8}", d.local_edges, d.server_edges);

                    let create_of = |op: &deciduous::oplog::Op| match &op.body {
                        deciduous::oplog::OpBody::CreateNode { change_id, .. } => {
                            Some(change_id.clone())
                        }
                        _ => None,
                    };
                    let queued: std::collections::HashSet<String> = log_state
                        .as_ref()
                        .map(|(_, st)| st.pending.iter().filter_map(create_of).collect())
                        .unwrap_or_default();
                    // A create in the log with an answer that is not a
                    // delivery: "no op covers it" said the opposite of the
                    // line above it.
                    let held: std::collections::HashMap<String, (String, bool)> = log_state
                        .as_ref()
                        .map(|(_, st)| {
                            st.rejected
                                .iter()
                                .filter_map(|(op, a)| {
                                    Some((
                                        create_of(op)?,
                                        (op.op_id.chars().take(8).collect(), a.is_set_aside()),
                                    ))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let nul_at: std::collections::HashMap<String, String> = nodes
                        .iter()
                        .filter_map(|n| {
                            let at = deciduous::remote::row_nul(&serde_json::to_value(n).ok()?)?;
                            Some((n.change_id.clone(), at))
                        })
                        .collect();
                    let list = |title: &str, rows: Vec<String>| {
                        if rows.is_empty() {
                            return;
                        }
                        println!("\n{} ({})", title.bold(), rows.len());
                        for r in rows.iter().take(20) {
                            println!("  {r}");
                        }
                        if rows.len() > 20 {
                            println!("  ... and {} more", rows.len() - 20);
                        }
                    };
                    list(
                        "Only here",
                        d.only_local
                            .iter()
                            .map(|(cid, l)| {
                                if queued.contains(cid) {
                                    format!("{l}  (waiting in the log)")
                                } else if let Some(at) = nul_at.get(cid) {
                                    format!(
                                        "{l}  (holds a NUL character at {at}, which the server cannot \
                                         store; nothing sends it until that text is changed here)"
                                    )
                                } else if let Some((id, aside)) = held.get(cid) {
                                    format!(
                                        "{l}  (its op {id} is {} above)",
                                        if *aside {
                                            "set aside by this machine: listed"
                                        } else {
                                            "rejected by the server: listed"
                                        }
                                    )
                                } else {
                                    format!(
                                        "{l}  (no op covers it: `deciduous remote push --seed`)"
                                    )
                                }
                            })
                            .collect(),
                    );
                    list(
                        "Deleted on the server",
                        d.deleted_on_server
                            .iter()
                            .map(|(_, l)| format!("{l}  (`deciduous remote pull` removes it here)"))
                            .collect(),
                    );
                    // Deleted here with no op to say so: only --repair
                    // sends such a delete (round-2 BRIDGE-N2, team T4).
                    let unsent_deletes: std::collections::HashSet<String> = {
                        let store = RecordStore::path_for_db(&Database::db_path())
                            .and_then(|p| RecordStore::open(&p));
                        match (store, &log_state) {
                            (Some(store), Some((_, st))) => {
                                match deciduous::remote::repair_deletes(&nodes, &store, &server, st)
                                {
                                    Ok(ops) => ops
                                        .iter()
                                        .flat_map(|o| o.change_ids())
                                        .map(str::to_string)
                                        .collect(),
                                    Err(e) => {
                                        eprintln!("{} {}", "Error:".red(), e);
                                        exit(1);
                                    }
                                }
                            }
                            _ => Default::default(),
                        }
                    };
                    list(
                        "Only on the server",
                        d.only_server
                            .iter()
                            .map(|(cid, l)| {
                                if unsent_deletes.contains(cid) {
                                    format!(
                                        "{l}  (deleted here, and the delete never reached the server: \
                                         `deciduous remote push --repair` sends it)"
                                    )
                                } else {
                                    l.clone()
                                }
                            })
                            .collect(),
                    );
                    let base = match deciduous::remote::known_server_state(
                        &db,
                        log_state.as_ref().map(|(l, _)| l),
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    };
                    let server_by_cid: std::collections::HashMap<
                        &str,
                        &deciduous::remote::RemoteNode,
                    > = server
                        .nodes
                        .iter()
                        .filter(|n| n.deleted_at.is_none())
                        .map(|n| (n.change_id.as_str(), n))
                        .collect();
                    let node_by_cid: std::collections::HashMap<&str, &deciduous::db::DecisionNode> =
                        nodes.iter().map(|n| (n.change_id.as_str(), n)).collect();
                    let refused_nodes: std::collections::HashSet<String> = log_state
                        .as_ref()
                        .map(|(_, st)| {
                            st.rejected
                                .iter()
                                .filter(|(op, a)| !a.is_set_aside() && !settled.contains(&op.op_id))
                                .filter(|(op, _)| {
                                    matches!(op.body, deciduous::oplog::OpBody::UpdateNode { .. })
                                })
                                .flat_map(|(op, _)| op.body.change_ids())
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    let side = |cid: &str| -> deciduous::remote::Moved {
                        match (node_by_cid.get(cid), server_by_cid.get(cid)) {
                            (Some(n), Some(sn)) => deciduous::remote::moved(n, sn, base.get(cid)),
                            _ => deciduous::remote::Moved::Unknown,
                        }
                    };
                    list(
                        "Different",
                        d.differ
                            .iter()
                            .map(|nd| {
                                let f: Vec<String> = nd
                                    .fields
                                    .iter()
                                    .map(|(k, here, there)| {
                                        format!("{k}: here {here}, server {there}")
                                    })
                                    .collect();
                                let why = if refused_nodes.contains(&nd.change_id) {
                                    "the server refused this copy's edit: `deciduous remote pull` takes the server's value"
                                } else {
                                    match side(&nd.change_id) {
                                        deciduous::remote::Moved::Here => {
                                            "edited here since the last pull: `deciduous remote push --repair` sends it"
                                        }
                                        deciduous::remote::Moved::Server => {
                                            "changed on the server since the last pull: `deciduous remote pull` takes it"
                                        }
                                        deciduous::remote::Moved::Unknown => {
                                            "this copy never pulled it, so which side changed is unknown: \
                                             `deciduous remote pull` takes the server's; \
                                             `deciduous remote push --repair --overwrite-server` sends this copy's"
                                        }
                                    }
                                };
                                format!(
                                    "{} \"{}\"  {}  ({})",
                                    nd.change_id.chars().take(8).collect::<String>(),
                                    nd.title,
                                    f.join("; "),
                                    why
                                )
                            })
                            .collect(),
                    );
                    list("Edges only here", d.edges_only_local.clone());
                    list("Edges only on the server", d.edges_only_server.clone());
                    list(
                        "Documents only here",
                        docs_here
                            .iter()
                            .map(|l| format!("{l}  (`deciduous remote push --seed` sends it)"))
                            .collect(),
                    );
                    // Detached here, still attached on the server, and no
                    // detach waiting: a pasted secret would sit there unseen.
                    let docs_detached = match (db.get_node_documents(None, true), &log_state) {
                        (Ok(docs), Some((_, st))) => {
                            deciduous::remote::repair_detaches(&docs, &server, st)
                        }
                        (Ok(_), None) => Vec::new(),
                        (Err(e), _) => {
                            eprintln!("{} reading local documents: {}", "Error:".red(), e);
                            exit(1);
                        }
                    };
                    list(
                        "Documents detached here, still on the server",
                        docs_detached
                            .iter()
                            .map(|op| {
                                format!(
                                    "{}  (`deciduous remote push --repair` sends the detach)",
                                    op.describe()
                                )
                            })
                            .collect(),
                    );

                    if d.is_empty()
                        && docs_here.is_empty()
                        && docs_detached.is_empty()
                        && waiting == 0
                        && rejected == 0
                        && !damaged
                    {
                        println!(
                            "\n{} no writes waiting, and every node, edge and document matches field by field.\n\
                             Themes and tags are not sent to the server, so they are not compared.",
                            "In sync:".green()
                        );
                    } else {
                        // Only the remedies for what does differ: with only
                        // an unreadable-lines file, this listed push, --seed,
                        // --repair and pull, none of which touches it.
                        let mut todo = Vec::new();
                        if waiting > 0 || set_aside > 0 {
                            todo.push("`deciduous remote push` sends what is waiting and what this machine set aside".to_string());
                        }
                        if rejected > set_aside {
                            todo.push("`deciduous remote push --drop-rejected` discards the server's refusals, once read".to_string());
                        }
                        let unlogged = d.only_local.iter().any(|(cid, _)| {
                            !queued.contains(cid)
                                && !held.contains_key(cid)
                                && !nul_at.contains_key(cid)
                        });
                        if unlogged || !docs_here.is_empty() || !d.edges_only_local.is_empty() {
                            todo.push(
                                "`deciduous remote push --seed` adds what only this copy has"
                                    .to_string(),
                            );
                        }
                        let here_moved = d.differ.iter().any(|nd| {
                            !refused_nodes.contains(&nd.change_id)
                                && side(&nd.change_id) == deciduous::remote::Moved::Here
                        });
                        if here_moved || !unsent_deletes.is_empty() || !docs_detached.is_empty() {
                            todo.push("`deciduous remote push --repair` sends the edits and deletes made here that never became ops".to_string());
                        }
                        if d.only_server
                            .iter()
                            .any(|(c, _)| !unsent_deletes.contains(c))
                            || !d.deleted_on_server.is_empty()
                            || !d.edges_only_server.is_empty()
                            || d.differ.iter().any(|nd| {
                                refused_nodes.contains(&nd.change_id)
                                    || side(&nd.change_id) != deciduous::remote::Moved::Here
                            })
                        {
                            todo.push(
                                "`deciduous remote pull` takes the server's side".to_string(),
                            );
                        }
                        if !nul_at.is_empty() {
                            todo.push("a node holding a NUL has to be changed here before anything can send it".to_string());
                        }
                        if damaged {
                            if let Some((log, _)) = &log_state {
                                todo.push(format!(
                                    "lines of the log that are not entries are listed above: no push or pull \
                                     changes them; repair and re-append any that is a write, then delete {}",
                                    log.unreadable_path().display()
                                ));
                            }
                        }
                        if todo.is_empty() {
                            todo.push("`deciduous remote push` sends what this copy has; `deciduous remote pull` takes the server's side".to_string());
                        }
                        println!("\n{} {}.", "Differs:".yellow(), todo.join("; "));
                        // Scripts read drift from the exit code, like `sync --check`.
                        exit(1);
                    }
                }

                RemoteAction::Push {
                    overwrite,
                    drop_rejected,
                    retry_rejected,
                    drop,
                    seed,
                    repair,
                    overwrite_server,
                } => {
                    let remote = match deciduous::remote::Remote::for_data_dir(db.data_dir()) {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    };
                    let Some(log) = db.oplog() else {
                        eprintln!(
                            "{} this database has no server log: its .deciduous/config.toml has no [remote] url \
                             (DECIDUOUS_DB_PATH may point at another project's database).",
                            "Error:".red()
                        );
                        exit(1);
                    };

                    if drop_rejected {
                        match log.drop_rejected() {
                            Ok(n) => {
                                println!(
                                    "{} {} rejected op(s) from {}",
                                    "Dropped".yellow(),
                                    n,
                                    log.path().display()
                                );
                                let kept = log.read().map_or(0, |st| {
                                    st.rejected.iter().filter(|(_, a)| a.is_set_aside()).count()
                                });
                                if kept > 0 {
                                    println!(
                                        "{} {kept} op(s) this machine set aside: the server never \
                                         refused them. `deciduous remote push` sends them again; \
                                         `--drop <op id>` discards one.",
                                        "Kept".yellow()
                                    );
                                }
                            }
                            Err(e) => {
                                eprintln!("{} {}", "Error:".red(), e);
                                exit(1);
                            }
                        }
                        return;
                    }

                    if let Some(prefix) = drop {
                        match log.drop_op(&prefix) {
                            Ok(op) => println!(
                                "{} {} {} from {}",
                                "Dropped".yellow(),
                                &op.op_id,
                                op.body.describe(),
                                log.path().display()
                            ),
                            Err(e) => {
                                eprintln!("{} {}", "Error:".red(), e);
                                exit(1);
                            }
                        }
                        return;
                    }

                    if retry_rejected {
                        match log.retry_rejected() {
                            Ok(n) => {
                                println!("{} {} rejected op(s) to send again", "Queued".yellow(), n)
                            }
                            Err(e) => {
                                eprintln!("{} {}", "Error:".red(), e);
                                exit(1);
                            }
                        }
                    }

                    // What this machine set aside is a real write the
                    // server never judged: an explicit push tries it again.
                    if !retry_rejected {
                        match log.retry_set_aside() {
                            Ok(0) => {}
                            Ok(n) => println!(
                                "{} {n} write(s) this machine had set aside",
                                "Sending again".yellow()
                            ),
                            Err(e) => {
                                eprintln!("{} {}", "Error:".red(), e);
                                exit(1);
                            }
                        }
                    }

                    let mut undelivered = false;
                    let mut refused = 0usize;
                    // Refusals this run printed, so the ones still standing
                    // from earlier are named apart.
                    let mut shown_now: Vec<String> = Vec::new();
                    match deciduous::remote::replay(&remote, &log) {
                        Ok(r) if r.sent == 0 => println!(
                            "{} no writes are waiting in {}",
                            "Nothing to push:".green(),
                            log.path().display()
                        ),
                        Ok(r) => {
                            let aside = r
                                .rejected
                                .iter()
                                .filter(|(_, why)| deciduous::remote::is_set_aside(why))
                                .count();
                            println!(
                                "{} {} op(s) to {}: {} applied, {} already there, {} rejected{}",
                                "Pushed".green(),
                                r.sent,
                                remote.workspace.cyan(),
                                r.applied,
                                r.already,
                                r.rejected.len() - aside,
                                if aside > 0 {
                                    format!(", {aside} set aside by this machine")
                                } else {
                                    String::new()
                                }
                            );
                            deciduous::remote::print_rejected(&r.rejected, &log);
                            undelivered = aside > 0;
                            refused = own_refusals(&r.rejected);
                            shown_now.extend(r.rejected.iter().map(|(op, _)| op.op_id.clone()));
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            match log.read() {
                                Ok(s) => eprintln!(
                                    "{} write(s) still waiting in {}.",
                                    s.pending.len(),
                                    log.path().display()
                                ),
                                Err(e) => eprintln!(
                                    "{} could not be read to count what is waiting: {e}",
                                    log.path().display()
                                ),
                            }
                            exit(1);
                        }
                    }

                    let mut withheld_repair = false;
                    if repair {
                        let (nodes, server) = match (db.get_all_nodes(), remote.export()) {
                            (Ok(n), Ok(g)) => (n, g),
                            (Err(e), _) => {
                                eprintln!("{} reading the local graph: {}", "Error:".red(), e);
                                exit(1);
                            }
                            (_, Err(e)) => {
                                eprintln!("{} {}", "Error:".red(), e);
                                exit(1);
                            }
                        };
                        let base = match deciduous::remote::known_server_state(&db, Some(&log)) {
                            Ok(b) => b,
                            Err(e) => {
                                eprintln!("{} {}", "Error:".red(), e);
                                exit(1);
                            }
                        };
                        let mut plan =
                            deciduous::remote::repair_ops(&nodes, &server, &base, overwrite_server);
                        let deletes = RecordStore::path_for_db(&Database::db_path())
                            .and_then(|p| RecordStore::open(&p))
                            .map(|store| {
                                log.read().and_then(|st| {
                                    deciduous::remote::repair_deletes(&nodes, &store, &server, &st)
                                })
                            })
                            .unwrap_or(Ok(Vec::new()));
                        let deletes = deletes.and_then(|mut d| {
                            let docs = db
                                .get_node_documents(None, true)
                                .map_err(|e| format!("reading local documents: {e}"))?;
                            let st = log.read()?;
                            d.extend(deciduous::remote::repair_detaches(&docs, &server, &st));
                            Ok(d)
                        });
                        match deletes {
                            Ok(d) => {
                                plan.deletes = d.len();
                                plan.ops.extend(d);
                            }
                            Err(e) => {
                                eprintln!("{} {}", "Error:".red(), e);
                                exit(1);
                            }
                        }
                        if !plan.overwrites.is_empty() {
                            println!(
                                "{} {} field(s) differ where the server changed the value since this copy last pulled it, or this copy never pulled it:",
                                if overwrite_server {
                                    "Overwriting:".yellow()
                                } else {
                                    "Not sent:".yellow()
                                },
                                plan.overwrites.len()
                            );
                            for o in &plan.overwrites {
                                println!("  {o}");
                            }
                            if !overwrite_server {
                                println!(
                                    "  `deciduous remote pull` takes the server's values; `deciduous remote push --repair --overwrite-server` sends these over them."
                                );
                                withheld_repair = true;
                            }
                        }
                        for op in &plan.ops {
                            if let Err(e) = log.append(op.clone()) {
                                eprintln!("{} {}", "Error:".red(), e);
                                exit(1);
                            }
                        }
                        if !plan.ops.is_empty() {
                            match deciduous::remote::replay(&remote, &log) {
                                Ok(r) => {
                                    println!(
                                        "{} {} op(s) ({} delete(s) or detach(es)) to match this copy: {} applied, {} already there, {} rejected",
                                        "Repaired".green(),
                                        plan.ops.len(),
                                        plan.deletes,
                                        r.applied,
                                        r.already,
                                        r.rejected.len()
                                    );
                                    deciduous::remote::print_rejected(&r.rejected, &log);
                                    refused += own_refusals(&r.rejected);
                                    shown_now
                                        .extend(r.rejected.iter().map(|(op, _)| op.op_id.clone()));
                                }
                                Err(e) => {
                                    eprintln!(
                                        "{} {e}\nThe repair ops wait in {}.",
                                        "Error:".red(),
                                        log.path().display()
                                    );
                                    exit(1);
                                }
                            }
                        } else if plan.overwrites.is_empty() {
                            println!(
                                "{} nothing differs that --repair sends",
                                "Repaired:".green()
                            );
                        }
                        for s in &plan.skipped {
                            println!("  {} not repaired: {s}", "note:".yellow());
                        }
                    }

                    // A refusal whose rows now agree on both sides asks
                    // nothing of anyone: dropped, and said. What is left is
                    // a write the server did not take, whether it was
                    // refused by this push or an earlier one: "Nothing to
                    // push", exit 0, while `remote status` exited 1 on an
                    // earlier refusal, and a settled one stayed until a
                    // pull (round-2 verification of chapter 29).
                    let standing = match log.read() {
                        Ok(st) => st.rejected.iter().any(|(_, a)| !a.is_set_aside()),
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    };
                    if standing {
                        let settled = remote
                            .export()
                            .and_then(|g| deciduous::remote::drop_settled(&log, &db, &g));
                        match settled {
                            Ok(s) => print_settled(&s),
                            Err(e) => {
                                eprintln!(
                                    "{} could not check whether the refusals are settled: {e}",
                                    "Error:".red()
                                );
                                exit(1);
                            }
                        }
                        let earlier: Vec<(deciduous::oplog::Op, String)> = match log.read() {
                            Ok(st) => st
                                .rejected
                                .into_iter()
                                .filter(|(op, a)| !a.is_set_aside() && !op.from_git())
                                .map(|(op, a)| (op, a.reason.unwrap_or_default()))
                                .collect(),
                            Err(e) => {
                                eprintln!("{} {}", "Error:".red(), e);
                                exit(1);
                            }
                        };
                        let shown: std::collections::HashSet<&str> =
                            shown_now.iter().map(String::as_str).collect();
                        let before: Vec<_> = earlier
                            .iter()
                            .filter(|(op, _)| !shown.contains(op.op_id.as_str()))
                            .collect();
                        if !before.is_empty() {
                            eprintln!(
                                "{} {} write(s) the server refused before this push still stand:",
                                "Rejected:".red().bold(),
                                before.len()
                            );
                            for (op, reason) in before.iter().take(10) {
                                eprintln!("  {}  {}", op.body.describe(), reason.dimmed());
                            }
                            eprintln!(
                                "`deciduous remote pull` takes the server's values; \
                                 `deciduous remote status` shows them"
                            );
                        }
                        refused = earlier.len();
                    }

                    // Scripts read the exit code. A write set aside has not
                    // reached the server, and neither has one it refused:
                    // exit 0 after "Rejected: the server refused 1
                    // write(s)" let a script carry on as if it had
                    // (round-2 BRIDGE-N10).
                    if !(seed || overwrite) {
                        if undelivered || refused > 0 || withheld_repair {
                            exit(1);
                        }
                        return;
                    }

                    let graph = match db.get_graph() {
                        Ok(g) => match serde_json::to_value(&g) {
                            Ok(v) => v,
                            Err(e) => {
                                eprintln!("{} serializing the local graph: {}", "Error:".red(), e);
                                exit(1);
                            }
                        },
                        Err(e) => {
                            eprintln!("{} could not read the local graph: {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    let sent = if overwrite {
                        remote
                            .import(graph.clone())
                            .map(|r| (Some(r), Vec::new(), Vec::new()))
                    } else {
                        deciduous::remote::push_missing(&remote, &graph)
                    };
                    let withheld = match &sent {
                        Ok((_, _, w)) => w.clone(),
                        Err(_) => Vec::new(),
                    };

                    // Local nodes the server deleted: nothing pushed can
                    // change them, and printing "edges 1 of 1" for an edge
                    // into one, run after run, sent people back to push.
                    if let Ok((_, deleted, _)) = &sent {
                        if !deleted.is_empty() {
                            println!(
                                "{} the server deleted {} node(s) this machine still has; they were not pushed. `deciduous remote pull` to apply the deletion(s):",
                                "Note:".yellow(),
                                deleted.len()
                            );
                            for d in deleted.iter().take(10) {
                                println!("  {} \"{}\" deleted at {}", d.id, d.title, d.deleted_at);
                            }
                        }
                    }

                    match sent {
                        Ok((None, _, _)) if withheld.is_empty() => {
                            println!(
                                "{} the server already has every live node, edge and document in the local graph ({})",
                                "Nothing to seed:".green(),
                                remote.workspace.cyan()
                            );
                        }
                        Ok((None, _, _)) => {}
                        Ok((Some(r), _, _)) => {
                            deciduous::remote::report_refused(&r, &graph);
                            println!(
                                "{} {} -> {}",
                                if overwrite { "Overwrote:" } else { "Seeded:" }.green(),
                                remote.workspace.cyan(),
                                remote.url
                            );
                            println!("  nodes {} of {}", r.nodes.upserted, r.nodes.received);
                            println!("  edges {} of {}", r.edges.upserted, r.edges.received);
                            if let Some(d) = &r.documents {
                                if d.received > 0 {
                                    println!("  documents {} of {}", d.upserted, d.received);
                                }
                            }
                            if r.edges.unresolved > 0 {
                                println!(
                                    "  {} {} edges were not written (self-loops or missing endpoints)",
                                    "note:".yellow(),
                                    r.edges.unresolved
                                );
                            }
                            if r.edges.stale_change_ids > 0 {
                                println!(
                                    "  {} {} endpoints had a change_id disagreeing with their node",
                                    "note:".yellow(),
                                    r.edges.stale_change_ids
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    }
                    deciduous::remote::print_withheld(&withheld);
                    if undelivered || refused > 0 || !withheld.is_empty() {
                        exit(1);
                    }
                }

                RemoteAction::Pull => {
                    let remote = match deciduous::remote::Remote::for_data_dir(db.data_dir()) {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    let Some(store_path) = RecordStore::path_for_db(&Database::db_path()) else {
                        eprintln!(
                            "{} The database path has no directory of its own, so there is nowhere to keep the graph file.",
                            "Error:".red()
                        );
                        exit(1);
                    };
                    let store = match RecordStore::open(&store_path) {
                        Some(s) => s,
                        None => match RecordStore::create(&store_path) {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!(
                                    "{} could not open the graph file: {}",
                                    "Error:".red(),
                                    e
                                );
                                exit(1);
                            }
                        },
                    };

                    // Refuse another repository's graph before touching the
                    // local one: importing it would put its nodes into this
                    // project's committed graph.json.
                    if let Err(e) = remote.claim(false) {
                        eprintln!("{} {}\n\nNothing was pulled.", "Error:".red(), e);
                        exit(1);
                    }

                    // Unsent local writes go up first. Otherwise the pull
                    // reads a server that lacks them and reports a
                    // difference that is only this machine's queue. A
                    // repository that may not write yet (no commit, a
                    // shallow clone) still reads.
                    if let Some(why) = remote.write_blocker() {
                        eprintln!("{} {why}", "Note:".yellow());
                    } else if let Some(log) = db.oplog() {
                        match deciduous::remote::replay(&remote, &log) {
                            Ok(r) => deciduous::remote::print_rejected(&r.rejected, &log),
                            Err(e) => {
                                eprintln!(
                                    "{} could not send the writes waiting in {} first: {e}\nNothing was pulled.",
                                    "Error:".red(),
                                    log.path().display()
                                );
                                exit(1);
                            }
                        }
                    }

                    match deciduous::remote::pull(&remote, &db, &store) {
                        Ok(r) => {
                            println!(
                                "{} {} <- {}",
                                "Pulled:".green(),
                                remote.workspace.cyan(),
                                remote.url
                            );
                            println!(
                                "  fetched {} nodes, {} edges, {} deletion(s)",
                                r.fetched_nodes, r.fetched_edges, r.fetched_tombstones
                            );
                            println!(
                                "  imported {} new node(s), updated {}, removed {}; imported {} edge(s), removed {}",
                                r.imported_nodes,
                                r.updated_nodes,
                                r.removed_nodes,
                                r.imported_edges,
                                r.removed_edges
                            );
                            if r.dropped_rejected > 0 {
                                println!(
                                    "  dropped {} refused write(s) to nodes the server deleted",
                                    r.dropped_rejected
                                );
                            }
                            print_settled(&r.settled);
                            for d in &r.deleted_over_local_edits {
                                println!(
                                    "  {} node {} \"{}\" was edited here after the server deleted it at {}; the server refuses edits to a deleted node, so it is deleted here too and the edit with it",
                                    "note:".yellow(),
                                    d.id,
                                    d.title,
                                    d.deleted_at
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    }
                }

                RemoteAction::Watch {
                    url: url_only,
                    claude_code,
                    types,
                    branches,
                    edges,
                    json,
                } => {
                    let cfg = Config::load();
                    let remote = match deciduous::remote::Remote::resolve(&cfg, &cwd) {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    // Checked here, not left to whatever WebSocket client
                    // picks up the URL next: a bad token or an unreachable
                    // server should fail against a clear error message, not
                    // as an opaque handshake failure three tools removed from
                    // this one.
                    if let Err(e) = remote.check() {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }

                    let url = remote.events_url();

                    if url_only || claude_code {
                        println!("{} {}", "Watching:".green(), remote.workspace.cyan());
                        println!("{}", url);

                        if claude_code {
                            println!();
                            println!("{}", "Paste into Claude Code:".bold());
                            println!(
                                "Monitor({{ description: \"live writes to {}\", ws: {{ url: \"{}\" }} }})",
                                remote.workspace, url
                            );
                        }

                        println!();
                        println!(
                            "{} this URL carries a live token. Do not paste it anywhere it might be \
                             logged or shared.",
                            "Note:".yellow()
                        );
                        return;
                    }

                    // stderr, so stdout is exactly one event per line and
                    // can be piped without a header in the way.
                    eprintln!("{} {}", "Watching:".green(), remote.workspace.cyan());

                    let filter = deciduous::watch::Filter {
                        types,
                        branches,
                        edges,
                        json,
                    };
                    let unknown = filter.unknown_types();
                    if !unknown.is_empty() {
                        eprintln!(
                            "{} unknown node type{} in --types: {}. Valid types: {}",
                            "Error:".red(),
                            if unknown.len() == 1 { "" } else { "s" },
                            unknown.join(", "),
                            deciduous::watch::NODE_TYPES.join(", ")
                        );
                        exit(1);
                    }
                    let mut stdout = std::io::stdout();
                    if let Err(e) = deciduous::watch::run(&url, &filter, &mut stdout) {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                }
            }
        }

        Command::Sync { check, .. } => {
            let Some(store_path) = RecordStore::path_for_db(&Database::db_path()) else {
                eprintln!(
                    "{} The database path has no directory of its own, so there is nowhere to keep the graph file. Set DECIDUOUS_DB_PATH to a path inside a directory.",
                    "Error:".red()
                );
                exit(1);
            };
            let store = match RecordStore::open(&store_path) {
                Some(store) => store,
                None if check => {
                    eprintln!(
                        "{} No graph file at {}. Run `deciduous sync` once to create it.",
                        "Error:".red(),
                        store_path.display()
                    );
                    exit(1);
                }
                None => match RecordStore::create(&store_path) {
                    Ok(store) => {
                        println!(
                            "{} {} (commit this file)",
                            "Created".green(),
                            store_path.display()
                        );
                        // Later mutations in this process must publish too.
                        db.set_store(Some(store.clone()));
                        store
                    }
                    Err(e) => {
                        eprintln!("{} Creating the graph file: {}", "Error:".red(), e);
                        exit(1);
                    }
                },
            };

            if !check {
                if let Ok(cwd) = std::env::current_dir() {
                    match deciduous::init::ensure_merge_driver(&cwd) {
                        Ok(true) => println!(
                            "{} git merge driver for the graph file (this clone)",
                            "Configured".green()
                        ),
                        Ok(false) => {}
                        Err(e) => eprintln!("{} merge driver: {}", "Warning:".yellow(), e),
                    }
                }
            }

            // On a detached commit that is not mid-merge or mid-rebase, the
            // graph file is a historical version, not the place new rows go
            // (G7): sync imports from it and writes nothing into it.
            let detached = detached_head();
            let viewing_history = detached.is_some() && store.pending_conflicts().is_empty();

            if store.has_legacy_record_dir() {
                if check || viewing_history {
                    println!(
                        "{} .deciduous/sync/ (0.17 per-record files) present; `deciduous sync` will fold it into the graph file",
                        "Note:".yellow()
                    );
                } else {
                    match store.import_legacy_record_dir() {
                        Ok(report) => print_record_dir_import(&report),
                        Err(e) => {
                            eprintln!("{} Importing .deciduous/sync/: {}", "Error:".red(), e);
                            exit(1);
                        }
                    }
                }
            }

            if store.has_legacy_events() {
                if check || viewing_history {
                    println!(
                        "{} Legacy event log present; `deciduous sync` will import it",
                        "Note:".yellow()
                    );
                } else {
                    match store.import_legacy_events() {
                        Ok(report) => print_legacy_import(&report),
                        Err(e) => {
                            eprintln!("{} Importing legacy events: {}", "Error:".red(), e);
                            exit(1);
                        }
                    }
                }
            }

            let reconciled = if viewing_history && !check {
                reconcile_without_exporting(&db, &store)
            } else {
                reconcile(&db, &store, check)
            };
            let mut report = match reconciled {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{} Sync: {}", "Error:".red(), e);
                    exit(1);
                }
            };
            let withheld = if viewing_history {
                take_exports(&mut report)
            } else {
                Vec::new()
            };
            print_sync_report(&report, &store);
            if let (Some(at), false) = (&detached, withheld.is_empty()) {
                println!(
                    "  {} HEAD is detached at {at}, so the graph file was left as this commit has it: \
                     {} not exported. They are still in the database; check out a branch and \
                     run `deciduous sync` to export them there.",
                    "Note:".yellow(),
                    withheld.join(", ")
                );
            }

            if !check && report.conflicts.iter().any(|c| !c.merged) {
                eprintln!(
                    "{} {} is not merged; nothing else was synced",
                    "Error:".red(),
                    store_path.display()
                );
                exit(1);
            }

            if check {
                if report.is_settled() {
                    exit(0);
                }
                if !report.is_clean() || !report.conflicts.is_empty() {
                    println!(
                        "{} Run `deciduous sync` to apply, then commit {}",
                        "Pending:".yellow(),
                        store_path.display()
                    );
                }
                if report.edges_pending > 0 {
                    println!(
                        "{} {} edge(s) wait for nodes that are not in this graph file yet; pull (or ask for) the commit that adds them",
                        "Not settled:".yellow(),
                        report.edges_pending
                    );
                }
                if !report.read_errors.is_empty() {
                    println!(
                        "{} {} record(s) could not be read; `deciduous sync` will not fix them",
                        "Not settled:".yellow(),
                        report.read_errors.len()
                    );
                }
                exit(1);
            }
        }

        Command::Backup { output } => {
            let db_path = Database::db_path();
            if !db_path.exists() {
                eprintln!(
                    "{} No database found at {}",
                    "Error:".red(),
                    db_path.display()
                );
                exit(1);
            }

            let backup_path = output.unwrap_or_else(|| {
                let timestamp = Local::now().format("%Y%m%d_%H%M%S");
                PathBuf::from(format!("deciduous_backup_{}.db", timestamp))
            });

            // Not a file copy: in WAL mode the newest writes sit in
            // deciduous.db-wal until a checkpoint, and copying the main file
            // alone drops them without a word. VACUUM INTO reads through
            // SQLite and writes one self-contained file.
            match db
                .backup_to(&backup_path)
                .map_err(|e| e.to_string())
                .and_then(|_| {
                    std::fs::metadata(&backup_path)
                        .map(|m| m.len())
                        .map_err(|e| e.to_string())
                }) {
                Ok(bytes) => {
                    println!(
                        "{} backup: {} ({} bytes)",
                        "Created".green(),
                        backup_path.display(),
                        bytes
                    );
                }
                Err(e) => {
                    eprintln!("{} Creating backup: {}", "Error:".red(), e);
                    exit(1);
                }
            }
        }

        Command::Commands { limit } => match db.get_recent_commands(limit) {
            Ok(commands) => {
                if commands.is_empty() {
                    println!("No commands logged.");
                } else {
                    for c in commands {
                        println!(
                            "[{}] {} (exit: {})",
                            c.started_at,
                            truncate(&c.command, 60),
                            c.exit_code
                                .map(|c| c.to_string())
                                .unwrap_or_else(|| "running".to_string())
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                exit(1);
            }
        },

        Command::Dot {
            output,
            roots,
            nodes,
            png,
            auto,
            title,
            rankdir,
        } => {
            match db.get_graph() {
                Ok(graph) => {
                    // Filter by specific node IDs if provided
                    let filtered_graph = cli_export_subgraph(&db, graph, nodes, roots);

                    let config = DotConfig {
                        title,
                        show_rationale: true,
                        show_confidence: true,
                        show_ids: true,
                        rankdir,
                    };

                    let dot = graph_to_dot(&filtered_graph, &config);

                    // Determine output path
                    let effective_output = if auto {
                        // Auto-generate branch-specific filename
                        let branch = ProcessCommand::new("git")
                            .args(["rev-parse", "--abbrev-ref", "HEAD"])
                            .output()
                            .ok()
                            .and_then(|o| String::from_utf8(o.stdout).ok())
                            .map(|s| s.trim().to_string())
                            .unwrap_or_else(|| "main".to_string());

                        // Sanitize branch name for filename
                        let safe_branch = branch.replace('/', "-");

                        // Create docs/ if needed
                        let _ = std::fs::create_dir_all("docs");

                        Some(PathBuf::from(format!(
                            "docs/decision-graph-{}.dot",
                            safe_branch
                        )))
                    } else {
                        output.clone()
                    };

                    if png || auto {
                        // Generate PNG using graphviz
                        let dot_path = effective_output
                            .clone()
                            .unwrap_or_else(|| PathBuf::from("graph.dot"));
                        let png_path = dot_path.with_extension("png");

                        // Write DOT file
                        if let Err(e) = std::fs::write(&dot_path, &dot) {
                            eprintln!("{} Writing DOT file: {}", "Error:".red(), e);
                            exit(1);
                        }

                        // Run graphviz
                        match ProcessCommand::new("dot")
                            .args([
                                "-Tpng",
                                &dot_path.to_string_lossy(),
                                "-o",
                                &png_path.to_string_lossy(),
                            ])
                            .output()
                        {
                            Ok(output) => {
                                if output.status.success() {
                                    println!("{} DOT: {}", "Exported".green(), dot_path.display());
                                    println!("{} PNG: {}", "Generated".green(), png_path.display());
                                } else {
                                    eprintln!(
                                        "{} graphviz failed: {}",
                                        "Error:".red(),
                                        String::from_utf8_lossy(&output.stderr)
                                    );
                                    eprintln!(
                                        "Make sure graphviz is installed: brew install graphviz"
                                    );
                                    exit(1);
                                }
                            }
                            Err(e) => {
                                eprintln!("{} Running graphviz: {}", "Error:".red(), e);
                                eprintln!("Make sure graphviz is installed: brew install graphviz");
                                exit(1);
                            }
                        }
                    } else if let Some(path) = output {
                        // Write to file
                        if let Err(e) = std::fs::write(&path, &dot) {
                            eprintln!("{} Writing file: {}", "Error:".red(), e);
                            exit(1);
                        }
                        println!("{} DOT graph to {}", "Exported".green(), path.display());
                        println!(
                            "  {} nodes, {} edges",
                            filtered_graph.nodes.len(),
                            filtered_graph.edges.len()
                        );
                    } else {
                        // Print to stdout
                        println!("{}", dot);
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            }
        }

        Command::Writeup {
            title,
            roots,
            nodes,
            output,
            png,
            auto,
            no_dot,
            no_test_plan,
        } => {
            match db.get_graph() {
                Ok(graph) => {
                    // Filter by specific node IDs if provided
                    let filtered_graph = cli_export_subgraph(&db, graph, nodes, roots);

                    // Auto-detect GitHub repo from git remote
                    let github_repo = ProcessCommand::new("git")
                        .args(["remote", "get-url", "origin"])
                        .output()
                        .ok()
                        .and_then(|o| String::from_utf8(o.stdout).ok())
                        .and_then(|url| {
                            // Parse GitHub URL: git@github.com:owner/repo.git or https://github.com/owner/repo.git
                            let url = url.trim();
                            if url.contains("github.com") {
                                let repo = url
                                    .trim_end_matches(".git")
                                    .split("github.com")
                                    .last()
                                    .map(|s| s.trim_start_matches(':').trim_start_matches('/'))
                                    .map(|s| s.to_string());
                                repo
                            } else {
                                None
                            }
                        });

                    // Auto-detect current branch
                    let git_branch = ProcessCommand::new("git")
                        .args(["rev-parse", "--abbrev-ref", "HEAD"])
                        .output()
                        .ok()
                        .and_then(|o| String::from_utf8(o.stdout).ok())
                        .map(|s| s.trim().to_string());

                    // Determine PNG filename
                    let png_filename = if auto {
                        // Auto-generate from branch name
                        git_branch.as_ref().map(|branch| {
                            let safe_branch = branch.replace('/', "-");
                            format!("docs/decision-graph-{}.png", safe_branch)
                        })
                    } else {
                        png
                    };

                    let config = WriteupConfig {
                        title: title.unwrap_or_else(|| "Pull Request".to_string()),
                        root_ids: vec![], // Already filtered above
                        include_dot: !no_dot,
                        include_test_plan: !no_test_plan,
                        png_filename,
                        github_repo,
                        git_branch,
                    };

                    let writeup = generate_pr_writeup(&filtered_graph, &config);

                    if let Some(path) = output {
                        if let Err(e) = std::fs::write(&path, &writeup) {
                            eprintln!("{} Writing file: {}", "Error:".red(), e);
                            exit(1);
                        }
                        println!("{} PR writeup to {}", "Generated".green(), path.display());
                    } else {
                        println!("{}", writeup);
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            }
        }

        Command::Migrate => match db.migrate_add_change_ids() {
            Ok(true) => {
                println!(
                    "{} Database migrated - added change_id columns for multi-user sync",
                    "Success:".green()
                );
            }
            Ok(false) => {
                println!(
                    "{} Database already has change_id columns - no migration needed",
                    "Info:".cyan()
                );
            }
            Err(e) => {
                eprintln!("{} Migration failed: {}", "Error:".red(), e);
                exit(1);
            }
        },

        Command::Events { action } => {
            eprintln!(
                "{} `deciduous events` is deprecated; `deciduous sync` does all of this now.",
                "Note:".yellow()
            );
            let Some(store_path) = RecordStore::path_for_db(&Database::db_path()) else {
                eprintln!(
                    "{} The database path has no directory of its own.",
                    "Error:".red()
                );
                exit(1);
            };
            match action {
                EventsAction::Init => match RecordStore::create(&store_path) {
                    Ok(_) => println!(
                        "{} {}. Run `deciduous sync` to fill it.",
                        "Created".green(),
                        store_path.display()
                    ),
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                },
                EventsAction::Checkpoint { .. } => {
                    println!(
                        "Checkpoints are gone: the whole graph is one file now. Run `deciduous sync`."
                    );
                }
                EventsAction::Status | EventsAction::Rebuild { .. } | EventsAction::Emit { .. } => {
                    let Some(store) = RecordStore::open(&store_path) else {
                        eprintln!(
                            "{} No graph file at {}. Run `deciduous sync` to create it.",
                            "Error:".red(),
                            store_path.display()
                        );
                        exit(1);
                    };
                    let dry_run = match &action {
                        EventsAction::Status => true,
                        EventsAction::Rebuild { dry_run } => *dry_run,
                        _ => false,
                    };
                    if !dry_run && store.has_legacy_record_dir() {
                        match store.import_legacy_record_dir() {
                            Ok(report) => print_record_dir_import(&report),
                            Err(e) => {
                                eprintln!("{} Importing .deciduous/sync/: {}", "Error:".red(), e);
                                exit(1);
                            }
                        }
                    }
                    if !dry_run && store.has_legacy_events() {
                        match store.import_legacy_events() {
                            Ok(report) => print_legacy_import(&report),
                            Err(e) => {
                                eprintln!("{} Importing legacy events: {}", "Error:".red(), e);
                                exit(1);
                            }
                        }
                    }
                    if let EventsAction::Emit { node_id } = &action {
                        let node_id = resolve_node_or_exit(&db, node_id);
                        match db.get_node(node_id) {
                            Ok(Some(node)) => {
                                if let Err(e) = store.publish_node(&node) {
                                    eprintln!("{} {}", "Error:".red(), e);
                                    exit(1);
                                }
                                println!("{} record for node {}", "Wrote".green(), node_id);
                            }
                            _ => {
                                eprintln!("{} Node {} not found", "Error:".red(), node_id);
                                exit(1);
                            }
                        }
                    }
                    match reconcile(&db, &store, dry_run) {
                        Ok(report) => print_sync_report(&report, &store),
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    }
                }
            }
        }

        Command::Doc { action } => match action {
            DocAction::Attach {
                node_id,
                file,
                description,
                ai_describe,
            } => {
                let node_id = resolve_node_or_exit(&db, &node_id);
                if !file.exists() {
                    eprintln!("{} File not found: {}", "Error:".red(), file.display());
                    exit(1);
                }

                let original_filename = file
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                // Compute SHA-256 hash
                use sha2::{Digest, Sha256};
                let file_bytes = match std::fs::read(&file) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("{} Failed to read file: {}", "Error:".red(), e);
                        exit(1);
                    }
                };
                let hash = format!("{:x}", Sha256::digest(&file_bytes));
                let hash_prefix = &hash[..8];

                // Storage filename: original_name.sha_prefix
                let storage_filename = format!("{}.{}", original_filename, hash_prefix);

                // Detect MIME type
                let mime_type = detect_mime_type(&original_filename);

                let file_size = file_bytes.len() as i32;

                // Store file in .deciduous/documents/
                let docs_dir = db.documents_dir();
                if let Err(e) = std::fs::create_dir_all(&docs_dir) {
                    eprintln!("{} Failed to create documents dir: {}", "Error:".red(), e);
                    exit(1);
                }

                let dest_path = docs_dir.join(&storage_filename);
                if !dest_path.exists() {
                    if let Err(e) = std::fs::copy(&file, &dest_path) {
                        eprintln!("{} Failed to copy file: {}", "Error:".red(), e);
                        exit(1);
                    }
                }

                // Get description (manual, AI, or none)
                let desc = if let Some(d) = description {
                    Some((d, "manual"))
                } else if ai_describe {
                    match generate_ai_description(&original_filename, &file) {
                        Some(d) => Some((d, "ai")),
                        None => {
                            eprintln!(
                                "{} Could not generate AI description (is claude CLI installed?)",
                                "Warning:".yellow()
                            );
                            None
                        }
                    }
                } else {
                    None
                };

                let (desc_text, desc_source) = match &desc {
                    Some((text, source)) => (Some(text.as_str()), *source),
                    None => (None, "none"),
                };

                match db.attach_document(
                    node_id,
                    &hash,
                    &original_filename,
                    &storage_filename,
                    mime_type,
                    file_size,
                    desc_text,
                    desc_source,
                    None,
                ) {
                    Ok(id) => {
                        println!(
                            "{} document {} to node {} ({})",
                            "Attached".green(),
                            id,
                            node_id,
                            original_filename
                        );
                        if let Some((text, _)) = &desc {
                            println!("  Description: {}", truncate(text, 80));
                        }
                    }
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                }
            }

            DocAction::List {
                node_id,
                include_detached,
                json,
            } => match db.get_node_documents(
                node_id.as_deref().map(|r| resolve_node_or_exit(&db, r)),
                include_detached,
            ) {
                Ok(docs) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&docs).unwrap());
                    } else if docs.is_empty() {
                        println!("No documents found.");
                    } else {
                        println!("{} documents:", docs.len());
                        println!(
                            "{:<5} {:<8} {:<25} {:<10} {:<8} DESCRIPTION",
                            "ID", "NODE", "FILENAME", "TYPE", "SIZE"
                        );
                        println!("{}", "-".repeat(80));
                        for d in docs {
                            let size_str = format_file_size(d.file_size);
                            let desc = d
                                .description
                                .as_deref()
                                .map(|s| truncate(s, 30))
                                .unwrap_or_default();
                            println!(
                                "{:<5} {:<8} {:<25} {:<10} {:<8} {}",
                                d.id,
                                d.node_id,
                                truncate(&d.original_filename, 24),
                                truncate(&d.mime_type, 9),
                                size_str,
                                desc
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            },

            DocAction::Describe {
                doc_id,
                description,
                ai,
            } => {
                let desc = if let Some(d) = description {
                    (d, "manual")
                } else if ai {
                    let doc = match db.get_document(doc_id) {
                        Ok(Some(d)) => d,
                        Ok(None) => {
                            eprintln!("{} Document {} not found", "Error:".red(), doc_id);
                            exit(1);
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    };
                    let file_path = db.documents_dir().join(&doc.storage_filename);
                    match generate_ai_description(&doc.original_filename, &file_path) {
                        Some(d) => (d, "ai"),
                        None => {
                            eprintln!("{} Could not generate AI description", "Error:".red());
                            exit(1);
                        }
                    }
                } else {
                    // Read from stdin
                    let mut input = String::new();
                    std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)
                        .unwrap_or_default();
                    (input.trim().to_string(), "manual")
                };

                match db.update_document_description(doc_id, &desc.0, desc.1) {
                    Ok(()) => println!("{} description for document {}", "Updated".green(), doc_id),
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                }
            }

            DocAction::Detach { doc_id } => match db.detach_document(doc_id) {
                Ok(()) => println!("{} document {}", "Detached".red(), doc_id),
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            },

            DocAction::Show { doc_id, json } => match db.get_document(doc_id) {
                Ok(Some(doc)) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&doc).unwrap());
                    } else {
                        println!("{}", "Document Details".bold().underline());
                        println!("  ID:          {}", doc.id);
                        println!("  Node:        {}", doc.node_id);
                        println!("  Filename:    {}", doc.original_filename);
                        println!("  MIME type:   {}", doc.mime_type);
                        println!("  Size:        {}", format_file_size(doc.file_size));
                        println!("  Hash:        {}", doc.content_hash);
                        println!(
                            "  Storage:     .deciduous/documents/{}",
                            doc.storage_filename
                        );
                        println!("  Attached:    {}", doc.attached_at);
                        if let Some(by) = &doc.attached_by {
                            println!("  Attached by: {}", by);
                        }
                        if let Some(desc) = &doc.description {
                            println!("  Description: {} ({})", desc, doc.description_source);
                        }
                        if doc.detached_at.is_some() {
                            println!("  {}", "DETACHED".red());
                        }
                    }
                }
                Ok(None) => {
                    eprintln!("{} Document {} not found", "Error:".red(), doc_id);
                    exit(1);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            },

            DocAction::Open { doc_id } => match db.get_document(doc_id) {
                Ok(Some(doc)) => {
                    let file_path = db.documents_dir().join(&doc.storage_filename);
                    if !file_path.exists() {
                        eprintln!(
                            "{} File not found on disk: {}",
                            "Error:".red(),
                            file_path.display()
                        );
                        exit(1);
                    }

                    // Copy to temp with original filename for better OS handling
                    let temp_dir = std::env::temp_dir().join("deciduous-docs");
                    std::fs::create_dir_all(&temp_dir).ok();
                    let temp_path = temp_dir.join(&doc.original_filename);
                    if let Err(e) = std::fs::copy(&file_path, &temp_path) {
                        eprintln!("{} Failed to copy file: {}", "Error:".red(), e);
                        exit(1);
                    }

                    #[cfg(target_os = "macos")]
                    let open_cmd = "open";
                    #[cfg(not(target_os = "macos"))]
                    let open_cmd = "xdg-open";

                    match std::process::Command::new(open_cmd).arg(&temp_path).spawn() {
                        Ok(_) => println!("{} {}", "Opened".green(), doc.original_filename),
                        Err(e) => {
                            eprintln!("{} Failed to open file: {}", "Error:".red(), e);
                            exit(1);
                        }
                    }
                }
                Ok(None) => {
                    eprintln!("{} Document {} not found", "Error:".red(), doc_id);
                    exit(1);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            },

            DocAction::Gc { dry_run } => {
                let docs_dir = db.documents_dir();
                if !docs_dir.exists() {
                    println!("No documents directory found.");
                    return;
                }

                let active_hashes = db.get_active_content_hashes().unwrap_or_default();
                let mut orphans = Vec::new();

                if let Ok(entries) = std::fs::read_dir(&docs_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            let fname = path
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_default();

                            // Check if any active doc references this storage filename
                            let is_active = db
                                .get_node_documents(None, false)
                                .unwrap_or_default()
                                .iter()
                                .any(|d| d.storage_filename == fname);

                            if !is_active {
                                orphans.push(path);
                            }
                        }
                    }
                }

                if orphans.is_empty() {
                    println!("No orphaned files found.");
                } else {
                    println!("{} orphaned files:", orphans.len());
                    for p in &orphans {
                        println!("  {}", p.display());
                    }
                    if dry_run {
                        println!("(dry run - no files deleted)");
                    } else {
                        for p in &orphans {
                            std::fs::remove_file(p).ok();
                        }
                        println!("{} {} orphaned files", "Deleted".red(), orphans.len());
                    }
                }

                drop(active_hashes);
            }
        },

        // ================================================================
        // Theme Commands
        // ================================================================
        Command::Themes { action } => match action {
            ThemesAction::Create {
                name,
                color,
                description,
            } => match db.create_theme(&name, &color, description.as_deref()) {
                Ok(id) => println!(
                    "{} theme '{}' (id: {}, color: {})",
                    "Created".green(),
                    name.to_lowercase().replace(' ', "-"),
                    id,
                    color
                ),
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            },

            ThemesAction::List => match db.get_all_themes() {
                Ok(themes) => {
                    if themes.is_empty() {
                        println!(
                            "No themes defined. Create one with: deciduous themes create <name>"
                        );
                    } else {
                        println!("{} themes:", themes.len());
                        println!("{:<20} {:<10} DESCRIPTION", "NAME", "COLOR");
                        println!("{}", "-".repeat(60));
                        for t in themes {
                            println!(
                                "{:<20} {:<10} {}",
                                t.name,
                                t.color,
                                t.description.as_deref().unwrap_or("")
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            },

            ThemesAction::Delete { name } => match db.delete_theme(&name) {
                Ok(true) => println!("{} theme '{}'", "Deleted".red(), name),
                Ok(false) => {
                    eprintln!("{} Theme '{}' not found", "Error:".red(), name);
                    exit(1);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            },
        },

        // ================================================================
        // Tag Commands
        // ================================================================
        Command::Tag { action } => match action {
            TagAction::Add { node_id, theme } => {
                match db.tag_node(resolve_node_or_exit(&db, &node_id), &theme, "manual") {
                    Ok(()) => {
                        println!("{} theme '{}' to node {}", "Tagged".green(), theme, node_id)
                    }
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                }
            }

            TagAction::Remove { node_id, theme } => {
                match db.untag_node(resolve_node_or_exit(&db, &node_id), &theme) {
                    Ok(true) => println!(
                        "{} theme '{}' from node {}",
                        "Removed".red(),
                        theme,
                        node_id
                    ),
                    Ok(false) => {
                        eprintln!(
                            "{} Theme '{}' not found on node {}",
                            "Error:".red(),
                            theme,
                            node_id
                        );
                        exit(1);
                    }
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                }
            }

            TagAction::List { node_id } => {
                match db.get_node_themes(resolve_node_or_exit(&db, &node_id)) {
                    Ok(themes) => {
                        if themes.is_empty() {
                            println!("Node {} has no themes.", node_id);
                        } else {
                            println!("Themes for node {}:", node_id);
                            for t in themes {
                                println!("  {} ({})", t.name, t.color);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                }
            }

            TagAction::Suggest { node_id, apply } => {
                let nodes_to_check: Vec<deciduous::DecisionNode> = if let Some(node_ref) = node_id {
                    let id = resolve_node_or_exit(&db, &node_ref);
                    match db.get_node(id) {
                        Ok(Some(n)) => vec![n],
                        Ok(None) => {
                            eprintln!("{} Node {} not found", "Error:".red(), id);
                            exit(1);
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    }
                } else {
                    db.get_all_nodes().unwrap_or_default()
                };

                let all_themes = db.get_all_themes().unwrap_or_default();
                if all_themes.is_empty() {
                    println!(
                        "No themes defined. Create themes first: deciduous themes create <name>"
                    );
                    return;
                }

                let mut total_suggestions = 0;
                for node in &nodes_to_check {
                    let existing: std::collections::HashSet<String> = db
                        .get_node_themes(node.id)
                        .unwrap_or_default()
                        .iter()
                        .map(|t| t.name.clone())
                        .collect();

                    let text = format!(
                        "{} {}",
                        node.title.to_lowercase(),
                        node.description.as_deref().unwrap_or("").to_lowercase()
                    );

                    for theme in &all_themes {
                        if existing.contains(&theme.name) {
                            continue;
                        }

                        // Keyword matching: check if theme name appears in node text
                        let score = if text.contains(&theme.name) {
                            0.9
                        } else if let Some(desc) = &theme.description {
                            let desc_lower = desc.to_lowercase();
                            let keywords: Vec<&str> = desc_lower
                                .split_whitespace()
                                .filter(|w| w.len() > 3)
                                .collect();
                            if keywords.is_empty() {
                                0.0
                            } else {
                                let matches = keywords.iter().filter(|k| text.contains(*k)).count();
                                matches as f64 / keywords.len() as f64
                            }
                        } else {
                            0.0
                        };

                        if score > 0.3 {
                            total_suggestions += 1;
                            println!(
                                "  Node {} ({}): suggest '{}' (score: {:.1})",
                                node.id,
                                truncate(&node.title, 30),
                                theme.name,
                                score
                            );
                            if apply {
                                db.tag_node(node.id, &theme.name, "suggested").ok();
                                println!("    {} as suggested", "Applied".green());
                            }
                        }
                    }
                }

                if total_suggestions == 0 {
                    println!("No theme suggestions found.");
                } else if !apply {
                    println!(
                        "\n{} suggestions. Use --apply to tag them as 'suggested'.",
                        total_suggestions
                    );
                }
            }

            TagAction::Confirm { node_id, theme } => {
                match db.confirm_tag(resolve_node_or_exit(&db, &node_id), &theme) {
                    Ok(true) => println!(
                        "{} theme '{}' on node {} (suggested → manual)",
                        "Confirmed".green(),
                        theme,
                        node_id
                    ),
                    Ok(false) => {
                        eprintln!(
                            "{} Theme '{}' not found on node {}",
                            "Error:".red(),
                            theme,
                            node_id
                        );
                        exit(1);
                    }
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                }
            }
        },

        Command::Completion { .. } => unreachable!(), // Handled above
        Command::LogLoop { .. } => unreachable!(),    // Handled above
        Command::DemoSwarm { .. } => unreachable!(),  // Handled above
        Command::Mcp { .. } => unreachable!(),        // Handled above

        Command::Audit {
            associate_commits,
            min_score,
            dry_run,
            yes,
        } => {
            if !associate_commits {
                eprintln!(
                    "{} No audit action specified. Use --associate-commits",
                    "Error:".red()
                );
                exit(1);
            }

            // Get all nodes
            let nodes = match db.get_all_nodes() {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            };

            // Get git commits since Nov 2024
            let commits = get_git_commits_for_audit();
            if commits.is_empty() {
                eprintln!("{} No git commits found", "Error:".red());
                exit(1);
            }

            println!(
                "{} {} nodes, {} commits",
                "Analyzing:".cyan(),
                nodes.len(),
                commits.len()
            );

            // Find action/outcome nodes without commits
            let nodes_to_check: Vec<_> = nodes
                .iter()
                .filter(|n| n.node_type == "action" || n.node_type == "outcome")
                .filter(|n| {
                    // Check if already has commit
                    !n.metadata_json
                        .as_ref()
                        .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
                        .and_then(|v| {
                            v.get("commit")
                                .and_then(|c| c.as_str())
                                .map(|s| !s.is_empty())
                        })
                        .unwrap_or(false)
                })
                .collect();

            let with_commits = nodes
                .iter()
                .filter(|n| n.node_type == "action" || n.node_type == "outcome")
                .filter(|n| {
                    n.metadata_json
                        .as_ref()
                        .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
                        .and_then(|v| {
                            v.get("commit")
                                .and_then(|c| c.as_str())
                                .map(|s| !s.is_empty())
                        })
                        .unwrap_or(false)
                })
                .count();

            println!(
                "  Action/outcome nodes: {} with commits, {} without",
                with_commits,
                nodes_to_check.len()
            );

            // Find matches
            let mut matches: Vec<CommitMatch> = Vec::new();
            let threshold = min_score as f64 / 100.0;

            for node in &nodes_to_check {
                let mut best_match: Option<(&AuditCommit, f64)> = None;

                for commit in &commits {
                    let score = keyword_match_score(&node.title, &commit.message);
                    if score >= threshold && (best_match.is_none() || score > best_match.unwrap().1)
                    {
                        best_match = Some((commit, score));
                    }
                }

                if let Some((commit, score)) = best_match {
                    matches.push(CommitMatch {
                        node_id: node.id,
                        node_title: node.title.clone(),
                        commit_hash: commit.hash.clone(),
                        commit_message: commit.message.clone(),
                        score,
                    });
                }
            }

            if matches.is_empty() {
                println!(
                    "\n{} No matches found above {}% threshold",
                    "Result:".cyan(),
                    min_score
                );
                return;
            }

            // Sort by score descending
            matches.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            println!(
                "\n{} Found {} potential matches (>= {}%):",
                "Matches:".green(),
                matches.len(),
                min_score
            );
            println!("{}", "=".repeat(80));

            for m in &matches {
                println!(
                    "\nNode #{} ({}%): {}",
                    m.node_id,
                    (m.score * 100.0) as u8,
                    truncate(&m.node_title, 55)
                );
                println!(
                    "  -> {}: {}",
                    &m.commit_hash[..7],
                    truncate(&m.commit_message, 55)
                );
            }

            if dry_run {
                println!("\n{} Dry run - no changes made", "Info:".cyan());
                return;
            }

            // Confirm unless --yes
            if !yes {
                println!("\n{}", "=".repeat(80));
                print!("Apply {} associations? [y/N]: ", matches.len());
                use std::io::Write;
                std::io::stdout().flush().ok();

                let mut input = String::new();
                if std::io::stdin().read_line(&mut input).is_err()
                    || input.trim().to_lowercase() != "y"
                {
                    println!("{}", "Aborted".yellow());
                    return;
                }
            }

            // Apply matches
            let mut applied = 0;
            let mut failed = 0;

            for m in &matches {
                match db.update_node_commit(m.node_id, &m.commit_hash) {
                    Ok(()) => {
                        applied += 1;
                        println!(
                            "{} Node #{} <- {}",
                            "Linked:".green(),
                            m.node_id,
                            &m.commit_hash[..7]
                        );
                    }
                    Err(e) => {
                        failed += 1;
                        eprintln!("{} Node #{}: {}", "Failed:".red(), m.node_id, e);
                    }
                }
            }

            println!(
                "\n{} {} linked, {} failed",
                "Done:".green(),
                applied,
                failed
            );
        }

        Command::Pulse {
            branch,
            recent,
            json,
            summary,
        } => match deciduous::pulse::generate_pulse(&db, branch.as_deref(), recent) {
            Ok(report) => {
                if json {
                    match serde_json::to_string_pretty(&report) {
                        Ok(j) => println!("{}", j),
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    }
                } else {
                    deciduous::pulse::print_pulse(&report, summary);
                }
            }
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                exit(1);
            }
        },

        Command::Narratives { action } => match action {
            NarrativesAction::Init { output, force } => {
                // Next to the database, not `./.deciduous/`: run from a
                // subdirectory, that created a second `.deciduous/` there.
                let path = output.unwrap_or_else(|| db.data_dir().join("narratives.md"));
                if let Err(e) = deciduous::narratives::init_narratives(&db, &path, force) {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            }
            NarrativesAction::Show { path } => {
                let p = path.unwrap_or_else(|| db.data_dir().join("narratives.md"));
                match deciduous::narratives::show_narratives(&p) {
                    Ok(content) => print!("{}", content),
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                }
            }
            NarrativesAction::Pivots { branch, json } => {
                match deciduous::narratives::find_pivots(&db, branch.as_deref()) {
                    Ok(pivots) => {
                        if json {
                            match serde_json::to_string_pretty(&pivots) {
                                Ok(j) => println!("{}", j),
                                Err(e) => {
                                    eprintln!("{} {}", "Error:".red(), e);
                                    exit(1);
                                }
                            }
                        } else {
                            deciduous::narratives::print_pivots(&pivots);
                        }
                    }
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                }
            }
        },

        Command::Archaeology { action } => match action {
            ArchaeologyAction::Pivot {
                from_id,
                observation,
                new_approach,
                confidence,
                reason,
                dry_run,
            } => {
                let from_id = resolve_node_or_exit(&db, &from_id);
                match deciduous::archaeology::create_pivot(
                    &db,
                    from_id,
                    &observation,
                    &new_approach,
                    confidence,
                    reason.as_deref(),
                    dry_run,
                ) {
                    Ok(result) => {
                        deciduous::archaeology::print_pivot_result(&result, dry_run);
                    }
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                }
            }
            ArchaeologyAction::Timeline {
                limit,
                node_type,
                branch,
                json,
            } => {
                match deciduous::archaeology::timeline(
                    &db,
                    limit,
                    node_type.as_deref(),
                    branch.as_deref(),
                ) {
                    Ok(nodes) => {
                        if json {
                            match serde_json::to_string_pretty(&nodes) {
                                Ok(j) => println!("{}", j),
                                Err(e) => {
                                    eprintln!("{} {}", "Error:".red(), e);
                                    exit(1);
                                }
                            }
                        } else {
                            deciduous::archaeology::print_timeline(&nodes);
                        }
                    }
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                }
            }
            ArchaeologyAction::Supersede {
                id,
                cascade,
                dry_run,
            } => match deciduous::archaeology::supersede(
                &db,
                resolve_node_or_exit(&db, &id),
                cascade,
                dry_run,
            ) {
                Ok(result) => {
                    deciduous::archaeology::print_supersede_result(&result, dry_run);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            },
        },

        Command::Roadmap { action } => {
            match action {
                RoadmapAction::Init { path } => {
                    let roadmap_path = path.unwrap_or_else(|| PathBuf::from("ROADMAP.md"));

                    if !roadmap_path.exists() {
                        eprintln!(
                            "{} File not found: {}",
                            "Error:".red(),
                            roadmap_path.display()
                        );
                        exit(1);
                    }

                    // Parse the roadmap
                    let parsed = match parse_roadmap(&roadmap_path) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("{} Parsing roadmap: {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    println!(
                        "{} Found {} sections in {}",
                        "Parsed:".green(),
                        parsed.sections.len(),
                        roadmap_path.display()
                    );

                    // Read original content for rewriting
                    let content = match std::fs::read_to_string(&roadmap_path) {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("{} Reading file: {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    // Write back with metadata
                    let updated = match write_roadmap_with_metadata(
                        &roadmap_path,
                        &parsed.sections,
                        &content,
                    ) {
                        Ok(u) => u,
                        Err(e) => {
                            eprintln!("{} Writing metadata: {}", "Error:".red(), e);
                            exit(1);
                        }
                    };
                    if let Err(e) = std::fs::write(&roadmap_path, &updated) {
                        eprintln!("{} Writing file: {}", "Error:".red(), e);
                        exit(1);
                    }

                    // Track current level-2 parent section for grouping
                    let mut current_l2_parent: Option<String> = None;

                    // Store sections in database
                    for section in &parsed.sections {
                        // Level 2 headers (## Section) are top-level groupings
                        // Level 3 headers (### Subsection) contain the actual tasks
                        let (section_parent, items_section) = if section.level == 2 {
                            current_l2_parent = Some(section.title.clone());
                            // Level 2 sections have no parent, their items go under them
                            (None, Some(section.title.as_str()))
                        } else {
                            // Level 3 sections belong to the current L2 parent
                            // Their items belong directly to this L3 section
                            (current_l2_parent.as_deref(), Some(section.title.as_str()))
                        };

                        // Create the section header entry (checkbox_state = "none")
                        if let Err(e) = db.create_roadmap_item(
                            &section.title,
                            section.description.as_deref(),
                            section_parent,
                            None, // parent_id - we don't track hierarchy by ID yet
                            "none",
                        ) {
                            eprintln!("{} Creating roadmap item: {}", "Warning:".yellow(), e);
                        }

                        // Create items for checkboxes - they belong to THIS section
                        for item in &section.items {
                            let state = if item.checked { "checked" } else { "unchecked" };
                            if let Err(e) = db.create_roadmap_item(
                                &item.text,
                                None,
                                items_section, // Items belong to the section that contains them
                                None,          // parent_id
                                state,
                            ) {
                                eprintln!("{} Creating roadmap item: {}", "Warning:".yellow(), e);
                            }
                        }
                    }

                    // Count items
                    let total_items: usize = parsed.sections.iter().map(|s| s.items.len()).sum();
                    println!(
                        "{} Initialized {} sections with {} items",
                        "Success:".green(),
                        parsed.sections.len(),
                        total_items
                    );
                    println!("  Metadata comments added to {}", roadmap_path.display());
                }

                RoadmapAction::Refresh { path } => {
                    let roadmap_path = path.unwrap_or_else(|| PathBuf::from("ROADMAP.md"));

                    if !roadmap_path.exists() {
                        eprintln!(
                            "{} File not found: {}",
                            "Error:".red(),
                            roadmap_path.display()
                        );
                        exit(1);
                    }

                    // Clear existing roadmap items
                    let cleared = match db.clear_roadmap_items() {
                        Ok(n) => n,
                        Err(e) => {
                            eprintln!("{} Clearing roadmap items: {}", "Error:".red(), e);
                            exit(1);
                        }
                    };
                    println!(
                        "{} Cleared {} existing roadmap items",
                        "Info:".cyan(),
                        cleared
                    );

                    // Re-parse the roadmap
                    let parsed = match parse_roadmap(&roadmap_path) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("{} Parsing roadmap: {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    // Track current level-2 parent section for grouping
                    let mut current_l2_parent: Option<String> = None;

                    // Store sections in database
                    for section in &parsed.sections {
                        let (section_parent, items_section) = if section.level == 2 {
                            current_l2_parent = Some(section.title.clone());
                            (None, Some(section.title.as_str()))
                        } else {
                            (current_l2_parent.as_deref(), Some(section.title.as_str()))
                        };

                        // Create the section header entry
                        if let Err(e) = db.create_roadmap_item(
                            &section.title,
                            section.description.as_deref(),
                            section_parent,
                            None,
                            "none",
                        ) {
                            eprintln!("{} Creating roadmap item: {}", "Warning:".yellow(), e);
                        }

                        // Create items for checkboxes
                        for item in &section.items {
                            let state = if item.checked { "checked" } else { "unchecked" };
                            if let Err(e) =
                                db.create_roadmap_item(&item.text, None, items_section, None, state)
                            {
                                eprintln!("{} Creating roadmap item: {}", "Warning:".yellow(), e);
                            }
                        }
                    }

                    let total_items: usize = parsed.sections.iter().map(|s| s.items.len()).sum();
                    println!(
                        "{} Refreshed {} sections with {} items",
                        "Success:".green(),
                        parsed.sections.len(),
                        total_items
                    );
                }

                RoadmapAction::Sync {
                    path,
                    repo,
                    execute,
                    create_issues,
                } => {
                    let dry_run = !execute; // Default is dry-run mode
                    let roadmap_path = path.unwrap_or_else(|| PathBuf::from("ROADMAP.md"));

                    if !roadmap_path.exists() {
                        eprintln!(
                            "{} File not found: {}",
                            "Error:".red(),
                            roadmap_path.display()
                        );
                        eprintln!("Run 'deciduous roadmap init' first");
                        exit(1);
                    }

                    // Initialize GitHub client
                    let gh_client = match repo {
                        Some(r) => GitHubClient::new(Some(r)),
                        None => match GitHubClient::auto_detect() {
                            Ok(c) => c,
                            Err(e) => {
                                eprintln!("{} Auto-detecting repo: {}", "Error:".red(), e);
                                eprintln!("Specify repo with --repo owner/repo");
                                exit(1);
                            }
                        },
                    };

                    // Check auth
                    match GitHubClient::check_auth() {
                        Ok(true) => {}
                        Ok(false) | Err(_) => {
                            eprintln!("{} Not authenticated with GitHub", "Error:".red());
                            eprintln!("Run 'gh auth login' first");
                            exit(1);
                        }
                    }

                    // Parse roadmap
                    let parsed = match parse_roadmap(&roadmap_path) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("{} Parsing roadmap: {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    // Only sync level 3 sections (actual items, not parent headers)
                    let syncable_sections: Vec<&RoadmapSection> =
                        parsed.sections.iter().filter(|s| s.level == 3).collect();

                    if dry_run {
                        println!(
                            "{} {} sections (use --execute to apply changes)",
                            "Roadmap (dry run):".yellow(),
                            syncable_sections.len()
                        );
                    } else {
                        println!(
                            "{} Syncing {} sections",
                            "Roadmap:".cyan(),
                            syncable_sections.len()
                        );
                    }

                    if let Some(repo_name) = gh_client.repo_name() {
                        println!("  Repository: {}", repo_name);
                    }

                    // Ensure 'roadmap' label exists if we're creating issues
                    if !dry_run && create_issues {
                        match ensure_roadmap_label(&gh_client) {
                            Ok(true) => println!("  {} Created 'roadmap' label", "✓".green()),
                            Ok(false) => {} // Label already exists
                            Err(e) => eprintln!(
                                "  {} Creating label: {} (issues may fail)",
                                "Warning:".yellow(),
                                e
                            ),
                        }
                    }

                    let mut created = 0;
                    let mut updated = 0;
                    let mut skipped = 0;

                    for section in &syncable_sections {
                        // Check if section already has an issue
                        if let Some(issue_num) = section.github_issue_number {
                            // Update existing issue
                            let body = generate_issue_body(section);

                            if dry_run {
                                println!(
                                    "  {} Would update issue #{}: {}",
                                    "[DRY]".yellow(),
                                    issue_num,
                                    section.title
                                );
                                updated += 1;
                            } else {
                                match gh_client.update_issue_body(issue_num, &body) {
                                    Ok(()) => {
                                        println!(
                                            "  {} Updated issue #{}: {}",
                                            "✓".green(),
                                            issue_num,
                                            section.title
                                        );
                                        updated += 1;
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "  {} Updating issue #{}: {}",
                                            "✗".red(),
                                            issue_num,
                                            e
                                        );
                                    }
                                }
                            }
                        } else if create_issues {
                            // Create new issue
                            let body = generate_issue_body(section);

                            if dry_run {
                                println!(
                                    "  {} Would create issue: {}",
                                    "[DRY]".yellow(),
                                    section.title
                                );
                                created += 1;
                            } else {
                                match gh_client.create_issue(&section.title, &body, &["roadmap"]) {
                                    Ok(issue) => {
                                        println!(
                                            "  {} Created issue #{}: {}",
                                            "✓".green(),
                                            issue.number,
                                            section.title
                                        );
                                        created += 1;

                                        // Update database with issue number
                                        if let Err(e) = db.update_roadmap_item_github_by_title(
                                            &section.title,
                                            issue.number,
                                            &issue.state,
                                        ) {
                                            eprintln!(
                                                "    {} Updating database: {}",
                                                "Warning:".yellow(),
                                                e
                                            );
                                        }

                                        // Cache issue for web display
                                        if let Some(repo_name) = gh_client.repo_name() {
                                            if let Err(e) = db.cache_github_issue(
                                                issue.number,
                                                repo_name,
                                                &issue.title,
                                                Some(&issue.body),
                                                &issue.state,
                                                &issue.html_url,
                                                &issue.created_at,
                                                &issue.updated_at,
                                            ) {
                                                eprintln!(
                                                    "    {} Caching issue: {}",
                                                    "Warning:".yellow(),
                                                    e
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "  {} Creating issue for '{}': {}",
                                            "✗".red(),
                                            section.title,
                                            e
                                        );
                                    }
                                }
                            }
                        } else {
                            println!("  {} Skipping (no issue): {}", "-".dimmed(), section.title);
                            skipped += 1;
                        }
                    }

                    // Write updated roadmap with issue metadata
                    if !dry_run && created > 0 {
                        let content = std::fs::read_to_string(&roadmap_path).unwrap_or_default();
                        match write_roadmap_with_metadata(&roadmap_path, &parsed.sections, &content)
                        {
                            Ok(updated_content) => {
                                if let Err(e) = std::fs::write(&roadmap_path, &updated_content) {
                                    eprintln!("{} Writing roadmap: {}", "Warning:".yellow(), e);
                                }
                            }
                            Err(e) => eprintln!("{} Updating metadata: {}", "Warning:".yellow(), e),
                        }
                    }

                    println!(
                        "\n{} {} created, {} updated, {} skipped",
                        if dry_run {
                            "Summary (dry run):".yellow()
                        } else {
                            "Summary:".green()
                        },
                        created,
                        updated,
                        skipped
                    );
                }

                RoadmapAction::List {
                    path,
                    section,
                    with_issues,
                    without_issues,
                } => {
                    let roadmap_path = path.unwrap_or_else(|| PathBuf::from("ROADMAP.md"));

                    if !roadmap_path.exists() {
                        eprintln!(
                            "{} File not found: {}",
                            "Error:".red(),
                            roadmap_path.display()
                        );
                        exit(1);
                    }

                    let parsed = match parse_roadmap(&roadmap_path) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("{} Parsing roadmap: {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    // Filter sections
                    let filtered: Vec<_> = parsed
                        .sections
                        .iter()
                        .filter(|s| {
                            if let Some(ref sect) = section {
                                s.title.to_lowercase().contains(&sect.to_lowercase())
                            } else {
                                true
                            }
                        })
                        .filter(|s| {
                            if with_issues {
                                s.github_issue_number.is_some()
                            } else if without_issues {
                                s.github_issue_number.is_none()
                            } else {
                                true
                            }
                        })
                        .collect();

                    if filtered.is_empty() {
                        println!("No roadmap items found matching filters.");
                        return;
                    }

                    println!("{} ({} sections)\n", "ROADMAP.md".cyan(), filtered.len());

                    for s in &filtered {
                        // Show section header based on level
                        let header_prefix = if s.level == 2 { "##" } else { "###" };

                        let issue_str = match s.github_issue_number {
                            Some(n) => format!("#{}", n).green().to_string(),
                            None => "no issue".dimmed().to_string(),
                        };

                        let completed: usize = s.items.iter().filter(|i| i.checked).count();
                        let total = s.items.len();

                        if total > 0 {
                            println!(
                                "{} {} [{}/{}] ({})",
                                header_prefix.yellow(),
                                s.title,
                                completed,
                                total,
                                issue_str
                            );
                        } else {
                            println!("{} {} ({})", header_prefix.yellow(), s.title, issue_str);
                        }

                        // Show checkbox items
                        for item in &s.items {
                            let check = if item.checked {
                                "✓".green()
                            } else {
                                "○".dimmed()
                            };
                            println!("    {} {}", check, item.text);
                        }
                    }
                }

                RoadmapAction::Link { item, outcome_id } => {
                    let outcome_id = resolve_node_or_exit(&db, &outcome_id);
                    // Find roadmap item by title or change_id
                    let items = match db.get_all_roadmap_items() {
                        Ok(i) => i,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    let target = items.iter().find(|i| {
                        i.change_id == item || i.title.to_lowercase().contains(&item.to_lowercase())
                    });

                    match target {
                        Some(roadmap_item) => {
                            // Verify outcome node exists and is an outcome
                            match db.get_all_nodes() {
                                Ok(nodes) => {
                                    let node = nodes.iter().find(|n| n.id == outcome_id);
                                    match node {
                                        Some(n) if n.node_type == "outcome" => {
                                            // Link them
                                            match db.link_roadmap_to_outcome(
                                                roadmap_item.id,
                                                outcome_id,
                                                &n.change_id,
                                            ) {
                                                Ok(()) => {
                                                    println!(
                                                        "{} Linked '{}' to outcome #{}: {}",
                                                        "Success:".green(),
                                                        roadmap_item.title,
                                                        outcome_id,
                                                        n.title
                                                    );
                                                }
                                                Err(e) => {
                                                    eprintln!("{} {}", "Error:".red(), e);
                                                    exit(1);
                                                }
                                            }
                                        }
                                        Some(n) => {
                                            eprintln!(
                                                "{} Node #{} is a {}, not an outcome",
                                                "Error:".red(),
                                                outcome_id,
                                                n.node_type
                                            );
                                            exit(1);
                                        }
                                        None => {
                                            eprintln!(
                                                "{} Node #{} not found",
                                                "Error:".red(),
                                                outcome_id
                                            );
                                            exit(1);
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("{} {}", "Error:".red(), e);
                                    exit(1);
                                }
                            }
                        }
                        None => {
                            eprintln!("{} Roadmap item '{}' not found", "Error:".red(), item);
                            eprintln!("Run 'deciduous roadmap list' to see available items");
                            exit(1);
                        }
                    }
                }

                RoadmapAction::Unlink { item } => {
                    let items = match db.get_all_roadmap_items() {
                        Ok(i) => i,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    let target = items.iter().find(|i| {
                        i.change_id == item || i.title.to_lowercase().contains(&item.to_lowercase())
                    });

                    match target {
                        Some(roadmap_item) => {
                            match db.unlink_roadmap_from_outcome(roadmap_item.id) {
                                Ok(()) => {
                                    println!(
                                        "{} Unlinked '{}' from outcome",
                                        "Success:".green(),
                                        roadmap_item.title
                                    );
                                }
                                Err(e) => {
                                    eprintln!("{} {}", "Error:".red(), e);
                                    exit(1);
                                }
                            }
                        }
                        None => {
                            eprintln!("{} Roadmap item '{}' not found", "Error:".red(), item);
                            exit(1);
                        }
                    }
                }

                RoadmapAction::Conflicts { resolve } => {
                    let conflicts = match db.get_unresolved_conflicts() {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    if conflicts.is_empty() {
                        println!("{} No sync conflicts", "Success:".green());
                        return;
                    }

                    println!(
                        "{} {} conflicts found:\n",
                        "Conflicts:".yellow(),
                        conflicts.len()
                    );

                    for conflict in &conflicts {
                        println!(
                            "  Item: {} ({})",
                            conflict.item_change_id, conflict.conflict_type
                        );
                        println!(
                            "    Local:  {}",
                            conflict.local_value.as_deref().unwrap_or("(none)")
                        );
                        println!(
                            "    Remote: {}",
                            conflict.remote_value.as_deref().unwrap_or("(none)")
                        );
                        if let Some(ref res) = conflict.resolution {
                            println!("    Resolution: {}", res);
                        }
                        println!();
                    }

                    if resolve {
                        println!(
                            "{} Interactive conflict resolution not yet implemented",
                            "TODO:".yellow()
                        );
                        println!(
                            "For now, manually edit ROADMAP.md and run 'deciduous roadmap sync'"
                        );
                    }
                }

                RoadmapAction::Status { path } => {
                    let roadmap_path = path.unwrap_or_else(|| PathBuf::from("ROADMAP.md"));

                    // Get sync state from database
                    match db.get_roadmap_sync_state(&roadmap_path.to_string_lossy()) {
                        Ok(Some(state)) => {
                            println!("{}", "Roadmap Sync Status".cyan());
                            println!("  Path: {}", roadmap_path.display());
                            if let Some(ref repo) = state.github_repo {
                                println!("  GitHub Repo: {}", repo);
                            }
                            if let Some(ref last_sync) = state.last_github_sync {
                                println!("  Last GitHub Sync: {}", last_sync);
                            }
                            if let Some(ref last_parse) = state.last_markdown_parse {
                                println!("  Last Parse: {}", last_parse);
                            }
                            if state.conflict_count > 0 {
                                println!("  {} {} conflicts", "⚠".yellow(), state.conflict_count);
                            } else {
                                println!("  {} No conflicts", "✓".green());
                            }
                        }
                        Ok(None) => {
                            println!("{} Roadmap not initialized", "Status:".yellow());
                            println!("Run 'deciduous roadmap init' to get started");
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    }

                    // Show item counts from database
                    match db.get_all_roadmap_items() {
                        Ok(items) => {
                            let with_issues = items
                                .iter()
                                .filter(|i| i.github_issue_number.is_some())
                                .count();
                            let with_outcomes =
                                items.iter().filter(|i| i.outcome_node_id.is_some()).count();
                            let completed = items
                                .iter()
                                .filter(|i| i.checkbox_state == "checked")
                                .count();

                            println!("\n{}", "Items:".cyan());
                            println!("  Total: {}", items.len());
                            println!("  With GitHub Issues: {}", with_issues);
                            println!("  With Outcome Links: {}", with_outcomes);
                            println!("  Completed: {}", completed);
                        }
                        Err(_) => {
                            println!("\n{} No items in database yet", "Items:".dimmed());
                        }
                    }
                }

                RoadmapAction::Check {
                    path: _,
                    incomplete,
                    complete,
                } => {
                    // Get all roadmap items from database
                    let items = match db.get_all_roadmap_items() {
                        Ok(i) => i,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            exit(1);
                        }
                    };

                    if items.is_empty() {
                        println!("{} No roadmap items in database", "Status:".yellow());
                        println!("Run 'deciduous roadmap init' first");
                        return;
                    }

                    // Check completion for each item
                    let mut complete_count = 0;
                    let mut incomplete_count = 0;
                    let mut results: Vec<(String, bool, bool, bool, bool)> = Vec::new();

                    for item in &items {
                        match db.check_roadmap_item_completion(item.id) {
                            Ok((is_complete, has_outcome, issue_closed)) => {
                                let checkbox_checked = item.checkbox_state == "checked";

                                if is_complete && checkbox_checked {
                                    complete_count += 1;
                                } else {
                                    incomplete_count += 1;
                                }

                                results.push((
                                    item.title.clone(),
                                    is_complete && checkbox_checked,
                                    checkbox_checked,
                                    has_outcome,
                                    issue_closed,
                                ));
                            }
                            Err(e) => {
                                eprintln!("{} Checking {}: {}", "Warning:".yellow(), item.title, e);
                            }
                        }
                    }

                    // Print header
                    println!("{}", "Roadmap Completion Audit".cyan().bold());
                    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                    println!();

                    // Print results based on filters
                    for (title, is_complete, checkbox, outcome, issue) in &results {
                        // Apply filters
                        if incomplete && *is_complete {
                            continue;
                        }
                        if complete && !*is_complete {
                            continue;
                        }

                        let status_icon = if *is_complete {
                            "✓".green()
                        } else {
                            "○".yellow()
                        };

                        let checkbox_icon = if *checkbox {
                            "☑".green()
                        } else {
                            "☐".dimmed()
                        };
                        let outcome_icon = if *outcome {
                            "⚡".green()
                        } else {
                            "⚡".dimmed()
                        };
                        let issue_icon = if *issue {
                            "🔒".green()
                        } else {
                            "🔓".dimmed()
                        };

                        println!(
                            "{} {} {} {} {}",
                            status_icon,
                            checkbox_icon,
                            outcome_icon,
                            issue_icon,
                            truncate(title, 60)
                        );
                    }

                    // Print summary
                    println!();
                    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                    println!();
                    println!("{}", "Legend:".dimmed());
                    println!(
                        "  {} = checkbox checked    {} = outcome linked    {} = issue closed",
                        "☑".green(),
                        "⚡".green(),
                        "🔒".green()
                    );
                    println!();
                    println!("{}", "Summary:".cyan());
                    println!("  {} {} complete", "✓".green(), complete_count);
                    println!("  {} {} incomplete", "○".yellow(), incomplete_count);
                    println!("  {} total items", items.len());

                    if incomplete_count > 0 {
                        println!();
                        println!(
                            "{} Completion requires: checkbox ☑ AND outcome ⚡ AND issue closed 🔒",
                            "Note:".dimmed()
                        );
                    }
                }
            }
        }

        Command::Hooks { action } => {
            // Hooks commands don't need the database
            match action {
                HooksAction::Install {} => {
                    let project_root = Config::find_project_root().unwrap_or_else(|| {
                        std::env::current_dir().expect("Could not get current directory")
                    });

                    println!("\n{}", "Installing Claude Code hooks...".cyan().bold());
                    if let Err(e) = deciduous::hooks::install_hooks(&project_root) {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                    println!(
                        "\n{}",
                        "Hooks installed from .deciduous/config.toml."
                            .green()
                            .bold()
                    );
                    println!();
                }
                HooksAction::Status {} => {
                    if let Err(e) = deciduous::hooks::hooks_status() {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                }
                HooksAction::Uninstall {} => {
                    let project_root = Config::find_project_root().unwrap_or_else(|| {
                        std::env::current_dir().expect("Could not get current directory")
                    });

                    println!("\n{}", "Uninstalling Claude Code hooks...".cyan().bold());
                    if let Err(e) = deciduous::hooks::uninstall_hooks(&project_root) {
                        eprintln!("{} {}", "Error:".red(), e);
                        exit(1);
                    }
                    println!("\n{}", "Hooks uninstalled.".green().bold());
                    println!();
                }
            }
        }

        Command::Integration {} => {
            if let Err(e) = deciduous::hooks::integration_status() {
                eprintln!("{} {}", "Error:".red(), e);
                exit(1);
            }
        }

        Command::Opencode { action } => match action {
            OpencodeAction::Install {} => {
                let project_root = Config::find_project_root().unwrap_or_else(|| {
                    std::env::current_dir().expect("Could not get current directory")
                });

                if let Err(e) = deciduous::opencode::install_opencode(&project_root) {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            }
            OpencodeAction::Status {} => {
                if let Err(e) = deciduous::opencode::opencode_status() {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
            }
            OpencodeAction::Uninstall {} => {
                let project_root = Config::find_project_root().unwrap_or_else(|| {
                    std::env::current_dir().expect("Could not get current directory")
                });

                println!("\n{}", "Uninstalling OpenCode integration...".cyan().bold());
                if let Err(e) = deciduous::opencode::uninstall_opencode(&project_root) {
                    eprintln!("{} {}", "Error:".red(), e);
                    exit(1);
                }
                println!("\n{}", "OpenCode integration uninstalled.".green().bold());
                println!();
            }
        },
    }

    // Show update reminder if integration files are outdated
    if let Some(reminder) = deciduous::changelog::check_version_reminder(env!("CARGO_PKG_VERSION"))
    {
        eprintln!();
        eprintln!("{}", reminder.yellow());
    }
}

fn detect_mime_type(filename: &str) -> &'static str {
    match filename
        .rsplit('.')
        .next()
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("md" | "markdown") => "text/markdown",
        Some("txt") => "text/plain",
        Some("rs") => "text/x-rust",
        Some("ts" | "tsx") => "text/typescript",
        Some("js" | "jsx") => "text/javascript",
        Some("py") => "text/x-python",
        Some("json") => "application/json",
        Some("yaml" | "yml") => "text/yaml",
        Some("toml") => "text/toml",
        Some("html" | "htm") => "text/html",
        Some("css") => "text/css",
        Some("zip") => "application/zip",
        Some("tar") => "application/x-tar",
        Some("gz") => "application/gzip",
        Some("csv") => "text/csv",
        Some("xml") => "text/xml",
        Some("sql") => "text/x-sql",
        Some("sh" | "bash") => "text/x-shellscript",
        Some("go") => "text/x-go",
        Some("rb") => "text/x-ruby",
        Some("java") => "text/x-java",
        Some("c" | "h") => "text/x-c",
        Some("cpp" | "hpp" | "cc") => "text/x-c++",
        _ => "application/octet-stream",
    }
}

fn format_file_size(bytes: i32) -> String {
    let bytes = bytes as f64;
    if bytes < 1024.0 {
        format!("{}B", bytes as i64)
    } else if bytes < 1024.0 * 1024.0 {
        format!("{:.1}KB", bytes / 1024.0)
    } else {
        format!("{:.1}MB", bytes / (1024.0 * 1024.0))
    }
}

fn generate_ai_description(filename: &str, file_path: &std::path::Path) -> Option<String> {
    let prompt = format!(
        "Analyze this file and provide a concise 1-2 sentence description of its contents, purpose, and key details. File: {}",
        filename
    );

    // Try to read text content for context
    let content_context = if let Ok(content) = std::fs::read_to_string(file_path) {
        let preview: String = content.chars().take(2000).collect();
        format!("{}\n\nFile content preview:\n{}", prompt, preview)
    } else {
        prompt
    };

    let output = std::process::Command::new("claude")
        .args(["-p", &content_context])
        .output()
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let char_len = max_len.saturating_sub(3);
        let truncated: String = s.chars().take(char_len).collect();
        format!("{}...", truncated)
    }
}

// =============================================================================
// Audit command helpers
// =============================================================================

/// Commit info for audit matching
struct AuditCommit {
    hash: String,
    message: String,
}

/// A potential node-to-commit match
struct CommitMatch {
    node_id: i32,
    node_title: String,
    commit_hash: String,
    commit_message: String,
    score: f64,
}

/// Get git commits for audit (since Nov 2024)
fn get_git_commits_for_audit() -> Vec<AuditCommit> {
    let output = ProcessCommand::new("git")
        .args(["log", "--format=%H|%s", "--since=2024-11-01"])
        .output()
        .ok();

    match output {
        Some(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(2, '|').collect();
                if parts.len() == 2 {
                    Some(AuditCommit {
                        hash: parts[0].to_string(),
                        message: parts[1].to_string(),
                    })
                } else {
                    None
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Calculate keyword match score between node title and commit message
fn keyword_match_score(node_title: &str, commit_message: &str) -> f64 {
    let stopwords: std::collections::HashSet<&str> = [
        "the", "a", "an", "and", "or", "to", "for", "in", "on", "with", "is", "was", "be", "as",
        "of", "it", "that", "this", "from", "by",
    ]
    .iter()
    .cloned()
    .collect();

    let normalize = |s: &str| -> std::collections::HashSet<String> {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .filter(|w| !stopwords.contains(w))
            .map(|s| s.to_string())
            .collect()
    };

    let node_words = normalize(node_title);
    let commit_words = normalize(commit_message);

    if node_words.is_empty() {
        return 0.0;
    }

    let common: std::collections::HashSet<_> = node_words.intersection(&commit_words).collect();
    common.len() as f64 / node_words.len() as f64
}

// =============================================================================
// Tests
// =============================================================================

/// Turn a CLI node reference (id or change_id prefix) into a local id, or exit.
fn resolve_node_or_exit(db: &Database, reference: &str) -> i32 {
    match db.resolve_node_ref(reference) {
        Ok(id) => id,
        Err(deciduous::db::DbError::NoSuchNode(msg)) => match resolve_server_id(db, reference) {
            Ok(id) => id,
            Err(why) => {
                eprintln!(
                    "{} {msg}\n{why}\n\n{}",
                    "Error:".red(),
                    deciduous::db::NODE_REF_KINDS
                );
                exit(1);
            }
        },
        Err(e) => {
            eprintln!("{} {}", "Error:".red(), e);
            exit(1);
        }
    }
}

/// A reference no local id or change_id matched, tried as the server's id
/// for a node (T5): agents quote those, and `dx link 56eb8923 ...` answered
/// "No node has a change_id starting with '56eb8923'" for a node this clone
/// had under change_id 291cdb5a. Asks the server only after every local
/// kind has missed, so it costs nothing when a local reference works. The
/// Err is the sentence explaining why the server did not resolve it.
fn resolve_server_id(db: &Database, reference: &str) -> Result<i32, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("(no working directory: {e})"))?;
    let cfg = Config::load();
    if !cfg.remote.is_configured() {
        return Err(
            "This project has no [remote], so it was not looked up as a server id.".to_string(),
        );
    }
    let remote = deciduous::remote::Remote::resolve(&cfg, &cwd)
        .map_err(|e| format!("It was not looked up as a server id: {e}"))?;
    let matches = deciduous::remote::find_by_server_id(&remote, reference)
        .map_err(|e| format!("It may be a server id, but the server could not be asked: {e}"))?;
    let describe = |m: &deciduous::remote::ServerIdMatch| {
        format!(
            "server id {} = change_id {} ({})",
            m.id.chars().take(12).collect::<String>(),
            m.change_id.chars().take(12).collect::<String>(),
            m.title
        )
    };
    let m = match matches.as_slice() {
        [] => {
            return Err(format!(
                "It is not a server id in workspace {} either.",
                remote.workspace
            ))
        }
        [m] => m,
        many => {
            return Err(format!(
                "As a server id prefix it matches {} nodes; use more characters: {}",
                many.len(),
                many.iter()
                    .take(5)
                    .map(describe)
                    .collect::<Vec<_>>()
                    .join("; ")
            ))
        }
    };
    if m.deleted {
        return Err(format!(
            "It is the {}, which was deleted on the server.",
            describe(m)
        ));
    }
    match db.resolve_node_ref(&m.change_id) {
        Ok(id) => {
            eprintln!(
                "{} '{reference}' is the {}: local #{id}",
                "Note:".cyan(),
                describe(m)
            );
            Ok(id)
        }
        Err(_) => Err(format!(
            "It is the {}, which this clone does not have yet. Run `deciduous remote pull` first.",
            describe(m)
        )),
    }
}

fn print_legacy_import(report: &deciduous::LegacyImport) {
    println!(
        "{} legacy event log: {} events{} -> {} node records, {} edge records",
        "Imported".green(),
        report.events,
        if report.checkpoint {
            " + checkpoint"
        } else {
            ""
        },
        report.nodes,
        report.edges
    );
    if report.removed {
        println!("  Removed .deciduous/sync/events/ and checkpoint.json (git rm them too)");
    } else {
        println!(
            "  {} {} line(s) could not be read, so the legacy files were kept:",
            "Warning:".yellow(),
            report.errors.len()
        );
        for e in &report.errors {
            println!("    {}", e);
        }
    }
}

fn print_record_dir_import(report: &deciduous::LegacyImport) {
    println!(
        "{} .deciduous/sync/ into the graph file: {} nodes, {} edges, {} themes, {} tags",
        "Folded".green(),
        report.nodes,
        report.edges,
        report.themes,
        report.tags
    );
    if report.removed {
        println!("  Removed .deciduous/sync/ (git rm -r it too)");
    } else {
        println!(
            "  {} {} file(s) could not be read, so .deciduous/sync/ was kept:",
            "Warning:".yellow(),
            report.errors.len()
        );
        for e in &report.errors {
            println!("    {}", e);
        }
    }
}

fn print_sync_report(report: &SyncReport, store: &RecordStore) {
    let verb = if report.dry_run { "would" } else { "did" };
    let on_disk = match store.read_doc_all() {
        Ok(_) => {
            let counts = store.counts();
            format!(
                "{} nodes, {} edges, {} themes, {} tags on disk",
                counts.nodes, counts.edges, counts.themes, counts.tags
            )
        }
        // Not "0 nodes": the records are there, they just do not parse yet.
        Err(_) => "not readable until it is merged".to_string(),
    };
    println!(
        "{} {} ({})",
        if report.dry_run { "Checked" } else { "Synced" }.cyan(),
        store.path().display(),
        on_disk
    );
    let unresolved = report.conflicts.iter().any(|c| !c.merged);
    let mut lines: Vec<String> = Vec::new();
    let mut push = |n: usize, what: &str| {
        if n > 0 {
            lines.push(format!("{} {}", n, what));
        }
    };
    push(report.nodes_imported, "nodes imported");
    push(report.nodes_updated, "nodes updated from records");
    push(report.nodes_deleted, "nodes deleted (tombstones)");
    push(report.nodes_exported, "nodes exported");
    push(report.edges_imported, "edges imported");
    push(report.edges_updated, "edges updated from records");
    push(report.edges_deleted, "edges deleted (tombstones)");
    push(report.edges_exported, "edges exported");
    push(report.themes_imported, "themes imported");
    push(report.themes_updated, "themes updated");
    push(report.themes_deleted, "themes deleted");
    push(report.themes_exported, "themes exported");
    push(report.themes_merged, "same-named themes folded together");
    push(report.tags_imported, "tags imported");
    push(report.tags_deleted, "tags deleted");
    push(report.tags_exported, "tags exported");
    if unresolved {
        println!("  Not compared with the database until the graph file is merged");
    } else if lines.is_empty() {
        println!("  Database and records already agree");
    } else {
        println!("  {} {}: {}", "Changes".bold(), verb, lines.join(", "));
    }
    if report.edges_pending > 0 {
        println!(
            "  {} {} edge(s) wait for a node that has not arrived yet:",
            "Pending:".yellow(),
            report.edges_pending
        );
        for d in report.pending_details.iter().take(10) {
            println!("    {}", d);
        }
    }
    if report.edges_orphaned > 0 {
        println!(
            "  {} edge(s) point at deleted nodes and were skipped",
            report.edges_orphaned
        );
    }
    for e in &report.errors {
        println!(
            "  {} the database refused a record: {}",
            "Warning:".yellow(),
            e
        );
    }
    for c in &report.conflicts {
        match (&c.merged, &c.message) {
            (true, Some(m)) => println!("  {} {}: {}", "Merged".green(), c.path, m),
            (true, None) => println!("  {} conflict markers in {}", "Merged".green(), c.path),
            (false, Some(m)) => println!("  {} {}: {}", "Conflict:".yellow(), c.path, m),
            (false, None) => println!("  {} {}", "Conflict:".yellow(), c.path),
        }
    }
    if !report.read_errors.is_empty() {
        println!(
            "  {} {} record(s) could not be read (fix them, or `git checkout` the graph file):",
            "Warning:".yellow(),
            report.read_errors.len()
        );
        for e in &report.read_errors {
            println!("    {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === keyword_match_score Tests ===

    #[test]
    fn test_keyword_match_exact() {
        // Exact match should be 100%
        let score = keyword_match_score("Add user authentication", "feat: Add user authentication");
        assert!((score - 1.0).abs() < 0.01, "Expected ~100%, got {}", score);
    }

    #[test]
    fn test_keyword_match_partial() {
        // Partial overlap
        let score =
            keyword_match_score("Implement dark mode toggle", "feat: add dark mode support");
        // "dark" and "mode" match, "implement" and "toggle" don't
        assert!(
            score > 0.3 && score < 0.8,
            "Expected partial match, got {}",
            score
        );
    }

    #[test]
    fn test_keyword_match_no_overlap() {
        let score = keyword_match_score("Fix database connection", "feat: add new UI component");
        assert!(score < 0.1, "Expected no match, got {}", score);
    }

    #[test]
    fn test_keyword_match_ignores_stopwords() {
        // Stopwords like "the", "a", "to" should be ignored
        let score = keyword_match_score("the fix for the bug", "a fix to the issue");
        // Only "fix" matches, "bug" vs "issue" don't
        assert!(score > 0.0, "Should have some match from 'fix'");
    }

    #[test]
    fn test_keyword_match_case_insensitive() {
        let score = keyword_match_score("ADD USER AUTH", "add user auth");
        assert!(
            (score - 1.0).abs() < 0.01,
            "Should match case-insensitively"
        );
    }

    #[test]
    fn test_keyword_match_empty_title() {
        let score = keyword_match_score("", "some commit message");
        assert_eq!(score, 0.0, "Empty title should return 0");
    }

    #[test]
    fn test_keyword_match_all_stopwords() {
        let score = keyword_match_score("the a an", "the a an");
        assert_eq!(score, 0.0, "All stopwords should return 0");
    }

    #[test]
    fn test_keyword_match_special_chars() {
        // Special characters are filtered, identical strings match
        let score = keyword_match_score("fix: user-auth (v2)", "fix: user-auth (v2)");
        // Both strings normalize the same, should be 100%
        assert!(
            (score - 1.0).abs() < 0.01,
            "Same string should match 100%, got {}",
            score
        );

        // Punctuation like colons is stripped
        let score2 = keyword_match_score("fix bug", "fix: bug");
        assert!(
            (score2 - 1.0).abs() < 0.01,
            "Punctuation should be ignored, got {}",
            score2
        );
    }

    #[test]
    fn test_keyword_match_real_example() {
        // Real example from the codebase
        let score = keyword_match_score(
            "Implemented prompt tracking for decision nodes",
            "feat: add prompt tracking to decision nodes",
        );
        assert!(
            score > 0.7,
            "Real example should have high match, got {}",
            score
        );
    }
}
