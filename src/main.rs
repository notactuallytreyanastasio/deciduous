use chrono::Local;
use clap::{CommandFactory, Parser, Subcommand};
use colored::Colorize;
use deciduous::github::{ensure_roadmap_label, GitHubClient};
use deciduous::roadmap::{
    generate_issue_body, parse_roadmap, write_roadmap_with_metadata, RoadmapSection,
};
use deciduous::{
    filter_graph_by_ids, generate_pr_writeup, graph_to_dot, parse_node_range, Database, DotConfig,
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
    Init {
        /// Initialize for Claude Code (creates .claude/commands/ and CLAUDE.md)
        #[arg(long, group = "editor")]
        claude: bool,

        /// Initialize for Windsurf (creates .windsurf/rules/ and AGENTS.md)
        #[arg(long, group = "editor")]
        windsurf: bool,

        /// Initialize for OpenCode (creates .opencode/command/ and AGENTS.md)
        #[arg(long, group = "editor")]
        opencode: bool,
    },

    /// Update tooling files to latest version (overwrites existing)
    Update {
        /// Update Claude Code files (.claude/commands/, CLAUDE.md)
        #[arg(long, group = "editor")]
        claude: bool,

        /// Update Windsurf files (.windsurf/rules/, AGENTS.md)
        #[arg(long, group = "editor")]
        windsurf: bool,

        /// Update OpenCode files (.opencode/command/, AGENTS.md)
        #[arg(long, group = "editor")]
        opencode: bool,
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

        /// Files associated with this node (comma-separated)
        #[arg(short, long)]
        files: Option<String>,

        /// Git branch (auto-detected if not specified)
        #[arg(short, long)]
        branch: Option<String>,

        /// Skip auto-detection of git branch
        #[arg(long)]
        no_branch: bool,
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

    /// Update node status
    Status {
        /// Node ID
        id: i32,

        /// New status: pending, active, completed, rejected
        status: String,
    },

    /// List all nodes
    Nodes {
        /// Filter by git branch
        #[arg(short, long)]
        branch: Option<String>,

        /// Filter by node type (goal, decision, action, etc.)
        #[arg(short = 't', long)]
        node_type: Option<String>,
    },

    /// List all edges
    Edges,

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

    /// Export or apply graph diff patches for multi-user sync
    Diff {
        #[command(subcommand)]
        action: DiffAction,
    },

    /// Migrate database to add change_id columns (for multi-user sync)
    Migrate,

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

    /// Launch the terminal user interface
    Tui {
        /// Optional database path (default: auto-discover)
        #[arg(short, long)]
        db: Option<PathBuf>,
    },

    /// Manage ROADMAP.md sync with GitHub Issues
    Roadmap {
        #[command(subcommand)]
        action: RoadmapAction,
    },

    /// Generate shell completions
    Completion {
        /// Shell type: bash, zsh, fish, powershell, elvish
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand, Debug)]
enum DiffAction {
    /// Export nodes as a patch file
    Export {
        /// Output file path (required)
        #[arg(short, long)]
        output: PathBuf,

        /// Node IDs or ranges to export (e.g., "1-11" or "1,3,5-10")
        #[arg(short, long)]
        nodes: Option<String>,

        /// Filter by git branch
        #[arg(short, long)]
        branch: Option<String>,

        /// Author name to include in patch
        #[arg(short, long)]
        author: Option<String>,

        /// Git commit hash at time of export
        #[arg(long)]
        base_commit: Option<String>,
    },

    /// Apply a patch file to local database
    Apply {
        /// Patch file(s) to apply
        files: Vec<PathBuf>,

        /// Show what would be applied without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Show status of unapplied patches
    Status {
        /// Directory to scan for patches (default: .deciduous/patches/)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Validate a patch file (check for missing node references)
    Validate {
        /// Patch file(s) to validate
        files: Vec<PathBuf>,
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

fn main() {
    let args = Args::parse();

    // Handle init separately - it doesn't need an existing database
    if let Command::Init {
        claude: _,
        windsurf,
        opencode,
    } = args.command
    {
        // Determine editor type: default to Claude if neither specified
        let editor = if windsurf {
            deciduous::init::Editor::Windsurf
        } else if opencode {
            deciduous::init::Editor::Opencode
        } else {
            deciduous::init::Editor::Claude
        };

        if let Err(e) = deciduous::init::init_project(editor) {
            eprintln!("{} {}", "Error:".red(), e);
            std::process::exit(1);
        }
        return;
    }

    // Handle update separately - it doesn't need an existing database
    if let Command::Update {
        claude: _,
        windsurf,
        opencode,
    } = args.command
    {
        // Determine editor type: default to Claude if neither specified
        let editor = if windsurf {
            deciduous::init::Editor::Windsurf
        } else if opencode {
            deciduous::init::Editor::Opencode
        } else {
            deciduous::init::Editor::Claude
        };

        if let Err(e) = deciduous::init::update_tooling(editor) {
            eprintln!("{} {}", "Error:".red(), e);
            std::process::exit(1);
        }
        return;
    }

    // Handle TUI separately - it has its own event loop
    if let Command::Tui { db } = args.command {
        if let Err(e) = deciduous::tui::run(db) {
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
        Command::Add {
            node_type,
            title,
            description,
            confidence,
            commit,
            prompt,
            files,
            branch,
            no_branch,
        } => {
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

            match db.create_node_full(
                &node_type,
                &title,
                description.as_deref(),
                confidence,
                effective_commit.as_deref(),
                prompt.as_deref(),
                files.as_deref(),
                effective_branch.as_deref(),
            ) {
                Ok(id) => {
                    let conf_str = confidence
                        .map(|c| format!(" [confidence: {}%]", c))
                        .unwrap_or_default();
                    let commit_str = effective_commit
                        .as_ref()
                        .map(|c| format!(" [commit: {}]", &c[..7.min(c.len())]))
                        .unwrap_or_default();
                    let prompt_str = if prompt.is_some() {
                        " [prompt saved]"
                    } else {
                        ""
                    };
                    let files_str = files
                        .as_ref()
                        .map(|f| format!(" [files: {}]", f))
                        .unwrap_or_default();
                    let branch_str = effective_branch
                        .as_ref()
                        .map(|b| format!(" [branch: {}]", b))
                        .unwrap_or_default();
                    println!(
                        "{} node {} (type: {}, title: {}){}{}{}{}{}",
                        "Created".green(),
                        id,
                        node_type,
                        title,
                        conf_str,
                        commit_str,
                        prompt_str,
                        files_str,
                        branch_str
                    );
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
            }
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                std::process::exit(1);
            }
        },

        Command::Status { id, status } => match db.update_node_status(id, &status) {
            Ok(()) => println!("{} node {} status to '{}'", "Updated".green(), id, status),
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                std::process::exit(1);
            }
        },

        Command::Nodes { branch, node_type } => {
            match db.get_all_nodes() {
                Ok(nodes) => {
                    // Filter nodes by branch and/or type
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
                            branch_match && type_match
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

            match db.get_graph() {
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
                                                    if let Err(e) =
                                                        export_git_history(&graph.nodes, &demo_dir)
                                                    {
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

        Command::Diff { action } => {
            match action {
                DiffAction::Export {
                    output,
                    nodes,
                    branch,
                    author,
                    base_commit,
                } => {
                    // Parse node IDs if provided
                    let node_ids = nodes.as_ref().map(|n| parse_node_range(n));

                    match db.export_patch(node_ids, branch.as_deref(), author, base_commit) {
                        Ok(patch) => match patch.save(&output) {
                            Ok(()) => {
                                println!(
                                    "{} Exported {} nodes and {} edges to {}",
                                    "Success:".green(),
                                    patch.nodes.len(),
                                    patch.edges.len(),
                                    output.display()
                                );
                            }
                            Err(e) => {
                                eprintln!("{} {}", "Error:".red(), e);
                                std::process::exit(1);
                            }
                        },
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    }
                }

                DiffAction::Apply { files, dry_run } => {
                    let mut total_added = 0;
                    let mut total_skipped = 0;
                    let mut total_edges_added = 0;
                    let mut total_edges_skipped = 0;

                    for file in files {
                        match deciduous::GraphPatch::load(&file) {
                            Ok(patch) => match db.apply_patch(&patch, dry_run) {
                                Ok(result) => {
                                    if dry_run {
                                        println!(
                                            "{} {} (dry run)",
                                            "Would apply:".cyan(),
                                            file.display()
                                        );
                                    } else {
                                        println!("{} {}", "Applied:".green(), file.display());
                                    }
                                    println!(
                                        "  Nodes: {} added, {} skipped",
                                        result.nodes_added, result.nodes_skipped
                                    );
                                    println!(
                                        "  Edges: {} added, {} skipped",
                                        result.edges_added, result.edges_skipped
                                    );
                                    if !result.edges_failed.is_empty() {
                                        println!(
                                            "  {} edges failed (missing nodes):",
                                            result.edges_failed.len()
                                        );
                                        for msg in &result.edges_failed {
                                            println!("    - {}", msg);
                                        }
                                    }
                                    total_added += result.nodes_added;
                                    total_skipped += result.nodes_skipped;
                                    total_edges_added += result.edges_added;
                                    total_edges_skipped += result.edges_skipped;
                                }
                                Err(e) => {
                                    eprintln!(
                                        "{} Applying {}: {}",
                                        "Error:".red(),
                                        file.display(),
                                        e
                                    );
                                }
                            },
                            Err(e) => {
                                eprintln!("{} Loading {}: {}", "Error:".red(), file.display(), e);
                            }
                        }
                    }

                    if !dry_run {
                        println!(
                            "\n{} {} nodes added, {} skipped; {} edges added, {} skipped",
                            "Total:".cyan(),
                            total_added,
                            total_skipped,
                            total_edges_added,
                            total_edges_skipped
                        );
                    }
                }

                DiffAction::Status { path } => {
                    let patches_dir = path.unwrap_or_else(|| PathBuf::from(".deciduous/patches"));
                    if !patches_dir.exists() {
                        println!(
                            "{} No patches directory found at {}",
                            "Info:".cyan(),
                            patches_dir.display()
                        );
                        println!("Create one with: mkdir -p {}", patches_dir.display());
                        return;
                    }

                    // List all .json files in the directory
                    let entries = match std::fs::read_dir(&patches_dir) {
                        Ok(e) => e,
                        Err(e) => {
                            eprintln!("{} Reading directory: {}", "Error:".red(), e);
                            return;
                        }
                    };

                    println!("{} {}", "Patches in:".cyan(), patches_dir.display());
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().map(|e| e == "json").unwrap_or(false) {
                            if let Ok(patch) = deciduous::GraphPatch::load(&path) {
                                let author = patch.author.as_deref().unwrap_or("unknown");
                                let branch = patch.branch.as_deref().unwrap_or("unknown");
                                println!(
                                    "  {} - {} nodes, {} edges (author: {}, branch: {})",
                                    path.file_name().unwrap_or_default().to_string_lossy(),
                                    patch.nodes.len(),
                                    patch.edges.len(),
                                    author,
                                    branch
                                );
                            }
                        }
                    }
                }

                DiffAction::Validate { files } => {
                    use std::collections::HashSet;

                    let mut any_errors = false;

                    for file in &files {
                        match deciduous::GraphPatch::load(file) {
                            Ok(patch) => {
                                // Collect all node change_ids in the patch
                                let node_ids: HashSet<&str> =
                                    patch.nodes.iter().map(|n| n.change_id.as_str()).collect();

                                // Check each edge for missing nodes
                                let mut missing_edges = Vec::new();
                                for edge in &patch.edges {
                                    let from_missing =
                                        !node_ids.contains(edge.from_change_id.as_str());
                                    let to_missing = !node_ids.contains(edge.to_change_id.as_str());

                                    if from_missing || to_missing {
                                        let mut missing = Vec::new();
                                        if from_missing {
                                            missing.push(format!(
                                                "from: {}",
                                                &edge.from_change_id
                                                    [..8.min(edge.from_change_id.len())]
                                            ));
                                        }
                                        if to_missing {
                                            missing.push(format!(
                                                "to: {}",
                                                &edge.to_change_id
                                                    [..8.min(edge.to_change_id.len())]
                                            ));
                                        }
                                        missing_edges
                                            .push((edge.edge_type.clone(), missing.join(", ")));
                                    }
                                }

                                println!("{} {}", "Validating:".cyan(), file.display());
                                println!("  Nodes: {}", patch.nodes.len());
                                println!(
                                    "  Edges: {} ({} valid, {} with missing refs)",
                                    patch.edges.len(),
                                    patch.edges.len() - missing_edges.len(),
                                    missing_edges.len()
                                );

                                if !missing_edges.is_empty() {
                                    any_errors = true;
                                    println!(
                                        "  {} Edges referencing missing nodes:",
                                        "Warning:".yellow()
                                    );
                                    for (edge_type, missing) in &missing_edges {
                                        println!("    - {} edge: missing {}", edge_type, missing);
                                    }
                                    println!();
                                    println!("  {} This patch has edges that reference nodes not in the patch.", "Note:".cyan());
                                    println!("  When applied, these edges will fail unless the referenced nodes");
                                    println!("  already exist in the target database or are imported first.");
                                    println!();
                                    println!("  {} Re-export with all dependent nodes, or apply patches in order:", "Fix:".green());
                                    println!(
                                        "    1. Apply the patch containing the parent nodes first"
                                    );
                                    println!("    2. Then apply this patch");
                                } else {
                                    println!(
                                        "  {} All edges reference nodes within the patch",
                                        "OK:".green()
                                    );
                                }
                            }
                            Err(e) => {
                                any_errors = true;
                                eprintln!("{} {}: {}", "Error:".red(), file.display(), e);
                            }
                        }
                        println!();
                    }

                    if any_errors {
                        std::process::exit(1);
                    }
                }
            }
        }

        Command::Tui { .. } => unreachable!(), // Handled above
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
                        if section.github_issue_number.is_some() {
                            // Update existing issue
                            let issue_num = section.github_issue_number.unwrap();
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

                                        // Cache issue for TUI/Web display
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
            "Implemented --claude and --windsurf flags for init command",
            "feat: add --claude and --windsurf flags to init command",
        );
        assert!(
            score > 0.7,
            "Real example should have high match, got {}",
            score
        );
    }
}
