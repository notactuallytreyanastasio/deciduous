//! Report generation for analysis results
//!
//! This module provides output formatters for analysis results in multiple formats:
//!
//! - **HTML**: Interactive report with D3.js visualizations (spectral waterfall, charts)
//! - **JSON**: Machine-readable format for programmatic consumption
//! - **CSV**: Spreadsheet-compatible format for bulk analysis
//!
//! # Usage
//!
//! ```ignore
//! use losselot::report;
//!
//! // Automatically picks format based on extension
//! report::generate("report.html", &results)?;  // HTML
//! report::generate("report.json", &results)?;  // JSON
//! report::generate("report.csv", &results)?;   // CSV
//! ```

pub mod csv;
pub mod html;
pub mod json;

use crate::analyzer::AnalysisResult;
use std::io;
use std::path::Path;

/// Generate a report in the appropriate format based on file extension
pub fn generate<P: AsRef<Path>>(path: P, results: &[AnalysisResult]) -> io::Result<()> {
    let path = path.as_ref();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mut file = std::fs::File::create(path)?;

    match ext.as_str() {
        "html" | "htm" => html::write(&mut file, results),
        "json" => json::write(&mut file, results),
        _ => csv::write(&mut file, results),
    }
}

/// Summary statistics for a batch of results
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Summary {
    pub total: usize,
    pub ok: usize,
    pub suspect: usize,
    pub transcode: usize,
    pub error: usize,
}

