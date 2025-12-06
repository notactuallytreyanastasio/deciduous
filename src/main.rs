use chrono::Local;
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use losselot::{AnalysisResult, Analyzer, Database, Verdict};
use rayon::prelude::*;
use std::io::{self, Write};
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(name = "losselot")]
#[command(author, version, about = "Detect 'lossless' files that were created from lossy sources")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// File or directory to analyze (optional in GUI mode)
    path: Option<PathBuf>,

    /// Launch GUI file picker (auto-enabled when double-clicked)
    #[arg(long)]
    gui: bool,

    /// Output report file (.csv, .json)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Directory for auto-generated reports
    #[arg(long, default_value = "losselot-reports")]
    report_dir: PathBuf,

    /// Don't auto-generate CSV report
    #[arg(long)]
    no_report: bool,

    /// Don't prompt to open report
    #[arg(long)]
    no_open: bool,

    /// Number of parallel workers (default: number of CPUs)
    #[arg(short, long)]
    jobs: Option<usize>,

    /// Skip spectral analysis (faster, binary-only)
    #[arg(long)]
    no_spectral: bool,

    /// Show detailed analysis
    #[arg(short, long)]
    verbose: bool,

    /// Only show summary
    #[arg(short, long)]
    quiet: bool,

    /// Transcode threshold percentage (default: 65)
    #[arg(long, default_value = "65")]
    threshold: u32,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Start interactive web UI for analysis
    Serve {
        /// File or directory to analyze
        path: PathBuf,

        /// Port to listen on
        #[arg(short, long, default_value = "3001")]
        port: u16,
    },

    /// Database operations for decision graph
    Db {
        #[command(subcommand)]
        action: DbAction,
    },
}

#[derive(Subcommand, Debug)]
enum DbAction {
    /// List all decision nodes
    Nodes,

    /// List all edges
    Edges,

    /// Show full graph as JSON
    Graph,

    /// Add a new decision node
    AddNode {
        /// Node type: goal, decision, option, action, outcome, observation
        #[arg(short = 't', long)]
        node_type: String,

        /// Title of the node
        title: String,

        /// Optional description
        #[arg(short, long)]
        description: Option<String>,

        /// Confidence level (0-100) - how sure are we about this decision?
        #[arg(short, long)]
        confidence: Option<u8>,

        /// Git commit hash to link this node to
        #[arg(long)]
        commit: Option<String>,
    },

    /// Add an edge between nodes
    AddEdge {
        /// Source node ID
        from: i32,

        /// Target node ID
        to: i32,

        /// Edge type: leads_to, requires, chosen, rejected, blocks, enables
        #[arg(short = 't', long, default_value = "leads_to")]
        edge_type: String,

        /// Rationale for this edge
        #[arg(short, long)]
        rationale: Option<String>,
    },

    /// Update node status
    Status {
        /// Node ID
        id: i32,

        /// New status: pending, active, completed, rejected
        status: String,
    },

    /// Show recent commands
    Commands {
        /// Number of commands to show
        #[arg(short, long, default_value = "20")]
        limit: i64,
    },

