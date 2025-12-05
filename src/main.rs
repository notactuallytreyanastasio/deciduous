use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use losselot::{AnalysisResult, Analyzer, Verdict};
use rayon::prelude::*;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(name = "losselot")]
#[command(author, version, about = "Detect 'lossless' files that were created from lossy sources")]
struct Args {
    /// File or directory to analyze
    #[arg(required = true)]
    path: PathBuf,

    /// Output report file (.html, .csv, .json)
    #[arg(short, long)]
    output: Option<PathBuf>,

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

fn main() {
    let args = Args::parse();

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
    let files: Vec<PathBuf> = if args.path.is_dir() {
        WalkDir::new(&args.path)
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
        vec![args.path.clone()]
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
                        "    Spectral: full={:.1}dB high={:.1}dB upper={:.1}dB | drops: high={:.1} upper={:.1}",
                        details.rms_full,
                        details.rms_high,
                        details.rms_upper,
                        details.high_drop,
                        details.upper_drop
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

    // Generate report
    if let Some(output_path) = args.output {
        if let Err(e) = losselot::report::generate(&output_path, &results) {
            eprintln!("Failed to write report: {}", e);
            std::process::exit(1);
        }
        if !args.quiet {
            eprintln!("\n\x1b[32mReport saved: {}\x1b[0m", output_path.display());
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

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
