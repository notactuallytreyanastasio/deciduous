use chrono::{Local, TimeZone};
use clap::{CommandFactory, Parser, Subcommand};
use colored::Colorize;
use deciduous::github::{ensure_roadmap_label, GitHubClient};
use deciduous::roadmap::{
    generate_issue_body, parse_roadmap, write_roadmap_with_metadata, RoadmapSection,
};
use deciduous::util::truncate;
use deciduous::{
    filter_graph_by_ids,
    generate_edge_id,
    generate_pr_writeup,
    get_current_author,
    graph_to_dot,
    parse_node_range,
    // Event log sync
    Checkpoint,
    CheckpointEdge,
    CheckpointNode,
    Config,
    Database,
    DotConfig,
    Event,
    EventLog,
    MaterializedState,
    WriteupConfig,
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
    /// Auto-detects which assistants are installed (.claude/, .opencode/)
    /// and updates their integration files.
    /// Does NOT touch: settings files, .deciduous/config.toml, docs/
    Update {},

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
        #[arg(short, long)]
        confidence: Option<u8>,

        /// Git commit hash to link this node to. Use "HEAD" to auto-detect current commit.
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
        /// Source node ID
        from: i32,

        /// Target node ID
        to: i32,

        /// Rationale for this connection
        #[arg(short, long)]
        rationale: Option<String>,

        /// Edge type: leads_to, requires, chosen, rejected, blocks, enables
        #[arg(short = 't', long, default_value = "leads_to")]
        edge_type: String,
    },

    /// Remove an edge between two nodes
    Unlink {
        /// Source node ID
        from: i32,

        /// Target node ID
        to: i32,
    },

    /// Delete a node and all its connected edges
    Delete {
        /// Node ID to delete
        id: i32,

        /// Show what would be deleted without actually deleting
        #[arg(long)]
        dry_run: bool,
    },

    /// Update node status
    Status {
        /// Node ID
        id: i32,

        /// New status: pending, active, completed, rejected
        status: String,
    },

    /// Update or add a prompt to an existing node
    Prompt {
        /// Node ID to update
        id: i32,

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
        /// Node ID to display
        id: i32,

        /// Show JSON output instead of formatted
        #[arg(long)]
        json: bool,
    },

    /// Export full graph as JSON
    Graph,

    /// Start the graph viewer server
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },

    /// Export graph to JSON file
    Sync {
        /// Output path (default: .deciduous/web/graph-data.json)
        #[arg(short, long)]
        output: Option<PathBuf>,
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

        /// Graph direction: TB (top-bottom) or LR (left-right)
        #[arg(long, default_value = "TB")]
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

    /// Event-based multi-user sync (alternative to diff/patch workflow)
    ///
    /// Uses append-only event logs instead of snapshot patches.
    /// Events are stored in .deciduous/sync/events/{user}.jsonl (git-tracked).
    /// Each user's events are automatically merged via git.
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

    /// Generate shell completions
    Completion {
        /// Shell type: bash, zsh, fish, powershell, elvish
        shell: clap_complete::Shell,
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
    /// Creates .deciduous/sync/ directory structure and adds to .gitignore
    /// the local database while tracking the sync directory.
    Init,

    /// Emit an event for a node (for testing/manual sync)
    Emit {
        /// Node ID to emit event for
        node_id: i32,
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

        /// Outcome node ID to link
        outcome_id: i32,
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
        /// Output path (default: .deciduous/narratives.md)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Overwrite existing file
        #[arg(long)]
        force: bool,
    },

    /// Display narratives.md contents
    Show {
        /// Path to narratives.md (default: .deciduous/narratives.md)
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
        /// Node ID of the existing approach being reconsidered
        from_id: i32,

        /// Observation text (what was learned that triggers the pivot)
        observation: String,

        /// New approach/decision title
        new_approach: String,

        /// Confidence for the new decision (0-100)
        #[arg(short, long)]
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
        /// Node ID to mark as superseded
        id: i32,

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
        /// Node ID to attach the file to
        node_id: i32,

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
        /// Node ID to list documents for (omit for all)
        node_id: Option<i32>,

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
        /// Node ID to tag
        node_id: i32,

        /// Theme name
        theme: String,
    },

    /// Remove a theme from a node
    Remove {
        /// Node ID to untag
        node_id: i32,

        /// Theme name
        theme: String,
    },

    /// List themes for a node
    List {
        /// Node ID to show themes for
        node_id: i32,
    },

    /// Auto-suggest themes for a node based on keywords and AI
    Suggest {
        /// Node ID to suggest themes for (omit for all untagged nodes)
        node_id: Option<i32>,

        /// Apply suggestions without confirmation
        #[arg(long)]
        apply: bool,
    },

    /// Confirm a suggested theme (change from "suggested" to "manual")
    Confirm {
        /// Node ID
        node_id: i32,

        /// Theme name to confirm
        theme: String,
    },
}

/// Emit sync events if multi-user sync is initialized (.deciduous/sync exists).
///
/// The closure receives the current author and returns the events to append.
/// Each append failure prints a warning but does not abort.
fn emit_sync_events(build: impl FnOnce(String) -> Vec<Event>) {
    let sync_dir = PathBuf::from(".deciduous/sync");
    if !sync_dir.exists() {
        return;
    }
    let author = get_current_author();
    if let Ok(event_log) = EventLog::new(&PathBuf::from(".deciduous"), author.clone()) {
        for event in build(author) {
            if let Err(e) = event_log.append(event) {
                eprintln!("{} Sync event: {}", "Warning:".yellow(), e);
            }
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

        if let Err(e) =
            deciduous::init::init_project(setup_claude, setup_opencode, windsurf, no_auto_update)
        {
            eprintln!("{} {}", "Error:".red(), e);
            std::process::exit(1);
        }
        return;
    }

    // Handle update separately - it doesn't need an existing database
    // Auto-detects which assistants are installed
    if let Command::Update {} = args.command {
        if let Err(e) = deciduous::init::update_tooling() {
            eprintln!("{} {}", "Error:".red(), e);
            std::process::exit(1);
        }
        return;
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
            std::process::exit(1);
        }

        let installed_version = match std::fs::read_to_string(version_file) {
            Ok(v) => v.trim().to_string(),
            Err(_) => {
                println!(
                    "{} Could not read version file. Run 'deciduous update'.",
                    "Update needed:".yellow()
                );
                std::process::exit(1);
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
            std::process::exit(1);
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
            std::process::exit(1);
        }
        return;
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

    let db = match Database::open() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("{} Failed to open database: {}", "Error:".red(), e);
            std::process::exit(1);
        }
    };

    match args.command {
        Command::Init { .. } => unreachable!(),   // Handled above
        Command::Update { .. } => unreachable!(), // Handled above
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

            // Expand "HEAD" to actual commit hash
            let effective_commit = commit.as_ref().and_then(|c| {
                if c.eq_ignore_ascii_case("HEAD") {
                    deciduous::get_current_git_commit()
                } else {
                    Some(c.clone())
                }
            });

            // Parse date parameter into RFC3339 format
            let effective_date = date.as_ref().map(|d| parse_backdate(d));

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
                        .map(|c| format!(" [commit: {}]", c.chars().take(7).collect::<String>()))
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

                    // Auto-emit event if sync is initialized
                    emit_sync_events(|author| {
                        if let Ok(Some(node)) = db.get_node(id) {
                            vec![Event::AddNode {
                                change_id: node.change_id.clone(),
                                node_type: node.node_type.clone(),
                                title: node.title.clone(),
                                description: node.description.clone(),
                                status: node.status.clone(),
                                metadata_json: node.metadata_json.clone(),
                                timestamp: chrono::Utc::now(),
                                author,
                            }]
                        } else {
                            Vec::new()
                        }
                    });
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        Command::Link {
            from,
            to,
            rationale,
            edge_type,
        } => match db.create_edge(from, to, &edge_type, rationale.as_deref()) {
            Ok(id) => {
                println!(
                    "{} edge {} ({} -> {} via {})",
                    "Created".green(),
                    id,
                    from,
                    to,
                    edge_type
                );

                // Auto-emit event if sync is initialized
                emit_sync_events(|author| {
                    // Get change_ids for the nodes
                    let from_node = db.get_node(from).ok().flatten();
                    let to_node = db.get_node(to).ok().flatten();

                    if let (Some(from_n), Some(to_n)) = (from_node, to_node) {
                        vec![Event::AddEdge {
                            edge_id: generate_edge_id(
                                &from_n.change_id,
                                &to_n.change_id,
                                &edge_type,
                            ),
                            from_change_id: from_n.change_id.clone(),
                            to_change_id: to_n.change_id.clone(),
                            edge_type: edge_type.clone(),
                            rationale: rationale.clone(),
                            timestamp: chrono::Utc::now(),
                            author,
                        }]
                    } else {
                        Vec::new()
                    }
                });
            }
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                std::process::exit(1);
            }
        },

        Command::Unlink { from, to } => {
            // Get node and edge info before deletion for event emission.
            // delete_edge removes ALL edges between the pair, so capture every
            // edge's type to emit matching DeleteEdge events.
            let from_node = db.get_node(from).ok().flatten();
            let to_node = db.get_node(to).ok().flatten();
            let deleted_edges = db.get_edges_between(from, to).unwrap_or_default();

            match db.delete_edge(from, to) {
                Ok(()) => {
                    println!("{} edge ({} -> {})", "Removed".red(), from, to);

                    // Auto-emit event if sync is initialized
                    emit_sync_events(|author| {
                        let mut events = Vec::new();
                        if let (Some(from_n), Some(to_n)) = (from_node, to_node) {
                            // Emit one DeleteEdge per deleted edge, using each
                            // edge's real type so the edge_id matches the one
                            // emitted when the edge was created
                            let mut emitted_ids: Vec<String> = Vec::new();
                            for edge in &deleted_edges {
                                let edge_id = generate_edge_id(
                                    &from_n.change_id,
                                    &to_n.change_id,
                                    &edge.edge_type,
                                );
                                if emitted_ids.contains(&edge_id) {
                                    continue;
                                }
                                emitted_ids.push(edge_id.clone());
                                events.push(Event::DeleteEdge {
                                    edge_id,
                                    timestamp: chrono::Utc::now(),
                                    author: author.clone(),
                                });
                            }
                        }
                        events
                    });
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        Command::Delete { id, dry_run } => {
            // Get node info before deletion for event emission
            let node_info = if !dry_run {
                db.get_node(id).ok().flatten()
            } else {
                None
            };

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

                        // Auto-emit event if sync is initialized
                        emit_sync_events(|author| {
                            if let Some(node) = node_info {
                                vec![Event::DeleteNode {
                                    change_id: node.change_id.clone(),
                                    timestamp: chrono::Utc::now(),
                                    author,
                                }]
                            } else {
                                Vec::new()
                            }
                        });
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        Command::Status { id, status } => match db.update_node_status(id, &status) {
            Ok(()) => {
                println!("{} node {} status to '{}'", "Updated".green(), id, status);

                // Auto-emit event if sync is initialized
                emit_sync_events(|author| {
                    if let Ok(Some(node)) = db.get_node(id) {
                        vec![Event::UpdateNode {
                            change_id: node.change_id.clone(),
                            title: None,
                            description: None,
                            status: Some(status.clone()),
                            metadata_json: None,
                            timestamp: chrono::Utc::now(),
                            author,
                        }]
                    } else {
                        Vec::new()
                    }
                });
            }
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                std::process::exit(1);
            }
        },

        Command::Prompt { id, prompt } => {
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
                std::process::exit(1);
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
                Ok(()) => println!(
                    "{} node {} prompt ({} chars)",
                    "Updated".green(),
                    id,
                    effective_prompt.len()
                ),
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
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
                                Some(b) => n.branch().is_some_and(|node_branch| node_branch == *b),
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
                        println!("{:<5} {:<12} {:<10} TITLE", "ID", "TYPE", "STATUS");
                        println!("{}", "-".repeat(70));
                        for n in filtered {
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
                                    let truncated = truncate(desc, 80);
                                    println!(
                                        "{:<5} {:<12} {:<10} {}",
                                        n.id, type_colored, n.status, n.title
                                    );
                                    println!("      {:<22} {}", "", truncated.dimmed());
                                } else {
                                    println!(
                                        "{:<5} {:<12} {:<10} {}",
                                        n.id, type_colored, n.status, n.title
                                    );
                                }
                            } else {
                                println!(
                                    "{:<5} {:<12} {:<10} {}",
                                    n.id, type_colored, n.status, n.title
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
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
                std::process::exit(1);
            }
        },

        Command::Show { id, json } => {
            match db.get_node(id) {
                Ok(Some(node)) => {
                    if json {
                        // JSON output mode
                        match serde_json::to_string_pretty(&node) {
                            Ok(json_str) => println!("{}", json_str),
                            Err(e) => {
                                eprintln!("{} Serializing node: {}", "Error:".red(), e);
                                std::process::exit(1);
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
                        println!("{}: {}", "Created".bold(), node.created_at);
                        println!("{}: {}", "Updated".bold(), node.updated_at);

                        // Parse metadata
                        if let Some(meta) = node.metadata() {
                            println!();
                            println!("{}", "Metadata".bold().underline());

                            if let Some(conf) = meta.get("confidence").and_then(|v| v.as_i64()) {
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
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        Command::Graph => match db.get_graph() {
            Ok(graph) => match serde_json::to_string_pretty(&graph) {
                Ok(json) => println!("{}", json),
                Err(e) => {
                    eprintln!("{} Serializing graph: {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            },
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                std::process::exit(1);
            }
        },

        Command::Serve { port } => {
            println!(
                "{} Starting graph viewer at http://localhost:{}",
                "Deciduous".cyan(),
                port
            );
            if let Err(e) = deciduous::serve::start_graph_server(port) {
                eprintln!("{} Server error: {}", "Error:".red(), e);
                std::process::exit(1);
            }
        }

        Command::Sync { output } => {
            // Default to docs/ for GitHub Pages compatibility
            let output_path = output.unwrap_or_else(|| PathBuf::from("docs/graph-data.json"));

            // Create parent directories if needed
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }

            // Load config and include it in export (for external repo support, etc.)
            let config = Config::load();
            let include_config = config.github.commit_repo.is_some();

            match db.get_graph_with_config(if include_config { Some(config) } else { None }) {
                Ok(graph) => {
                    match serde_json::to_string_pretty(&graph) {
                        Ok(json) => {
                            match std::fs::write(&output_path, &json) {
                                Ok(()) => {
                                    println!(
                                        "{} graph to {}",
                                        "Exported".green(),
                                        output_path.display()
                                    );
                                    println!(
                                        "  {} nodes, {} edges",
                                        graph.nodes.len(),
                                        graph.edges.len()
                                    );

                                    // Also sync to docs/demo/ if it exists (for GitHub Pages demo)
                                    let demo_path = PathBuf::from("docs/demo/graph-data.json");
                                    if demo_path.parent().map(|p| p.exists()).unwrap_or(false) {
                                        if let Err(e) = std::fs::write(&demo_path, &json) {
                                            eprintln!(
                                                "{} Also writing to demo/: {}",
                                                "Warning:".yellow(),
                                                e
                                            );
                                        }
                                    }

                                    // Export git history for linked commits
                                    // Skip when external repo is configured (commits won't be in local git)
                                    if !include_config {
                                        if let Some(output_dir) = output_path.parent() {
                                            match export_git_history(&graph.nodes, output_dir) {
                                                Ok(count) => {
                                                    if count > 0 {
                                                        println!(
                                                            "{} git-history.json ({} commits)",
                                                            "Exported".green(),
                                                            count
                                                        );
                                                    }
                                                    // Also sync to docs/demo/ if it exists
                                                    let demo_dir = PathBuf::from("docs/demo");
                                                    if demo_dir.exists() {
                                                        if let Err(e) = export_git_history(
                                                            &graph.nodes,
                                                            &demo_dir,
                                                        ) {
                                                            eprintln!("{} Also writing git history to demo/: {}", "Warning:".yellow(), e);
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    // Non-fatal: git history is optional
                                                    eprintln!(
                                                        "{} Exporting git history: {}",
                                                        "Warning:".yellow(),
                                                        e
                                                    );
                                                }
                                            }
                                        }
                                    } else {
                                        // External repo mode: preserve existing git-history.json
                                        if let Some(output_dir) = output_path.parent() {
                                            let git_history_path =
                                                output_dir.join("git-history.json");
                                            if git_history_path.exists() {
                                                println!(
                                                    "{} git-history.json (external repo mode - manually managed)",
                                                    "Preserved".cyan()
                                                );
                                            } else {
                                                println!(
                                                    "{} Create docs/git-history.json manually for external repo commits",
                                                    "Note:".yellow()
                                                );
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("{} Writing file: {}", "Error:".red(), e);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("{} Serializing graph: {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
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
                std::process::exit(1);
            }

            let backup_path = output.unwrap_or_else(|| {
                let timestamp = Local::now().format("%Y%m%d_%H%M%S");
                PathBuf::from(format!("deciduous_backup_{}.db", timestamp))
            });

            match std::fs::copy(&db_path, &backup_path) {
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
                    std::process::exit(1);
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
                std::process::exit(1);
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
                    let filtered_graph = if let Some(node_spec) = nodes {
                        let node_ids = parse_node_range(&node_spec);
                        filter_graph_by_ids(&graph, &node_ids)
                    } else if let Some(root_spec) = roots {
                        // Parse root IDs and traverse
                        let root_ids: Vec<i32> = root_spec
                            .split(',')
                            .filter_map(|s| s.trim().parse().ok())
                            .collect();
                        deciduous::filter_graph_from_roots(&graph, &root_ids)
                    } else {
                        graph
                    };

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
                            std::process::exit(1);
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
                                    std::process::exit(1);
                                }
                            }
                            Err(e) => {
                                eprintln!("{} Running graphviz: {}", "Error:".red(), e);
                                eprintln!("Make sure graphviz is installed: brew install graphviz");
                                std::process::exit(1);
                            }
                        }
                    } else if let Some(path) = output {
                        // Write to file
                        if let Err(e) = std::fs::write(&path, &dot) {
                            eprintln!("{} Writing file: {}", "Error:".red(), e);
                            std::process::exit(1);
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
                    std::process::exit(1);
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
                    let filtered_graph = if let Some(node_spec) = nodes {
                        let node_ids = parse_node_range(&node_spec);
                        filter_graph_by_ids(&graph, &node_ids)
                    } else if let Some(root_spec) = roots {
                        let root_ids: Vec<i32> = root_spec
                            .split(',')
                            .filter_map(|s| s.trim().parse().ok())
                            .collect();
                        deciduous::filter_graph_from_roots(&graph, &root_ids)
                    } else {
                        graph
                    };

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
                            std::process::exit(1);
                        }
                        println!("{} PR writeup to {}", "Generated".green(), path.display());
                    } else {
                        println!("{}", writeup);
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
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
                std::process::exit(1);
            }
        },

        Command::Events { action } => {
            // Get the .deciduous directory
            let deciduous_dir = PathBuf::from(".deciduous");
            if !deciduous_dir.exists() {
                eprintln!(
                    "{} No .deciduous directory found. Run 'deciduous init' first.",
                    "Error:".red()
                );
                std::process::exit(1);
            }

            let author = get_current_author();

            match action {
                EventsAction::Init => {
                    let sync_dir = deciduous_dir.join("sync");
                    let events_dir = sync_dir.join("events");

                    if sync_dir.exists() {
                        println!(
                            "{} Sync directory already exists at {}",
                            "Info:".cyan(),
                            sync_dir.display()
                        );
                    } else {
                        match std::fs::create_dir_all(&events_dir) {
                            Ok(()) => {
                                println!(
                                    "{} Created sync directory at {}",
                                    "Success:".green(),
                                    sync_dir.display()
                                );
                                println!("  Events will be stored in {}", events_dir.display());
                                println!();
                                println!("{}", "Next steps:".cyan());
                                println!("  1. Add .deciduous/sync/ to git tracking");
                                println!(
                                    "  2. Team members pull and run 'deciduous events rebuild'"
                                );
                            }
                            Err(e) => {
                                eprintln!("{} Creating sync directory: {}", "Error:".red(), e);
                                std::process::exit(1);
                            }
                        }
                    }
                }

                EventsAction::Status => {
                    match EventLog::new(&deciduous_dir, author.clone()) {
                        Ok(event_log) => {
                            println!("{} Event-based sync status", "Sync:".cyan());
                            println!("  Author: {}", author);
                            println!("  Sync dir: {}", event_log.sync_dir().display());

                            // Check for checkpoint
                            match event_log.load_checkpoint() {
                                Ok(Some(cp)) => {
                                    println!(
                                        "  Checkpoint: {} ({} nodes, {} edges)",
                                        cp.created_at.format("%Y-%m-%d %H:%M:%S"),
                                        cp.nodes.len(),
                                        cp.edges.len()
                                    );
                                }
                                Ok(None) => {
                                    println!("  Checkpoint: none");
                                }
                                Err(e) => {
                                    println!("  Checkpoint: error loading ({})", e);
                                }
                            }

                            // Count events
                            match event_log.get_events_after_checkpoint() {
                                Ok(events) => {
                                    println!("  Pending events: {}", events.len());

                                    // Count by author
                                    let mut by_author: std::collections::HashMap<String, usize> =
                                        std::collections::HashMap::new();
                                    for event in &events {
                                        *by_author
                                            .entry(event.author().to_string())
                                            .or_default() += 1;
                                    }
                                    if !by_author.is_empty() {
                                        println!("  Events by author:");
                                        for (a, count) in by_author {
                                            println!("    {}: {}", a, count);
                                        }
                                    }
                                }
                                Err(e) => {
                                    println!("  Pending events: error ({})", e);
                                }
                            }

                            // List event files
                            let events_dir = event_log.sync_dir().join("events");
                            if events_dir.exists() {
                                println!("  Event files:");
                                if let Ok(entries) = std::fs::read_dir(&events_dir) {
                                    for entry in entries.flatten() {
                                        let path = entry.path();
                                        if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                                            let size = std::fs::metadata(&path)
                                                .map(|m| m.len())
                                                .unwrap_or(0);
                                            println!(
                                                "    {} ({} bytes)",
                                                path.file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy(),
                                                size
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    }
                }

                EventsAction::Rebuild { dry_run } => {
                    match EventLog::new(&deciduous_dir, author) {
                        Ok(event_log) => {
                            println!("{} Rebuilding database from events...", "Sync:".cyan());

                            // Load checkpoint
                            let checkpoint = match event_log.load_checkpoint() {
                                Ok(cp) => cp,
                                Err(e) => {
                                    eprintln!("{} Loading checkpoint: {}", "Error:".red(), e);
                                    std::process::exit(1);
                                }
                            };

                            // Build materialized state
                            let mut state = match &checkpoint {
                                Some(cp) => {
                                    println!(
                                        "  Loaded checkpoint: {} nodes, {} edges",
                                        cp.nodes.len(),
                                        cp.edges.len()
                                    );
                                    MaterializedState::from_checkpoint(cp)
                                }
                                None => {
                                    println!("  No checkpoint found, starting fresh");
                                    MaterializedState::default()
                                }
                            };

                            // Get events after checkpoint
                            let events = match event_log.get_events_after_checkpoint() {
                                Ok(e) => e,
                                Err(e) => {
                                    eprintln!("{} Reading events: {}", "Error:".red(), e);
                                    std::process::exit(1);
                                }
                            };

                            println!("  Replaying {} events...", events.len());
                            state.replay(&events);

                            println!(
                                "  Result: {} nodes, {} edges",
                                state.nodes.len(),
                                state.edges.len()
                            );

                            if dry_run {
                                println!();
                                println!(
                                    "{} Dry run - no changes made to database",
                                    "Info:".cyan()
                                );
                            } else {
                                // Apply to database
                                // For now, we'll use the existing patch apply mechanism
                                // by creating nodes with specific change_ids
                                let mut nodes_created = 0;
                                let mut nodes_skipped = 0;
                                let mut edges_created = 0;
                                let mut edges_failed = 0;

                                // Get existing nodes
                                let existing_nodes = db.get_all_nodes().unwrap_or_default();
                                let existing_change_ids: std::collections::HashSet<String> =
                                    existing_nodes.iter().map(|n| n.change_id.clone()).collect();

                                // Create nodes
                                for node in state.nodes.values() {
                                    if existing_change_ids.contains(&node.change_id) {
                                        nodes_skipped += 1;
                                        continue;
                                    }

                                    // Parse metadata from the stored JSON
                                    let meta = node.metadata_json.as_ref().and_then(|m| {
                                        serde_json::from_str::<serde_json::Value>(m).ok()
                                    });

                                    let confidence = meta
                                        .as_ref()
                                        .and_then(|m| m.get("confidence"))
                                        .and_then(|c| c.as_u64())
                                        .map(|c| c as u8);
                                    let commit = meta
                                        .as_ref()
                                        .and_then(|m| m.get("commit"))
                                        .and_then(|c| c.as_str());
                                    let prompt = meta
                                        .as_ref()
                                        .and_then(|m| m.get("prompt"))
                                        .and_then(|p| p.as_str());
                                    let files =
                                        meta.as_ref().and_then(|m| m.get("files")).and_then(|f| {
                                            f.as_array().map(|arr| {
                                                arr.iter()
                                                    .filter_map(|v| v.as_str())
                                                    .collect::<Vec<_>>()
                                                    .join(",")
                                            })
                                        });
                                    let branch = meta
                                        .as_ref()
                                        .and_then(|m| m.get("branch"))
                                        .and_then(|b| b.as_str());

                                    match db.create_node_with_change_id(
                                        &node.change_id,
                                        &node.node_type,
                                        &node.title,
                                        node.description.as_deref(),
                                        confidence,
                                        commit,
                                        prompt,
                                        files.as_deref(),
                                        branch,
                                    ) {
                                        Ok(_) => nodes_created += 1,
                                        Err(e) => {
                                            eprintln!(
                                                "  Warning: Failed to create node {}: {}",
                                                node.change_id, e
                                            );
                                        }
                                    }
                                }

                                // Refresh node list to get local IDs
                                let all_nodes = db.get_all_nodes().unwrap_or_default();
                                let change_id_to_local_id: std::collections::HashMap<String, i32> =
                                    all_nodes
                                        .iter()
                                        .map(|n| (n.change_id.clone(), n.id))
                                        .collect();

                                // Get existing edges
                                let existing_edges = db.get_all_edges().unwrap_or_default();
                                let existing_edge_keys: std::collections::HashSet<(
                                    String,
                                    String,
                                    String,
                                )> = existing_edges
                                    .iter()
                                    .filter_map(|e| match (&e.from_change_id, &e.to_change_id) {
                                        (Some(from), Some(to)) => {
                                            Some((from.clone(), to.clone(), e.edge_type.clone()))
                                        }
                                        _ => None,
                                    })
                                    .collect();

                                // Create edges
                                for edge in state.edges.values() {
                                    let edge_key = (
                                        edge.from_change_id.clone(),
                                        edge.to_change_id.clone(),
                                        edge.edge_type.clone(),
                                    );

                                    if existing_edge_keys.contains(&edge_key) {
                                        continue;
                                    }

                                    let from_id = change_id_to_local_id.get(&edge.from_change_id);
                                    let to_id = change_id_to_local_id.get(&edge.to_change_id);

                                    match (from_id, to_id) {
                                        (Some(&from), Some(&to)) => {
                                            match db.create_edge(
                                                from,
                                                to,
                                                &edge.edge_type,
                                                edge.rationale.as_deref(),
                                            ) {
                                                Ok(_) => edges_created += 1,
                                                Err(e) => {
                                                    eprintln!(
                                                        "  Warning: Failed to create edge: {}",
                                                        e
                                                    );
                                                    edges_failed += 1;
                                                }
                                            }
                                        }
                                        _ => {
                                            edges_failed += 1;
                                        }
                                    }
                                }

                                println!();
                                println!(
                                    "{} Created {} nodes ({} skipped), {} edges ({} failed)",
                                    "Done:".green(),
                                    nodes_created,
                                    nodes_skipped,
                                    edges_created,
                                    edges_failed
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    }
                }

                EventsAction::Checkpoint { clear_events } => {
                    match EventLog::new(&deciduous_dir, author) {
                        Ok(event_log) => {
                            // Get current database state
                            let nodes = match db.get_all_nodes() {
                                Ok(n) => n,
                                Err(e) => {
                                    eprintln!("{} Getting nodes: {}", "Error:".red(), e);
                                    std::process::exit(1);
                                }
                            };

                            let edges = match db.get_all_edges() {
                                Ok(e) => e,
                                Err(e) => {
                                    eprintln!("{} Getting edges: {}", "Error:".red(), e);
                                    std::process::exit(1);
                                }
                            };

                            // Convert to checkpoint format
                            let checkpoint = Checkpoint {
                                created_at: chrono::Utc::now(),
                                nodes: nodes
                                    .iter()
                                    .map(|n| CheckpointNode {
                                        change_id: n.change_id.clone(),
                                        node_type: n.node_type.clone(),
                                        title: n.title.clone(),
                                        description: n.description.clone(),
                                        status: n.status.clone(),
                                        metadata_json: n.metadata_json.clone(),
                                        created_at: n.created_at.clone(),
                                        updated_at: n.updated_at.clone(),
                                    })
                                    .collect(),
                                edges: edges
                                    .iter()
                                    .filter_map(|e| match (&e.from_change_id, &e.to_change_id) {
                                        (Some(from), Some(to)) => Some(CheckpointEdge {
                                            edge_id: generate_edge_id(from, to, &e.edge_type),
                                            from_change_id: from.clone(),
                                            to_change_id: to.clone(),
                                            edge_type: e.edge_type.clone(),
                                            rationale: e.rationale.clone(),
                                            created_at: e.created_at.clone(),
                                        }),
                                        _ => None,
                                    })
                                    .collect(),
                                version: "1.0".to_string(),
                                themes: vec![],
                                node_themes: vec![],
                                documents: vec![],
                            };

                            match event_log.save_checkpoint(&checkpoint, clear_events) {
                                Ok(()) => {
                                    println!(
                                        "{} Checkpoint created: {} nodes, {} edges",
                                        "Success:".green(),
                                        checkpoint.nodes.len(),
                                        checkpoint.edges.len()
                                    );
                                    if clear_events {
                                        println!("  Event logs cleared");
                                    }
                                }
                                Err(e) => {
                                    eprintln!("{} Saving checkpoint: {}", "Error:".red(), e);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    }
                }

                EventsAction::Emit { node_id } => {
                    // Get the node
                    let node = match db.get_node(node_id) {
                        Ok(Some(n)) => n,
                        Ok(None) => {
                            eprintln!("{} Node {} not found", "Error:".red(), node_id);
                            std::process::exit(1);
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    };

                    match EventLog::new(&deciduous_dir, author.clone()) {
                        Ok(event_log) => {
                            let event = Event::AddNode {
                                change_id: node.change_id.clone(),
                                node_type: node.node_type.clone(),
                                title: node.title.clone(),
                                description: node.description.clone(),
                                status: node.status.clone(),
                                metadata_json: node.metadata_json.clone(),
                                timestamp: chrono::Utc::now(),
                                author,
                            };

                            match event_log.append(event) {
                                Ok(()) => {
                                    println!(
                                        "{} Emitted event for node {} ({})",
                                        "Success:".green(),
                                        node_id,
                                        node.change_id
                                    );
                                }
                                Err(e) => {
                                    eprintln!("{} {}", "Error:".red(), e);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    }
                }
            }
        }

        // ================================================================
        // Document Attachment Commands
        // ================================================================
        Command::Doc { action } => match action {
            DocAction::Attach {
                node_id,
                file,
                description,
                ai_describe,
            } => {
                if !file.exists() {
                    eprintln!("{} File not found: {}", "Error:".red(), file.display());
                    std::process::exit(1);
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
                        std::process::exit(1);
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
                let docs_dir = PathBuf::from(".deciduous/documents");
                if let Err(e) = std::fs::create_dir_all(&docs_dir) {
                    eprintln!("{} Failed to create documents dir: {}", "Error:".red(), e);
                    std::process::exit(1);
                }

                let dest_path = docs_dir.join(&storage_filename);
                if !dest_path.exists() {
                    if let Err(e) = std::fs::copy(&file, &dest_path) {
                        eprintln!("{} Failed to copy file: {}", "Error:".red(), e);
                        std::process::exit(1);
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
                        std::process::exit(1);
                    }
                }
            }

            DocAction::List {
                node_id,
                include_detached,
                json,
            } => match db.get_node_documents(node_id, include_detached) {
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
                    std::process::exit(1);
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
                            std::process::exit(1);
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    };
                    let file_path =
                        PathBuf::from(".deciduous/documents").join(&doc.storage_filename);
                    match generate_ai_description(&doc.original_filename, &file_path) {
                        Some(d) => (d, "ai"),
                        None => {
                            eprintln!("{} Could not generate AI description", "Error:".red());
                            std::process::exit(1);
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
                        std::process::exit(1);
                    }
                }
            }

            DocAction::Detach { doc_id } => match db.detach_document(doc_id) {
                Ok(()) => println!("{} document {}", "Detached".red(), doc_id),
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
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
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            },

            DocAction::Open { doc_id } => match db.get_document(doc_id) {
                Ok(Some(doc)) => {
                    let file_path =
                        PathBuf::from(".deciduous/documents").join(&doc.storage_filename);
                    if !file_path.exists() {
                        eprintln!(
                            "{} File not found on disk: {}",
                            "Error:".red(),
                            file_path.display()
                        );
                        std::process::exit(1);
                    }

                    // Copy to temp with original filename for better OS handling
                    let temp_dir = std::env::temp_dir().join("deciduous-docs");
                    std::fs::create_dir_all(&temp_dir).ok();
                    let temp_path = temp_dir.join(&doc.original_filename);
                    if let Err(e) = std::fs::copy(&file_path, &temp_path) {
                        eprintln!("{} Failed to copy file: {}", "Error:".red(), e);
                        std::process::exit(1);
                    }

                    #[cfg(target_os = "macos")]
                    let open_cmd = "open";
                    #[cfg(not(target_os = "macos"))]
                    let open_cmd = "xdg-open";

                    match std::process::Command::new(open_cmd).arg(&temp_path).spawn() {
                        Ok(_) => println!("{} {}", "Opened".green(), doc.original_filename),
                        Err(e) => {
                            eprintln!("{} Failed to open file: {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    }
                }
                Ok(None) => {
                    eprintln!("{} Document {} not found", "Error:".red(), doc_id);
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            },

            DocAction::Gc { dry_run } => {
                let docs_dir = PathBuf::from(".deciduous/documents");
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
                    std::process::exit(1);
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
                    std::process::exit(1);
                }
            },

            ThemesAction::Delete { name } => match db.delete_theme(&name) {
                Ok(true) => println!("{} theme '{}'", "Deleted".red(), name),
                Ok(false) => {
                    eprintln!("{} Theme '{}' not found", "Error:".red(), name);
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            },
        },

        // ================================================================
        // Tag Commands
        // ================================================================
        Command::Tag { action } => match action {
            TagAction::Add { node_id, theme } => match db.tag_node(node_id, &theme, "manual") {
                Ok(()) => println!("{} theme '{}' to node {}", "Tagged".green(), theme, node_id),
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            },

            TagAction::Remove { node_id, theme } => match db.untag_node(node_id, &theme) {
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
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            },

            TagAction::List { node_id } => match db.get_node_themes(node_id) {
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
                    std::process::exit(1);
                }
            },

            TagAction::Suggest { node_id, apply } => {
                let nodes_to_check: Vec<deciduous::DecisionNode> = if let Some(id) = node_id {
                    match db.get_node(id) {
                        Ok(Some(n)) => vec![n],
                        Ok(None) => {
                            eprintln!("{} Node {} not found", "Error:".red(), id);
                            std::process::exit(1);
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            std::process::exit(1);
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

            TagAction::Confirm { node_id, theme } => match db.confirm_tag(node_id, &theme) {
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
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            },
        },

        Command::Completion { .. } => unreachable!(), // Handled above
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
                std::process::exit(1);
            }

            // Get all nodes
            let nodes = match db.get_all_nodes() {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            };

            // Get git commits since Nov 2024
            let commits = get_git_commits_for_audit();
            if commits.is_empty() {
                eprintln!("{} No git commits found", "Error:".red());
                std::process::exit(1);
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
                    !n.commit().is_some_and(|s| !s.is_empty())
                })
                .collect();

            let with_commits = nodes
                .iter()
                .filter(|n| n.node_type == "action" || n.node_type == "outcome")
                .filter(|n| n.commit().is_some_and(|s| !s.is_empty()))
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
                            std::process::exit(1);
                        }
                    }
                } else {
                    deciduous::pulse::print_pulse(&report, summary);
                }
            }
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                std::process::exit(1);
            }
        },

        Command::Narratives { action } => match action {
            NarrativesAction::Init { output, force } => {
                let path = output.unwrap_or_else(|| PathBuf::from(".deciduous/narratives.md"));
                if let Err(e) = deciduous::narratives::init_narratives(&db, &path, force) {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
            NarrativesAction::Show { path } => {
                let p = path.unwrap_or_else(|| PathBuf::from(".deciduous/narratives.md"));
                match deciduous::narratives::show_narratives(&p) {
                    Ok(content) => print!("{}", content),
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        std::process::exit(1);
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
                                    std::process::exit(1);
                                }
                            }
                        } else {
                            deciduous::narratives::print_pivots(&pivots);
                        }
                    }
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        std::process::exit(1);
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
                        std::process::exit(1);
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
                                    std::process::exit(1);
                                }
                            }
                        } else {
                            deciduous::archaeology::print_timeline(&nodes);
                        }
                    }
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        std::process::exit(1);
                    }
                }
            }
            ArchaeologyAction::Supersede {
                id,
                cascade,
                dry_run,
            } => match deciduous::archaeology::supersede(&db, id, cascade, dry_run) {
                Ok(result) => {
                    deciduous::archaeology::print_supersede_result(&result, dry_run);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
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
                        std::process::exit(1);
                    }

                    // Parse the roadmap
                    let parsed = match parse_roadmap(&roadmap_path) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("{} Parsing roadmap: {}", "Error:".red(), e);
                            std::process::exit(1);
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
                            std::process::exit(1);
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
                            std::process::exit(1);
                        }
                    };
                    if let Err(e) = std::fs::write(&roadmap_path, &updated) {
                        eprintln!("{} Writing file: {}", "Error:".red(), e);
                        std::process::exit(1);
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
                        std::process::exit(1);
                    }

                    // Clear existing roadmap items
                    let cleared = match db.clear_roadmap_items() {
                        Ok(n) => n,
                        Err(e) => {
                            eprintln!("{} Clearing roadmap items: {}", "Error:".red(), e);
                            std::process::exit(1);
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
                            std::process::exit(1);
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
                        std::process::exit(1);
                    }

                    // Initialize GitHub client
                    let gh_client = match repo {
                        Some(r) => GitHubClient::new(Some(r)),
                        None => match GitHubClient::auto_detect() {
                            Ok(c) => c,
                            Err(e) => {
                                eprintln!("{} Auto-detecting repo: {}", "Error:".red(), e);
                                eprintln!("Specify repo with --repo owner/repo");
                                std::process::exit(1);
                            }
                        },
                    };

                    // Check auth
                    match GitHubClient::check_auth() {
                        Ok(true) => {}
                        Ok(false) | Err(_) => {
                            eprintln!("{} Not authenticated with GitHub", "Error:".red());
                            eprintln!("Run 'gh auth login' first");
                            std::process::exit(1);
                        }
                    }

                    // Parse roadmap
                    let parsed = match parse_roadmap(&roadmap_path) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("{} Parsing roadmap: {}", "Error:".red(), e);
                            std::process::exit(1);
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
                        std::process::exit(1);
                    }

                    let parsed = match parse_roadmap(&roadmap_path) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("{} Parsing roadmap: {}", "Error:".red(), e);
                            std::process::exit(1);
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
                    // Find roadmap item by title or change_id
                    let items = match db.get_all_roadmap_items() {
                        Ok(i) => i,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            std::process::exit(1);
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
                                                    std::process::exit(1);
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
                                            std::process::exit(1);
                                        }
                                        None => {
                                            eprintln!(
                                                "{} Node #{} not found",
                                                "Error:".red(),
                                                outcome_id
                                            );
                                            std::process::exit(1);
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("{} {}", "Error:".red(), e);
                                    std::process::exit(1);
                                }
                            }
                        }
                        None => {
                            eprintln!("{} Roadmap item '{}' not found", "Error:".red(), item);
                            eprintln!("Run 'deciduous roadmap list' to see available items");
                            std::process::exit(1);
                        }
                    }
                }

                RoadmapAction::Unlink { item } => {
                    let items = match db.get_all_roadmap_items() {
                        Ok(i) => i,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            std::process::exit(1);
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
                                    std::process::exit(1);
                                }
                            }
                        }
                        None => {
                            eprintln!("{} Roadmap item '{}' not found", "Error:".red(), item);
                            std::process::exit(1);
                        }
                    }
                }

                RoadmapAction::Conflicts { resolve } => {
                    let conflicts = match db.get_unresolved_conflicts() {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            std::process::exit(1);
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
                            std::process::exit(1);
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
                            std::process::exit(1);
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
                        std::process::exit(1);
                    }
                    println!("\n{}", "Hooks installed successfully!".green().bold());
                    println!();
                    println!("The following hooks are now active:");
                    println!(
                        "  • {} - blocks Edit/Write without recent action node",
                        "require-action-node".cyan()
                    );
                    println!(
                        "  • {} - reminds to link commits to graph",
                        "post-commit-reminder".cyan()
                    );
                    println!();
                }
                HooksAction::Status {} => {
                    if let Err(e) = deciduous::hooks::hooks_status() {
                        eprintln!("{} {}", "Error:".red(), e);
                        std::process::exit(1);
                    }
                }
                HooksAction::Uninstall {} => {
                    let project_root = Config::find_project_root().unwrap_or_else(|| {
                        std::env::current_dir().expect("Could not get current directory")
                    });

                    println!("\n{}", "Uninstalling Claude Code hooks...".cyan().bold());
                    if let Err(e) = deciduous::hooks::uninstall_hooks(&project_root) {
                        eprintln!("{} {}", "Error:".red(), e);
                        std::process::exit(1);
                    }
                    println!("\n{}", "Hooks uninstalled.".green().bold());
                    println!();
                }
            }
        }

        Command::Integration {} => {
            if let Err(e) = deciduous::hooks::integration_status() {
                eprintln!("{} {}", "Error:".red(), e);
                std::process::exit(1);
            }
        }

        Command::Opencode { action } => match action {
            OpencodeAction::Install {} => {
                let project_root = Config::find_project_root().unwrap_or_else(|| {
                    std::env::current_dir().expect("Could not get current directory")
                });

                if let Err(e) = deciduous::opencode::install_opencode(&project_root) {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
            OpencodeAction::Status {} => {
                if let Err(e) = deciduous::opencode::opencode_status() {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
            OpencodeAction::Uninstall {} => {
                let project_root = Config::find_project_root().unwrap_or_else(|| {
                    std::env::current_dir().expect("Could not get current directory")
                });

                println!("\n{}", "Uninstalling OpenCode integration...".cyan().bold());
                if let Err(e) = deciduous::opencode::uninstall_opencode(&project_root) {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
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

/// Parse a `--date` value into RFC3339, accepting RFC3339, "YYYY-MM-DD HH:MM:SS",
/// or "YYYY-MM-DD" (start of day). Local times that don't exist (DST gaps) or are
/// ambiguous resolve via `earliest()` instead of panicking; unparseable input is
/// passed through as-is with a warning.
fn parse_backdate(d: &str) -> String {
    // Try parsing as RFC3339 first
    if chrono::DateTime::parse_from_rfc3339(d).is_ok() {
        return d.to_string();
    }

    // Try "YYYY-MM-DD HH:MM:SS" format
    let naive = chrono::NaiveDateTime::parse_from_str(d, "%Y-%m-%d %H:%M:%S")
        .ok()
        // Try "YYYY-MM-DD" format (set to start of day)
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
        });

    if let Some(dt) = naive {
        if let Some(local) = chrono::Local.from_local_datetime(&dt).earliest() {
            return local.to_rfc3339();
        }
    }

    // Fallback: use as-is and hope for the best
    eprintln!(
        "{} Could not parse date '{}'. Use RFC3339 or YYYY-MM-DD format.",
        "Warning:".yellow(),
        d
    );
    d.to_string()
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
// Git history export helpers
// =============================================================================

/// Extract all unique commit hashes from nodes' metadata_json
fn extract_commit_hashes(nodes: &[deciduous::DecisionNode]) -> Vec<String> {
    let mut hashes = std::collections::HashSet::new();
    for node in nodes {
        if let Some(commit) = node.commit() {
            if !commit.is_empty() {
                hashes.insert(commit);
            }
        }
    }
    hashes.into_iter().collect()
}

/// Generate git-history.json for all commits linked to nodes
fn export_git_history(
    nodes: &[deciduous::DecisionNode],
    output_dir: &std::path::Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    let hashes = extract_commit_hashes(nodes);
    let mut commits: Vec<deciduous::util::GitCommit> = Vec::new();

    for hash in &hashes {
        if let Some(commit) = deciduous::util::get_git_commit_info(hash, None) {
            commits.push(commit);
        }
    }

    // Sort by date (newest first)
    commits.sort_by(|a, b| b.date.cmp(&a.date));

    let json = serde_json::to_string_pretty(&commits)?;
    let output_path = output_dir.join("git-history.json");
    std::fs::write(&output_path, &json)?;

    Ok(commits.len())
}

// =============================================================================
// Tests
// =============================================================================

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

    // === truncate Tests ===

    #[test]
    fn test_truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 80), "hello");
    }

    #[test]
    fn test_truncate_long_ascii() {
        let s = "a".repeat(100);
        let result = truncate(&s, 80);
        assert_eq!(result.chars().count(), 80);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_multibyte_at_boundary() {
        // Regression test: byte-slicing `&desc[..77]` panicked when a
        // multi-byte character straddled the boundary. Cyrillic chars are
        // 2 bytes each, so every odd byte index is mid-character.
        let s = "\u{0434}".repeat(100); // "д" x 100 (200 bytes)
        let result = truncate(&s, 80);
        assert_eq!(result.chars().count(), 80);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_emoji_description() {
        // 4-byte emoji near the boundary must not panic
        let s = format!("{}{}", "x".repeat(76), "\u{1F600}".repeat(10));
        let result = truncate(&s, 80);
        assert_eq!(result.chars().count(), 80);
        assert!(result.ends_with("..."));
    }

    // === parse_backdate Tests ===

    #[test]
    fn test_parse_backdate_rfc3339_passthrough() {
        let input = "2025-01-15T10:30:00+00:00";
        assert_eq!(parse_backdate(input), input);
    }

    #[test]
    fn test_parse_backdate_date_only() {
        let result = parse_backdate("2025-01-15");
        let parsed = chrono::DateTime::parse_from_rfc3339(&result)
            .expect("date-only input should produce valid RFC3339");
        assert!(result.starts_with("2025-01-15"), "got {}", result);
        let _ = parsed;
    }

    #[test]
    fn test_parse_backdate_datetime() {
        let result = parse_backdate("2025-01-15 10:30:00");
        assert!(
            chrono::DateTime::parse_from_rfc3339(&result).is_ok(),
            "datetime input should produce valid RFC3339, got {}",
            result
        );
    }

    #[test]
    fn test_parse_backdate_dst_gap_does_not_panic() {
        // Regression test: 2:30 AM on 2025-03-09 does not exist in US
        // timezones (DST spring-forward). The old code called .unwrap() on
        // from_local_datetime and panicked. Depending on the host timezone
        // this either resolves to a valid RFC3339 time or falls back to
        // passing the input through -- but it must never panic.
        let input = "2025-03-09 02:30:00";
        let result = parse_backdate(input);
        assert!(
            result == input || chrono::DateTime::parse_from_rfc3339(&result).is_ok(),
            "expected passthrough or valid RFC3339, got {}",
            result
        );
    }

    #[test]
    fn test_parse_backdate_garbage_passthrough() {
        assert_eq!(parse_backdate("not a date"), "not a date");
    }
}
