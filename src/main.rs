use chrono::Local;
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use losselot::{AnalysisResult, Analyzer, Verdict};
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

    /// Output report file (.html, .csv, .json)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Directory for auto-generated reports
    #[arg(long, default_value = "losselot-reports")]
    report_dir: PathBuf,

    /// Don't auto-generate HTML report
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
        let filename = format!("losselot_report_{}.html", timestamp);
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
