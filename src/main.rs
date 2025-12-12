use chrono::Local;
use clap::{Parser, Subcommand};
use colored::Colorize;
use deciduous::{Database, DotConfig, WriteupConfig, graph_to_dot, generate_pr_writeup, filter_graph_by_ids, parse_node_range};
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

#[derive(Parser, Debug)]
#[command(name = "deciduous")]
#[command(author, version, about = "Decision graph tooling for AI-assisted development")]
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
    },

    /// Update tooling files to latest version (overwrites existing)
    Update {
        /// Update Claude Code files (.claude/commands/, CLAUDE.md)
        #[arg(long, group = "editor")]
        claude: bool,

        /// Update Windsurf files (.windsurf/rules/, AGENTS.md)
        #[arg(long, group = "editor")]
        windsurf: bool,
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

    /// Launch the terminal user interface
    Tui {
        /// Optional database path (default: auto-discover)
        #[arg(short, long)]
        db: Option<PathBuf>,
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

fn main() {
    let args = Args::parse();

    // Handle init separately - it doesn't need an existing database
    if let Command::Init { claude: _, windsurf } = args.command {
        // Determine editor type: default to Claude if neither specified
        let editor = if windsurf {
            deciduous::init::Editor::Windsurf
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
    if let Command::Update { claude: _, windsurf } = args.command {
        // Determine editor type: default to Claude if neither specified
        let editor = if windsurf {
            deciduous::init::Editor::Windsurf
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

    let db = match Database::open() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("{} Failed to open database: {}", "Error:".red(), e);
            std::process::exit(1);
        }
    };

    match args.command {
        Command::Init { .. } => unreachable!(), // Handled above
        Command::Update { .. } => unreachable!(), // Handled above
        Command::Add { node_type, title, description, confidence, commit, prompt, files, branch, no_branch } => {
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

            match db.create_node_full(&node_type, &title, description.as_deref(), confidence, effective_commit.as_deref(), prompt.as_deref(), files.as_deref(), effective_branch.as_deref()) {
                Ok(id) => {
                    let conf_str = confidence.map(|c| format!(" [confidence: {}%]", c)).unwrap_or_default();
                    let commit_str = effective_commit.as_ref().map(|c| format!(" [commit: {}]", &c[..7.min(c.len())])).unwrap_or_default();
                    let prompt_str = if prompt.is_some() { " [prompt saved]" } else { "" };
                    let files_str = files.as_ref().map(|f| format!(" [files: {}]", f)).unwrap_or_default();
                    let branch_str = effective_branch.as_ref().map(|b| format!(" [branch: {}]", b)).unwrap_or_default();
                    println!("{} node {} (type: {}, title: {}){}{}{}{}{}",
                        "Created".green(), id, node_type, title, conf_str, commit_str, prompt_str, files_str, branch_str);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        Command::Link { from, to, rationale, edge_type } => {
            match db.create_edge(from, to, &edge_type, rationale.as_deref()) {
                Ok(id) => {
                    println!("{} edge {} ({} -> {} via {})", "Created".green(), id, from, to, edge_type);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        Command::Status { id, status } => {
            match db.update_node_status(id, &status) {
                Ok(()) => println!("{} node {} status to '{}'", "Updated".green(), id, status),
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        Command::Nodes { branch, node_type } => {
            match db.get_all_nodes() {
                Ok(nodes) => {
                    // Filter nodes by branch and/or type
                    let filtered: Vec<_> = nodes.into_iter().filter(|n| {
                        // Filter by branch if specified
                        let branch_match = match &branch {
                            Some(b) => {
                                n.metadata_json.as_ref().is_some_and(|meta| {
                                    serde_json::from_str::<serde_json::Value>(meta)
                                        .ok()
                                        .and_then(|v| v.get("branch").and_then(|br| br.as_str()).map(|s| s.to_string()))
                                        .is_some_and(|node_branch| node_branch == *b)
                                })
                            }
                            None => true,
                        };
                        // Filter by type if specified
                        let type_match = match &node_type {
                            Some(t) => n.node_type == *t,
                            None => true,
                        };
                        branch_match && type_match
                    }).collect();

                    if filtered.is_empty() {
                        if branch.is_some() || node_type.is_some() {
                            println!("No nodes found matching filters.");
                        } else {
                            println!("No nodes found. Add one with: deciduous add goal \"My goal\"");
                        }
                    } else {
                        let header = match &branch {
                            Some(b) => format!("Nodes on branch '{}' ({} total):", b, filtered.len()),
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
                            println!("{:<5} {:<12} {:<10} {}", n.id, type_colored, n.status, n.title);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        Command::Edges => {
            match db.get_all_edges() {
                Ok(edges) => {
                    if edges.is_empty() {
                        println!("No edges found. Link nodes with: deciduous link 1 2 -r \"reason\"");
                    } else {
                        println!("{:<5} {:<6} {:<6} {:<12} RATIONALE", "ID", "FROM", "TO", "TYPE");
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
            }
        }

        Command::Graph => {
            match db.get_graph() {
                Ok(graph) => {
                    match serde_json::to_string_pretty(&graph) {
                        Ok(json) => println!("{}", json),
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

        Command::Serve { port } => {
            println!("{} Starting graph viewer at http://localhost:{}", "Deciduous".cyan(), port);
            if let Err(e) = deciduous::serve::start_graph_server(port) {
                eprintln!("{} Server error: {}", "Error:".red(), e);
                std::process::exit(1);
            }
        }

        Command::Sync { output } => {
            // Default to docs/ for GitHub Pages compatibility
            let output_path = output.unwrap_or_else(|| {
                PathBuf::from("docs/graph-data.json")
            });

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
                                    println!("{} graph to {}", "Exported".green(), output_path.display());
                                    println!("  {} nodes, {} edges", graph.nodes.len(), graph.edges.len());

                                    // Also sync to docs/demo/ if it exists (for GitHub Pages demo)
                                    let demo_path = PathBuf::from("docs/demo/graph-data.json");
                                    if demo_path.parent().map(|p| p.exists()).unwrap_or(false) {
                                        if let Err(e) = std::fs::write(&demo_path, &json) {
                                            eprintln!("{} Also writing to demo/: {}", "Warning:".yellow(), e);
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
                eprintln!("{} No database found at {}", "Error:".red(), db_path.display());
                std::process::exit(1);
            }

            let backup_path = output.unwrap_or_else(|| {
                let timestamp = Local::now().format("%Y%m%d_%H%M%S");
                PathBuf::from(format!("deciduous_backup_{}.db", timestamp))
            });

            match std::fs::copy(&db_path, &backup_path) {
                Ok(bytes) => {
                    println!("{} backup: {} ({} bytes)", "Created".green(), backup_path.display(), bytes);
                }
                Err(e) => {
                    eprintln!("{} Creating backup: {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        Command::Commands { limit } => {
            match db.get_recent_commands(limit) {
                Ok(commands) => {
                    if commands.is_empty() {
                        println!("No commands logged.");
                    } else {
                        for c in commands {
                            println!(
                                "[{}] {} (exit: {})",
                                c.started_at,
                                truncate(&c.command, 60),
                                c.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "running".to_string())
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

        Command::Dot { output, roots, nodes, png, auto, title, rankdir } => {
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

                        Some(PathBuf::from(format!("docs/decision-graph-{}.dot", safe_branch)))
                    } else {
                        output.clone()
                    };

                    if png || auto {
                        // Generate PNG using graphviz
                        let dot_path = effective_output.clone().unwrap_or_else(|| PathBuf::from("graph.dot"));
                        let png_path = dot_path.with_extension("png");

                        // Write DOT file
                        if let Err(e) = std::fs::write(&dot_path, &dot) {
                            eprintln!("{} Writing DOT file: {}", "Error:".red(), e);
                            std::process::exit(1);
                        }

                        // Run graphviz
                        match ProcessCommand::new("dot")
                            .args(["-Tpng", &dot_path.to_string_lossy(), "-o", &png_path.to_string_lossy()])
                            .output()
                        {
                            Ok(output) => {
                                if output.status.success() {
                                    println!("{} DOT: {}", "Exported".green(), dot_path.display());
                                    println!("{} PNG: {}", "Generated".green(), png_path.display());
                                } else {
                                    eprintln!("{} graphviz failed: {}", "Error:".red(),
                                        String::from_utf8_lossy(&output.stderr));
                                    eprintln!("Make sure graphviz is installed: brew install graphviz");
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
                        println!("  {} nodes, {} edges", filtered_graph.nodes.len(), filtered_graph.edges.len());
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

        Command::Writeup { title, roots, nodes, output, png, auto, no_dot, no_test_plan } => {
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

        Command::Migrate => {
            match db.migrate_add_change_ids() {
                Ok(true) => {
                    println!("{} Database migrated - added change_id columns for multi-user sync", "Success:".green());
                }
                Ok(false) => {
                    println!("{} Database already has change_id columns - no migration needed", "Info:".cyan());
                }
                Err(e) => {
                    eprintln!("{} Migration failed: {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        Command::Diff { action } => {
            match action {
                DiffAction::Export { output, nodes, branch, author, base_commit } => {
                    // Parse node IDs if provided
                    let node_ids = nodes.as_ref().map(|n| parse_node_range(n));

                    match db.export_patch(node_ids, branch.as_deref(), author, base_commit) {
                        Ok(patch) => {
                            match patch.save(&output) {
                                Ok(()) => {
                                    println!("{} Exported {} nodes and {} edges to {}",
                                        "Success:".green(),
                                        patch.nodes.len(),
                                        patch.edges.len(),
                                        output.display());
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

                DiffAction::Apply { files, dry_run } => {
                    let mut total_added = 0;
                    let mut total_skipped = 0;
                    let mut total_edges_added = 0;
                    let mut total_edges_skipped = 0;

                    for file in files {
                        match deciduous::GraphPatch::load(&file) {
                            Ok(patch) => {
                                match db.apply_patch(&patch, dry_run) {
                                    Ok(result) => {
                                        if dry_run {
                                            println!("{} {} (dry run)", "Would apply:".cyan(), file.display());
                                        } else {
                                            println!("{} {}", "Applied:".green(), file.display());
                                        }
                                        println!("  Nodes: {} added, {} skipped", result.nodes_added, result.nodes_skipped);
                                        println!("  Edges: {} added, {} skipped", result.edges_added, result.edges_skipped);
                                        if !result.edges_failed.is_empty() {
                                            println!("  {} edges failed (missing nodes):", result.edges_failed.len());
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
                                        eprintln!("{} Applying {}: {}", "Error:".red(), file.display(), e);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("{} Loading {}: {}", "Error:".red(), file.display(), e);
                            }
                        }
                    }

                    if !dry_run {
                        println!("\n{} {} nodes added, {} skipped; {} edges added, {} skipped",
                            "Total:".cyan(),
                            total_added, total_skipped,
                            total_edges_added, total_edges_skipped);
                    }
                }

                DiffAction::Status { path } => {
                    let patches_dir = path.unwrap_or_else(|| PathBuf::from(".deciduous/patches"));
                    if !patches_dir.exists() {
                        println!("{} No patches directory found at {}", "Info:".cyan(), patches_dir.display());
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
                                println!("  {} - {} nodes, {} edges (author: {}, branch: {})",
                                    path.file_name().unwrap_or_default().to_string_lossy(),
                                    patch.nodes.len(),
                                    patch.edges.len(),
                                    author,
                                    branch);
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
                                let node_ids: HashSet<&str> = patch.nodes.iter()
                                    .map(|n| n.change_id.as_str())
                                    .collect();

                                // Check each edge for missing nodes
                                let mut missing_edges = Vec::new();
                                for edge in &patch.edges {
                                    let from_missing = !node_ids.contains(edge.from_change_id.as_str());
                                    let to_missing = !node_ids.contains(edge.to_change_id.as_str());

                                    if from_missing || to_missing {
                                        let mut missing = Vec::new();
                                        if from_missing {
                                            missing.push(format!("from: {}", &edge.from_change_id[..8.min(edge.from_change_id.len())]));
                                        }
                                        if to_missing {
                                            missing.push(format!("to: {}", &edge.to_change_id[..8.min(edge.to_change_id.len())]));
                                        }
                                        missing_edges.push((edge.edge_type.clone(), missing.join(", ")));
                                    }
                                }

                                println!("{} {}", "Validating:".cyan(), file.display());
                                println!("  Nodes: {}", patch.nodes.len());
                                println!("  Edges: {} ({} valid, {} with missing refs)",
                                    patch.edges.len(),
                                    patch.edges.len() - missing_edges.len(),
                                    missing_edges.len());

                                if !missing_edges.is_empty() {
                                    any_errors = true;
                                    println!("  {} Edges referencing missing nodes:", "Warning:".yellow());
                                    for (edge_type, missing) in &missing_edges {
                                        println!("    - {} edge: missing {}", edge_type, missing);
                                    }
                                    println!();
                                    println!("  {} This patch has edges that reference nodes not in the patch.", "Note:".cyan());
                                    println!("  When applied, these edges will fail unless the referenced nodes");
                                    println!("  already exist in the target database or are imported first.");
                                    println!();
                                    println!("  {} Re-export with all dependent nodes, or apply patches in order:", "Fix:".green());
                                    println!("    1. Apply the patch containing the parent nodes first");
                                    println!("    2. Then apply this patch");
                                } else {
                                    println!("  {} All edges reference nodes within the patch", "OK:".green());
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
