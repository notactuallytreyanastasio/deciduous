mod commands;

use chrono::{Local, TimeZone};
use clap::{CommandFactory, Parser, Subcommand};
use colored::Colorize;
use deciduous::{
    filter_graph_by_ids,
    generate_edge_id,
    generate_pr_writeup,
    get_current_author,
    graph_to_dot,
    parse_node_range,
    Config,
    Database,
    DotConfig,
    Event,
    EventLog,
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

    /// Generate shell completions
    Completion {
        /// Shell type: bash, zsh, fish, powershell, elvish
        shell: clap_complete::Shell,
    },
}


use commands::docs::DocAction;
use commands::roadmap::RoadmapAction;
use commands::sync::EventsAction;

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
                // Fallback: use as-is and hope for the best
                else {
                    eprintln!(
                        "{} Could not parse date '{}'. Use RFC3339 or YYYY-MM-DD format.",
                        "Warning:".yellow(),
                        d
                    );
                    d.clone()
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
                        .map(|c| format!(" [commit: {}]", &c[..7.min(c.len())]))
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
                    let sync_dir = PathBuf::from(".deciduous/sync");
                    if sync_dir.exists() {
                        if let Ok(Some(node)) = db.get_node(id) {
                            let author = get_current_author();
                            if let Ok(event_log) =
                                EventLog::new(&PathBuf::from(".deciduous"), author.clone())
                            {
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
                                if let Err(e) = event_log.append(event) {
                                    eprintln!("{} Sync event: {}", "Warning:".yellow(), e);
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
                let sync_dir = PathBuf::from(".deciduous/sync");
                if sync_dir.exists() {
                    // Get change_ids for the nodes
                    let from_node = db.get_node(from).ok().flatten();
                    let to_node = db.get_node(to).ok().flatten();

                    if let (Some(from_n), Some(to_n)) = (from_node, to_node) {
                        let author = get_current_author();
                        if let Ok(event_log) =
                            EventLog::new(&PathBuf::from(".deciduous"), author.clone())
                        {
                            let event = Event::AddEdge {
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
                            };
                            if let Err(e) = event_log.append(event) {
                                eprintln!("{} Sync event: {}", "Warning:".yellow(), e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                std::process::exit(1);
            }
        },

        Command::Unlink { from, to } => {
            // Get node info before deletion for event emission
            let from_node = db.get_node(from).ok().flatten();
            let to_node = db.get_node(to).ok().flatten();

            match db.delete_edge(from, to) {
                Ok(()) => {
                    println!("{} edge ({} -> {})", "Removed".red(), from, to);

                    // Auto-emit event if sync is initialized
                    let sync_dir = PathBuf::from(".deciduous/sync");
                    if sync_dir.exists() {
                        if let (Some(from_n), Some(to_n)) = (from_node, to_node) {
                            let author = get_current_author();
                            if let Ok(event_log) =
                                EventLog::new(&PathBuf::from(".deciduous"), author.clone())
                            {
                                // We need to figure out the edge_type for the edge_id
                                // For now, use "leads_to" as default since we don't have the edge info
                                let edge_id = generate_edge_id(
                                    &from_n.change_id,
                                    &to_n.change_id,
                                    "leads_to",
                                );
                                let event = Event::DeleteEdge {
                                    edge_id,
                                    timestamp: chrono::Utc::now(),
                                    author,
                                };
                                if let Err(e) = event_log.append(event) {
                                    eprintln!("{} Sync event: {}", "Warning:".yellow(), e);
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
                        let sync_dir = PathBuf::from(".deciduous/sync");
                        if sync_dir.exists() {
                            if let Some(node) = node_info {
                                let author = get_current_author();
                                if let Ok(event_log) =
                                    EventLog::new(&PathBuf::from(".deciduous"), author.clone())
                                {
                                    let event = Event::DeleteNode {
                                        change_id: node.change_id.clone(),
                                        timestamp: chrono::Utc::now(),
                                        author,
                                    };
                                    if let Err(e) = event_log.append(event) {
                                        eprintln!("{} Sync event: {}", "Warning:".yellow(), e);
                                    }
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

        Command::Status { id, status } => match db.update_node_status(id, &status) {
            Ok(()) => {
                println!("{} node {} status to '{}'", "Updated".green(), id, status);

                // Auto-emit event if sync is initialized
                let sync_dir = PathBuf::from(".deciduous/sync");
                if sync_dir.exists() {
                    if let Ok(Some(node)) = db.get_node(id) {
                        let author = get_current_author();
                        if let Ok(event_log) =
                            EventLog::new(&PathBuf::from(".deciduous"), author.clone())
                        {
                            let event = Event::UpdateNode {
                                change_id: node.change_id.clone(),
                                title: None,
                                description: None,
                                status: Some(status.clone()),
                                metadata_json: None,
                                timestamp: chrono::Utc::now(),
                                author,
                            };
                            if let Err(e) = event_log.append(event) {
                                eprintln!("{} Sync event: {}", "Warning:".yellow(), e);
                            }
                        }
                    }
                }
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
                            println!(
                                "{:<5} {:<12} {:<10} {}",
                                n.id, type_colored, n.status, n.title
                            );
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

        Command::Events { action } => commands::sync::handle_events(&db, action),


        // ================================================================
        // Document Attachment Commands
        // ================================================================
        Command::Doc { action } => commands::docs::handle_doc(&db, action),

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

        Command::Roadmap { action } => commands::roadmap::handle_roadmap(&db, action),

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

pub(crate) fn truncate(s: &str, max_len: usize) -> String {
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
// Git history export helpers
// =============================================================================

/// Git commit info for timeline view (matches web/src/types/graph.ts GitCommit)
#[derive(serde::Serialize)]
struct GitCommit {
    hash: String,
    short_hash: String,
    author: String,
    date: String,
    message: String,
    files_changed: Option<u32>,
}

/// Extract all unique commit hashes from nodes' metadata_json
fn extract_commit_hashes(nodes: &[deciduous::DecisionNode]) -> Vec<String> {
    let mut hashes = std::collections::HashSet::new();
    for node in nodes {
        if let Some(ref meta_json) = node.metadata_json {
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_json) {
                if let Some(commit) = meta.get("commit").and_then(|c| c.as_str()) {
                    if !commit.is_empty() {
                        hashes.insert(commit.to_string());
                    }
                }
            }
        }
    }
    hashes.into_iter().collect()
}

/// Get commit info from git for a given hash
fn get_git_commit_info(hash: &str) -> Option<GitCommit> {
    // Get commit info: hash, author, date (ISO), full message body
    // Use %x00 (null byte) as separator since message can have newlines
    let output = ProcessCommand::new("git")
        .args(["log", "-1", "--format=%H%x00%an%x00%aI%x00%B", hash])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split('\x00').collect();
    if parts.len() < 4 {
        return None;
    }

    // Clean up the message - trim whitespace
    let message = parts[3].trim().to_string();

    // Get files changed count
    let files_output = ProcessCommand::new("git")
        .args(["diff-tree", "--no-commit-id", "--name-only", "-r", hash])
        .output()
        .ok();

    let files_changed = files_output.and_then(|o| {
        if o.status.success() {
            let count = String::from_utf8_lossy(&o.stdout).trim().lines().count();
            Some(count as u32)
        } else {
            None
        }
    });

    Some(GitCommit {
        hash: parts[0].to_string(),
        short_hash: parts[0].chars().take(7).collect(),
        author: parts[1].to_string(),
        date: parts[2].to_string(),
        message,
        files_changed,
    })
}

/// Generate git-history.json for all commits linked to nodes
fn export_git_history(
    nodes: &[deciduous::DecisionNode],
    output_dir: &std::path::Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    let hashes = extract_commit_hashes(nodes);
    let mut commits: Vec<GitCommit> = Vec::new();

    for hash in &hashes {
        if let Some(commit) = get_git_commit_info(hash) {
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
}