    /// Create a backup of the database
    Backup {
        /// Output path for backup (default: losselot_backup_<timestamp>.db)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() {
    let args = Args::parse();

    // Handle subcommands first
    if let Some(cmd) = args.command {
        match cmd {
            Command::Serve { path, port } => {
                if let Err(e) = losselot::serve::start(port, path) {
                    eprintln!("Server error: {}", e);
                    std::process::exit(1);
                }
                return;
            }
            Command::Db { action } => {
                handle_db_action(action);
                return;
            }
        }
    }

    // Determine if we should use GUI mode
    // With GUI feature: launch GUI if --gui flag OR no path provided
    // This makes double-click behavior "just work"
    #[cfg(feature = "gui")]
    let use_gui = args.gui || args.path.is_none();

    #[cfg(not(feature = "gui"))]
    let use_gui = false;

    // Handle GUI mode
    #[cfg(feature = "gui")]
    let path = if use_gui {
        match pick_path_gui() {
            Some(p) => p,
            None => {
                // User cancelled - show message and exit
                eprintln!("No file or folder selected.");
                std::process::exit(0);
            }
        }
    } else {
        // Path was provided via CLI
        args.path.clone().unwrap()
    };

    #[cfg(not(feature = "gui"))]
    let path = if let Some(p) = args.path.clone() {
        p
    } else {
        eprintln!("Usage: losselot <PATH>");
        eprintln!("Run 'losselot --help' for more options.");
        eprintln!("Note: GUI mode not available in this build.");
        std::process::exit(1);
    };

    // Set up thread pool
    if let Some(jobs) = args.jobs {
        rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build_global()
            .ok();
    }

    // Supported audio formats
    let supported_extensions: std::collections::HashSet<&str> = [
        "flac", "wav", "wave", "aiff", "aif", "mp3", "m4a", "aac", "ogg", "opus", "wma", "alac"
    ].iter().cloned().collect();

    // Collect audio files
    let files: Vec<PathBuf> = if path.is_dir() {
        WalkDir::new(&path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| supported_extensions.contains(ext.to_ascii_lowercase().as_str()))
                    .unwrap_or(false)
            })
            .map(|e| e.path().to_path_buf())
            .collect()
    } else {
        vec![path.clone()]
    };

    if files.is_empty() {
        eprintln!("No audio files found (supported: flac, wav, mp3, m4a, ogg, opus, aiff)");
        std::process::exit(1);
    }

    if !args.quiet {
        eprintln!("\x1b[1mLosselot - Lossy Source Detector\x1b[0m");
        eprintln!("{}", "─".repeat(70));
        eprintln!("Found {} audio file(s)\n", files.len());
    }

    // Set up progress bar
    let pb = if !args.quiet && files.len() > 1 {
        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=>-"),
        );
        Some(pb)
    } else {
        None
    };

    // Create analyzer
    let analyzer = Analyzer::new()
        .with_skip_spectral(args.no_spectral)
        .with_thresholds(35, args.threshold);

    // Analyze files in parallel
    let results: Vec<AnalysisResult> = files
        .par_iter()
        .map(|path| {
            let result = analyzer.analyze(path);
            if let Some(ref pb) = pb {
                pb.inc(1);
                pb.set_message(format!("{}", result.file_name));
            }
            result
        })
        .collect();

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    // Print results
    if !args.quiet {
        for r in &results {
            let color = match r.verdict {
                Verdict::Ok => "\x1b[32m",        // Green
                Verdict::Suspect => "\x1b[33m",  // Yellow
                Verdict::Transcode => "\x1b[31m", // Red
                Verdict::Error => "\x1b[90m",    // Gray
            };
            let reset = "\x1b[0m";

            let flags_str = if r.flags.is_empty() {
                "-".to_string()
            } else {
                r.flags.join(",")
            };

            println!(
                "{}{:<10}{} {:>3}%  {:>4}kbps  {:<12}  {:<30}  {}",
                color,
                format!("[{}]", r.verdict),
                reset,
                r.combined_score,
                r.bitrate,
                &r.encoder,
                truncate(&flags_str, 30),
                &r.file_name
            );

            if args.verbose {
                if let Some(ref details) = r.spectral_details {
                    eprintln!(
                        "    Spectral: full={:.1}dB high={:.1}dB upper={:.1}dB ultrasonic={:.1}dB",
                        details.rms_full,
                        details.rms_high,
                        details.rms_upper,
                        details.rms_ultrasonic
                    );
                    eprintln!(
                        "    Drops: upper={:.1}dB ultrasonic={:.1}dB | flatness_19-21k={:.3}",
                        details.upper_drop,
                        details.ultrasonic_drop,
                        details.ultrasonic_flatness
                    );
                }
                if let Some(ref details) = r.binary_details {
                    eprintln!(
                        "    Binary: lowpass={} encoder_count={} frame_cv={:.1}%",
                        details.lowpass.map(|l| format!("{}Hz", l)).unwrap_or_else(|| "n/a".to_string()),
                        details.encoder_count,
                        details.frame_size_cv
                    );
                }
            }
        }
    }

    // Summary
    let ok_count = results.iter().filter(|r| r.verdict == Verdict::Ok).count();
    let suspect_count = results.iter().filter(|r| r.verdict == Verdict::Suspect).count();
    let transcode_count = results.iter().filter(|r| r.verdict == Verdict::Transcode).count();
    let error_count = results.iter().filter(|r| r.verdict == Verdict::Error).count();

    if !args.quiet {
        eprintln!("\n{}", "─".repeat(70));
        eprintln!("\x1b[1mSummary:\x1b[0m");
        eprintln!("  \x1b[32m✓ Clean:\x1b[0m     {}", ok_count);
        eprintln!("  \x1b[33m? Suspect:\x1b[0m   {}", suspect_count);
        eprintln!("  \x1b[31m✗ Transcode:\x1b[0m {}", transcode_count);
        if error_count > 0 {
            eprintln!("  \x1b[90mErrors:\x1b[0m      {}", error_count);
        }
    }

    // Determine report path
    let report_path = if let Some(ref output) = args.output {
        Some(output.clone())
    } else if !args.no_report {
        // Auto-generate report
        std::fs::create_dir_all(&args.report_dir).ok();
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("losselot_report_{}.csv", timestamp);
        Some(args.report_dir.join(filename))
    } else {
        None
    };

    // Generate report
    if let Some(ref output_path) = report_path {
        if let Err(e) = losselot::report::generate(output_path, &results) {
            eprintln!("Failed to write report: {}", e);
            std::process::exit(1);
        }
        if !args.quiet {
            eprintln!("\n\x1b[32mReport saved: {}\x1b[0m", output_path.display());
        }

        // Open report
        if !args.no_open {
            if use_gui {
                // In GUI mode, auto-open the report (no prompt)
                let _ = open::that(output_path);
            } else if !args.quiet {
                // In terminal mode, ask first
                eprint!("\nOpen report in browser? [Y/n] ");
                io::stderr().flush().ok();

                let mut input = String::new();
                if io::stdin().read_line(&mut input).is_ok() {
                    let input = input.trim().to_lowercase();
                    if input.is_empty() || input == "y" || input == "yes" {
                        if let Err(e) = open::that(output_path) {
                            eprintln!("Failed to open report: {}", e);
                        }
                    }
                }
            }
        }
    }

    if !args.quiet {
        eprintln!("\n\x1b[90mAnalysis complete.\x1b[0m");
    }

    // Exit with appropriate code
    if transcode_count > 0 {
        std::process::exit(2);
    } else if suspect_count > 0 {
        std::process::exit(1);
    }
}