impl Summary {
    pub fn from_results(results: &[AnalysisResult]) -> Self {
        let mut summary = Self::default();
        summary.total = results.len();

        for r in results {
            match r.verdict {
                crate::analyzer::Verdict::Ok => summary.ok += 1,
                crate::analyzer::Verdict::Suspect => summary.suspect += 1,
                crate::analyzer::Verdict::Transcode => summary.transcode += 1,
                crate::analyzer::Verdict::Error => summary.error += 1,
            }
        }

        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::Verdict;

    // ==========================================================================
    // SUMMARY STATISTICS TESTS
    // ==========================================================================
    //
    // The Summary struct aggregates verdict counts for a batch of files.
    // This is displayed at the top of reports to give an overview.
    // ==========================================================================

    fn create_test_result(verdict: Verdict) -> AnalysisResult {
        AnalysisResult {
            file_path: "/test/file.mp3".to_string(),
            file_name: "file.mp3".to_string(),
            bitrate: 320,
            sample_rate: 44100,
            duration_secs: 180.0,
            verdict,
            combined_score: 0,
            spectral_score: 0,
            binary_score: 0,
            flags: vec![],
            encoder: "LAME".to_string(),
            lowpass: None,
            spectral_details: None,
            binary_details: None,
            error: None,
        }
    }

    #[test]
    fn test_summary_empty() {
        let results: Vec<AnalysisResult> = vec![];
        let summary = Summary::from_results(&results);

        assert_eq!(summary.total, 0);
        assert_eq!(summary.ok, 0);
        assert_eq!(summary.suspect, 0);
        assert_eq!(summary.transcode, 0);
        assert_eq!(summary.error, 0);
    }

    #[test]
    fn test_summary_all_ok() {
        let results = vec![
            create_test_result(Verdict::Ok),
            create_test_result(Verdict::Ok),
            create_test_result(Verdict::Ok),
        ];
        let summary = Summary::from_results(&results);

        assert_eq!(summary.total, 3);
        assert_eq!(summary.ok, 3);
        assert_eq!(summary.suspect, 0);
        assert_eq!(summary.transcode, 0);
    }

    #[test]
    fn test_summary_mixed() {
        let results = vec![
            create_test_result(Verdict::Ok),
            create_test_result(Verdict::Ok),
            create_test_result(Verdict::Suspect),
            create_test_result(Verdict::Transcode),
            create_test_result(Verdict::Error),
        ];
        let summary = Summary::from_results(&results);

        assert_eq!(summary.total, 5);
        assert_eq!(summary.ok, 2);
        assert_eq!(summary.suspect, 1);
        assert_eq!(summary.transcode, 1);
        assert_eq!(summary.error, 1);
    }

    #[test]
    fn test_summary_all_transcodes() {
        // Worst case: entire library is fake
        let results = vec![
            create_test_result(Verdict::Transcode),
            create_test_result(Verdict::Transcode),
        ];
        let summary = Summary::from_results(&results);

        assert_eq!(summary.total, 2);
        assert_eq!(summary.ok, 0);
        assert_eq!(summary.transcode, 2);
    }

    #[test]
    fn test_summary_default() {
        let summary = Summary::default();

        assert_eq!(summary.total, 0);
        assert_eq!(summary.ok, 0);
        assert_eq!(summary.suspect, 0);
        assert_eq!(summary.transcode, 0);
        assert_eq!(summary.error, 0);
    }

    // ==========================================================================
    // COLLECTION HEATMAP TESTS
    // ==========================================================================
    //
    // The collection heatmap groups files by folder and displays quality
    // distribution as stacked bar charts. These tests verify the HTML output
    // contains the necessary elements for the visualization.
    // ==========================================================================

    fn create_test_result_with_path(verdict: Verdict, path: &str, name: &str) -> AnalysisResult {
        AnalysisResult {
            file_path: path.to_string(),
            file_name: name.to_string(),
            bitrate: 320,
            sample_rate: 44100,
            duration_secs: 180.0,
            verdict,
            combined_score: match verdict {
                Verdict::Ok => 10,
                Verdict::Suspect => 50,
                Verdict::Transcode => 80,
                Verdict::Error => 0,
            },
            spectral_score: 0,
            binary_score: 0,
            flags: vec![],
            encoder: "LAME".to_string(),
            lowpass: None,
            spectral_details: None,
            binary_details: None,
            error: None,
        }
    }

    #[test]
    fn test_html_contains_collection_heatmap_container() {
        // Verify the HTML template includes the collection heatmap div
        let html = include_str!("html.rs");
        assert!(html.contains("collection-heatmap"), "HTML should contain collection-heatmap container");
    }

    #[test]
    fn test_html_contains_heatmap_function_definition() {
        // Verify the drawCollectionHeatmap function is defined
        let html = include_str!("html.rs");
        assert!(html.contains("function drawCollectionHeatmap()"), "HTML should define drawCollectionHeatmap function");
    }

    #[test]
    fn test_html_contains_heatmap_function_call() {
        // Verify drawCollectionHeatmap is called during initialization
        let html = include_str!("html.rs");
        assert!(html.contains("drawCollectionHeatmap();"), "HTML should call drawCollectionHeatmap during init");
    }

    #[test]
    fn test_heatmap_groups_files_by_folder_logic() {
        // Test the grouping logic would work with multiple folders
        // This tests the data structure, not the JS rendering
        let results = vec![
            create_test_result_with_path(Verdict::Ok, "/music/album1/track1.flac", "track1.flac"),
            create_test_result_with_path(Verdict::Ok, "/music/album1/track2.flac", "track2.flac"),
            create_test_result_with_path(Verdict::Suspect, "/music/album2/track1.flac", "track1.flac"),
            create_test_result_with_path(Verdict::Transcode, "/music/album2/track2.flac", "track2.flac"),
        ];

        // Group by folder path (simulating JS logic)
        use std::collections::HashMap;
        let mut folders: HashMap<String, (u32, u32, u32)> = HashMap::new();

        for r in &results {
            let path = &r.file_path;
            let last_slash = path.rfind('/').unwrap_or(0);
            let folder = if last_slash > 0 { &path[..last_slash] } else { "(root)" };

            let entry = folders.entry(folder.to_string()).or_insert((0, 0, 0));
            match r.verdict {
                Verdict::Ok => entry.0 += 1,
                Verdict::Suspect => entry.1 += 1,
                Verdict::Transcode => entry.2 += 1,
                _ => {}
            }
        }

        assert_eq!(folders.len(), 2, "Should have 2 folders");
        assert_eq!(folders.get("/music/album1"), Some(&(2, 0, 0)), "album1 should have 2 ok files");
        assert_eq!(folders.get("/music/album2"), Some(&(0, 1, 1)), "album2 should have 1 suspect, 1 transcode");
    }

    #[test]
    fn test_heatmap_health_score_calculation() {
        // Test health score calculation (% of clean files)
        let total = 10;
        let ok = 7;
        let health_score = (ok as f64 / total as f64) * 100.0;
        assert!((health_score - 70.0).abs() < 0.01, "Health score should be 70%");

        // Edge case: all clean
        let health_all_ok = (10.0 / 10.0) * 100.0;
        assert_eq!(health_all_ok, 100.0);

        // Edge case: no clean files
        let health_none_ok = (0.0 / 10.0) * 100.0;
        assert_eq!(health_none_ok, 0.0);
    }
}
