//! Losselot - Detect fake lossless audio files
//!
//! Losselot analyzes audio files to detect if they were transcoded from
//! lossy sources (MP3, AAC) and falsely labeled as lossless (FLAC, WAV).
//!
//! # Overview
//!
//! When audio is encoded to a lossy format like MP3, high frequencies are
//! permanently removed to save space. Converting an MP3 back to FLAC doesn't
//! restore these frequencies - it just makes a bigger file that still sounds
//! like an MP3. Losselot detects this by analyzing the frequency content.
//!
//! # Detection Methods
//!
//! 1. **Binary Analysis** (MP3 only): Reads LAME encoder headers which honestly
//!    record the lowpass filter frequency. A "320kbps" file with 16kHz lowpass
//!    was transcoded from a 128kbps source.
//!
//! 2. **Spectral Analysis** (all formats): Uses FFT to measure energy in
//!    frequency bands. Transcoded files show a characteristic "cliff" where
//!    high frequencies suddenly drop to silence.
//!
//! # Quick Start
//!
//! ```no_run
//! use losselot::{Analyzer, Verdict};
//!
//! let analyzer = Analyzer::new();
//! let result = analyzer.analyze("suspicious.flac");
//!
//! match result.verdict {
//!     Verdict::Ok => println!("Looks legitimate"),
//!     Verdict::Suspect => println!("Something's off - investigate"),
//!     Verdict::Transcode => println!("Definitely fake!"),
//!     Verdict::Error => println!("Couldn't analyze: {:?}", result.error),
//! }
//!
//! println!("Score: {}/100", result.combined_score);
//! println!("Flags: {:?}", result.flags);
//! ```
//!
//! # Scoring System
//!
//! Files receive a score from 0-100 based on suspicious indicators:
//!
//! | Score Range | Verdict | Meaning |
//! |-------------|---------|---------|
//! | 0-34 | OK | Appears to be genuine lossless |
//! | 35-64 | SUSPECT | Some indicators of lossy origin |
//! | 65-100 | TRANSCODE | Strong evidence of fake file |
//!
//! # Modules
//!
//! - [`analyzer`]: Core analysis engine combining binary and spectral methods
//! - [`mp3`]: MP3 frame parsing and LAME header extraction
//! - [`report`]: Output formatters (HTML, JSON, CSV)

pub mod analyzer;
pub mod mp3;
pub mod report;
pub mod serve;

pub use analyzer::{AnalysisResult, Analyzer, Verdict};

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // PUBLIC API TESTS
    // ==========================================================================
    //
    // These tests verify the public API surface is correct and documented.
    // ==========================================================================

    #[test]
    fn test_public_exports() {
        // Verify core types are re-exported from crate root
        let _: Verdict = Verdict::Ok;
        let _analyzer = Analyzer::new();
        // AnalysisResult requires many fields, verified in analyzer tests
    }

    #[test]
    fn test_analyzer_accessible() {
        // Analyzer should be constructible from crate root
        let analyzer = Analyzer::new();
        assert!(!analyzer.skip_spectral);
    }

    #[test]
    fn test_verdict_variants() {
        // All verdict variants should be accessible
        let _ = Verdict::Ok;
        let _ = Verdict::Suspect;
        let _ = Verdict::Transcode;
        let _ = Verdict::Error;
    }
}