#[cfg(feature = "gui")]
fn pick_path_gui() -> Option<PathBuf> {
    // First try folder picker
    if let Some(folder) = rfd::FileDialog::new()
        .set_title("Select folder to analyze (or Cancel for single file)")
        .pick_folder()
    {
        return Some(folder);
    }

    // If cancelled, offer file picker
    rfd::FileDialog::new()
        .set_title("Select audio file to analyze")
        .add_filter("Audio files", &["flac", "wav", "mp3", "m4a", "aac", "ogg", "opus", "aiff"])
        .pick_file()
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn handle_db_action(action: DbAction) {
    let db = match Database::open() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Failed to open database: {}", e);
            std::process::exit(1);
        }
    };

    match action {
        DbAction::Nodes => {
            match db.get_all_nodes() {
                Ok(nodes) => {
                    if nodes.is_empty() {
                        println!("No nodes found.");
                    } else {
                        println!("{:<5} {:<12} {:<10} {}", "ID", "TYPE", "STATUS", "TITLE");
                        println!("{}", "-".repeat(60));
                        for n in nodes {
                            println!("{:<5} {:<12} {:<10} {}", n.id, n.node_type, n.status, n.title);
                        }
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }

        DbAction::Edges => {
            match db.get_all_edges() {
                Ok(edges) => {
                    if edges.is_empty() {
                        println!("No edges found.");
                    } else {
                        println!("{:<5} {:<6} {:<6} {:<12} {}", "ID", "FROM", "TO", "TYPE", "RATIONALE");
                        println!("{}", "-".repeat(60));
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
                Err(e) => eprintln!("Error: {}", e),
            }
        }

        DbAction::Graph => {
            match db.get_graph() {
                Ok(graph) => {
                    match serde_json::to_string_pretty(&graph) {
                        Ok(json) => println!("{}", json),
                        Err(e) => eprintln!("Error serializing graph: {}", e),
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }

        DbAction::AddNode { node_type, title, description, confidence, commit } => {
            match db.create_node(&node_type, &title, description.as_deref(), confidence, commit.as_deref()) {
                Ok(id) => {
                    let conf_str = confidence.map(|c| format!(" [confidence: {}%]", c)).unwrap_or_default();
                    let commit_str = commit.as_ref().map(|c| format!(" [commit: {}]", &c[..7.min(c.len())])).unwrap_or_default();
                    println!("Created node {} (type: {}, title: {}){}{}", id, node_type, title, conf_str, commit_str);
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }

        DbAction::AddEdge { from, to, edge_type, rationale } => {
            match db.create_edge(from, to, &edge_type, rationale.as_deref()) {
                Ok(id) => println!("Created edge {} ({} -> {} via {})", id, from, to, edge_type),
                Err(e) => eprintln!("Error: {}", e),
            }
        }

        DbAction::Status { id, status } => {
            match db.update_node_status(id, &status) {
                Ok(()) => println!("Updated node {} status to '{}'", id, status),
                Err(e) => eprintln!("Error: {}", e),
            }
        }

        DbAction::Commands { limit } => {
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
                Err(e) => eprintln!("Error: {}", e),
            }
        }

        DbAction::Backup { output } => {
            let db_path = Database::db_path();
            if !db_path.exists() {
                eprintln!("No database found at {}", db_path.display());
                return;
            }

            let backup_path = output.unwrap_or_else(|| {
                let timestamp = Local::now().format("%Y%m%d_%H%M%S");
                PathBuf::from(format!("losselot_backup_{}.db", timestamp))
            });

            match std::fs::copy(&db_path, &backup_path) {
                Ok(bytes) => {
                    println!("Backup created: {} ({} bytes)", backup_path.display(), bytes);
                }
                Err(e) => {
                    eprintln!("Failed to create backup: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
