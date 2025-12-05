//! Binary/structural analysis of MP3 files
//!
//! Analyzes the binary structure of MP3 files to detect transcoding:
//! - LAME header lowpass mismatch (smoking gun)
//! - Multiple encoder signatures
//! - Frame size irregularities
//! - ID3 tag inconsistencies
//!
//! # How Binary Analysis Works
//!
//! Unlike spectral analysis (which looks at the actual audio content), binary
//! analysis examines the metadata and structure embedded in the file itself.
//!
//! ## Key Detection Methods:
//!
//! 1. **Lowpass Mismatch**: The LAME encoder records what lowpass filter it used.
//!    If a "320kbps" file has lowpass=16kHz, it was transcoded from 128kbps.
//!
//! 2. **Multiple Encoder Signatures**: If a file has both "LAME" and "Lavf"
//!    (FFmpeg) signatures, it was likely re-encoded at some point.
//!
//! 3. **Frame Size Irregularities**: CBR files should have uniform frame sizes.
//!    High variance in a "CBR 320kbps" file suggests something is wrong.
//!
//! Binary analysis is fast (just reads headers) but only works on MP3 files
//! encoded with LAME. Other formats (AAC, Opus, FLAC) need spectral analysis.

use crate::mp3::{frame, lame};
use serde::Serialize;
use std::io::{Read, Seek};

/// Per-frame bitrate data for VBR timeline visualization
#[derive(Debug, Clone, Default, Serialize)]
pub struct BitrateTimeline {
    /// Time points in seconds for each frame
    pub times: Vec<f64>,
    /// Bitrate in kbps for each frame
    pub bitrates: Vec<u32>,
    /// Whether this is VBR (multiple different bitrates)
    pub is_vbr: bool,
    /// Minimum bitrate seen
    pub min_bitrate: u32,
    /// Maximum bitrate seen
    pub max_bitrate: u32,
    /// Average bitrate
    pub avg_bitrate: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BinaryDetails {
    pub lowpass: Option<u32>,
    pub expected_lowpass: Option<u32>,
    pub encoder_version: Option<String>,
    pub encoder_count: usize,
    pub frame_size_cv: f64,
    pub is_vbr: bool,
    pub total_frames: Option<u32>,
    /// Number of times LAME signature appears (>1 = re-encoded)
    pub lame_occurrences: usize,
    /// Number of times FFmpeg/Lavf signature appears
    pub ffmpeg_occurrences: usize,
    /// Human-readable encoding chain (e.g., "LAME → FFmpeg")
    pub encoding_chain: Option<String>,
    /// True if file shows evidence of re-encoding
    pub reencoded: bool,
    /// Per-frame bitrate timeline data for visualization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate_timeline: Option<BitrateTimeline>,
}

pub struct BinaryResult {
    pub score: u32,
    pub flags: Vec<String>,
    pub encoder: String,
    pub lowpass: Option<u32>,
    pub details: BinaryDetails,
}

impl Default for BinaryResult {
    fn default() -> Self {
        Self {
            score: 0,
            flags: vec![],
            encoder: "unknown".to_string(),
            lowpass: None,
            details: BinaryDetails::default(),
        }
    }
}

/// Perform binary analysis on MP3 data
pub fn analyze<R: Read + Seek>(data: &[u8], reader: &mut R, bitrate: u32) -> BinaryResult {
    let mut result = BinaryResult::default();

    // Extract LAME header
    if let Some(lame_header) = lame::LameHeader::extract(data) {
        result.encoder = if lame_header.encoder.is_empty() {
            "LAME".to_string()
        } else {
            lame_header.encoder.clone()
        };

        result.lowpass = lame_header.lowpass;
        result.details.lowpass = lame_header.lowpass;
        result.details.encoder_version = Some(lame_header.encoder);
        result.details.is_vbr = lame_header.is_vbr_header;
        result.details.total_frames = lame_header.total_frames;

        // KEY CHECK: Lowpass mismatch
        if let Some(actual_lowpass) = lame_header.lowpass {
            let (is_suspicious, expected, reason) =
                lame::check_lowpass_mismatch(bitrate, actual_lowpass);

            result.details.expected_lowpass = Some(expected);

            if is_suspicious {
                result.score += 35;
                result.flags.push(format!("lowpass_mismatch({}Hz)", actual_lowpass));

                if let Some(r) = reason {
                    // Log but don't add to flags (too verbose)
                    let _ = r;
                }
            }
        }
    } else {
        // Check for other encoders
        reader.seek(std::io::SeekFrom::Start(0)).ok();
        if let Ok(sigs) = lame::scan_encoder_signatures(reader) {
            if let Some(lame_ver) = sigs.lame {
                result.encoder = lame_ver;
            } else if sigs.fraunhofer {
                result.encoder = "Fraunhofer".to_string();
            } else if sigs.itunes {
                result.encoder = "iTunes".to_string();
            } else if sigs.ffmpeg {
                result.encoder = "FFmpeg".to_string();
            }
        }
    }

    // =========================================================================
    // RE-ENCODING DETECTION
    // =========================================================================
    // Scan for encoder signatures and count occurrences.
    // Multiple occurrences or mixed encoders indicate re-encoding.
    // =========================================================================
    reader.seek(std::io::SeekFrom::Start(0)).ok();
    if let Ok(sigs) = lame::scan_encoder_signatures(reader) {
        result.details.encoder_count = sigs.unique_encoder_count();
        result.details.lame_occurrences = sigs.lame_count;
        result.details.ffmpeg_occurrences = sigs.lavf_count;
        result.details.encoding_chain = sigs.encoding_chain_description();
        result.details.reencoded = sigs.shows_reencoding();

        // Score for re-encoding evidence
        if sigs.shows_reencoding() {
            // Multiple encoder signatures = file was processed multiple times
            if sigs.unique_encoder_count() > 1 {
                result.score += 20;
                result.flags.push("multi_encoder_sigs".to_string());
            }

            // Multiple LAME passes = encoded more than once with LAME
            if sigs.lame_count > 1 {
                result.score += 15;
                result.flags.push(format!("lame_reencoded_x{}", sigs.lame_count));
            }

            // Multiple FFmpeg passes = processed multiple times
            if sigs.lavf_count > 1 {
                result.score += 15;
                result.flags.push(format!("ffmpeg_processed_x{}", sigs.lavf_count));
            }

            // Multiple Fraunhofer passes
            if sigs.fraunhofer_count > 1 {
                result.score += 15;
                result.flags.push(format!("fraunhofer_reencoded_x{}", sigs.fraunhofer_count));
            }

            // Other encoders detected (GOGO, BladeEnc, Shine, Helix)
            for other in &sigs.other {
                result.flags.push(format!("encoder_{}", other.to_lowercase()));
            }

            // Encoding chain detected (LAME → FFmpeg etc)
            if let Some(ref chain) = result.details.encoding_chain {
                result.flags.push(format!("encoding_chain({})", chain));
            }
        }
    }

    // Frame size analysis and bitrate timeline
    reader.seek(std::io::SeekFrom::Start(0)).ok();
    if let Ok(frame_stats) = frame::scan_frames(reader, 500) {
        let cv = frame_stats.frame_size_cv();
        result.details.frame_size_cv = cv;

        // High variance in high-bitrate CBR is suspicious
        if bitrate >= 256 && cv > 15.0 {
            result.score += 10;
            result.flags.push("irregular_frames".to_string());
        }

        // Build bitrate timeline for visualization
        if !frame_stats.bitrates.is_empty() {
            // Calculate time for each frame
            // MPEG1 Layer3: 1152 samples/frame at 44100 Hz = ~26.1ms per frame
            const SAMPLES_PER_FRAME: f64 = 1152.0;
            const SAMPLE_RATE: f64 = 44100.0;
            let frame_duration = SAMPLES_PER_FRAME / SAMPLE_RATE;

            let times: Vec<f64> = (0..frame_stats.bitrates.len())
                .map(|i| i as f64 * frame_duration)
                .collect();

            result.details.bitrate_timeline = Some(BitrateTimeline {
                times,
                bitrates: frame_stats.bitrates.clone(),
                is_vbr: frame_stats.is_vbr,
                min_bitrate: frame_stats.min_bitrate,
                max_bitrate: frame_stats.max_bitrate,
                avg_bitrate: frame_stats.avg_bitrate,
            });
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // ==========================================================================
    // EDUCATIONAL BACKGROUND: Binary Analysis for Transcode Detection
    // ==========================================================================
    //
    // Binary analysis examines the FILE STRUCTURE, not the audio content.
    // This is complementary to spectral analysis:
    //
    //   - Binary Analysis: Reads metadata/headers. Fast, but only works if
    //     the encoder left behind forensic evidence (e.g., LAME headers).
    //
    //   - Spectral Analysis: Looks at actual frequency content. Works on any
    //     format, but slower and requires decoding the audio.
    //
    // When both methods agree, we have high confidence in the verdict.
    //
    // SCORING SYSTEM:
    // The binary analyzer adds points for suspicious indicators:
    //   +35 points: Lowpass mismatch (the smoking gun)
    //   +20 points: Multiple encoder signatures (re-encoding evidence)
    //   +10 points: Irregular frame sizes in supposed CBR file
    //
    // A score of 0 means no binary evidence of transcoding.
    // Higher scores indicate higher likelihood of fake/transcoded content.
    // ==========================================================================

    /// Helper: Create a minimal MP3-like structure with LAME header
    fn create_test_mp3_data(
        encoder_version: &str,
        lowpass_hz: u32,
        is_vbr: bool,
    ) -> Vec<u8> {
        let mut data = Vec::new();

        // MP3 frame sync
        data.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);

        // Padding before Xing/Info header
        data.extend_from_slice(&[0x00; 32]);

        // Xing or Info marker
        if is_vbr {
            data.extend_from_slice(b"Xing");
        } else {
            data.extend_from_slice(b"Info");
        }

        // Xing flags (all fields present)
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x0F]);

        // Frames count (4 bytes)
        data.extend_from_slice(&[0x00, 0x00, 0x10, 0x00]);

        // Bytes count (4 bytes)
        data.extend_from_slice(&[0x00, 0x10, 0x00, 0x00]);

        // TOC (100 bytes)
        data.extend_from_slice(&[0x00; 100]);

        // Quality (4 bytes)
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x64]);

        // LAME version string (9 bytes)
        let version_bytes = encoder_version.as_bytes();
        let mut lame_tag = [0u8; 9];
        let copy_len = version_bytes.len().min(9);
        lame_tag[..copy_len].copy_from_slice(&version_bytes[..copy_len]);
        data.extend_from_slice(&lame_tag);

        // VBR method + quality byte
        data.push(0x24);

        // Lowpass frequency / 100
        let lowpass_byte = (lowpass_hz / 100) as u8;
        data.push(lowpass_byte);

        // Padding to make it look realistic
        data.extend_from_slice(&[0x00; 200]);

        data
    }

    // ==========================================================================
    // LOWPASS MISMATCH DETECTION TESTS
    // ==========================================================================
    //
    // The lowpass mismatch is the MOST RELIABLE indicator of transcoding.
    //
    // When LAME encodes from a lossless source at 320kbps, it uses:
    //   lowpass ≈ 20500 Hz (keeping nearly all audible frequencies)
    //
    // When someone takes a 128kbps MP3 and re-encodes it as "320kbps":
    //   lowpass = 16000 Hz (because the original only had frequencies up to 16kHz)
    //
    // The LAME encoder HONESTLY RECORDS this, creating a forensic trail!
    // ==========================================================================

    #[test]
    fn test_lowpass_mismatch_flags_transcode() {
        // SCENARIO: Fake 320kbps file transcoded from 128kbps source
        // EVIDENCE: lowpass=16000Hz instead of expected ~20500Hz

        let data = create_test_mp3_data("LAME3.100", 16000, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 320);

        // Should have significant score due to lowpass mismatch
        assert!(
            result.score >= 35,
            "Lowpass mismatch should add 35+ points, got {}",
            result.score
        );

        // Should flag the mismatch
        assert!(
            result.flags.iter().any(|f| f.contains("lowpass_mismatch")),
            "Should flag lowpass mismatch: {:?}",
            result.flags
        );

        // Should record the lowpass value
        assert_eq!(result.lowpass, Some(16000));
    }

    #[test]
    fn test_legitimate_320_not_flagged() {
        // SCENARIO: Legitimate 320kbps encoding from lossless source
        // EVIDENCE: lowpass=20500Hz (appropriate for 320kbps)

        let data = create_test_mp3_data("LAME3.100", 20500, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 320);

        // Should have low/no score
        assert!(
            result.score < 35,
            "Legitimate 320kbps should not be flagged, got score {}",
            result.score
        );

        // Should NOT have lowpass_mismatch flag
        assert!(
            !result.flags.iter().any(|f| f.contains("lowpass_mismatch")),
            "Should not flag legitimate file: {:?}",
            result.flags
        );
    }

    #[test]
    fn test_encoder_version_extraction() {
        // The encoder version string tells us what software created the file
        // Common versions: "LAME3.99r", "LAME3.100", "LAME3.99.5"

        let data = create_test_mp3_data("LAME3.100", 20000, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 256);

        assert_eq!(result.encoder, "LAME3.100");
        assert_eq!(result.details.encoder_version, Some("LAME3.100".to_string()));
    }

    // ==========================================================================
    // VBR VS CBR DETECTION TESTS
    // ==========================================================================
    //
    // VBR (Variable Bit Rate): Uses "Xing" header marker
    //   - More efficient, better quality at same average size
    //   - Common for LAME V0, V2 settings
    //
    // CBR (Constant Bit Rate): Uses "Info" header marker
    //   - Every frame same size
    //   - Common for 320kbps "max quality" encodes
    //
    // Detection matters because VBR files naturally have variable frame sizes,
    // while CBR files with variable sizes are suspicious.
    // ==========================================================================

    #[test]
    fn test_vbr_detection() {
        let data = create_test_mp3_data("LAME3.99r", 19500, true);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 245);

        assert!(result.details.is_vbr, "Should detect VBR file");
    }

    #[test]
    fn test_cbr_detection() {
        let data = create_test_mp3_data("LAME3.100", 20500, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 320);

        assert!(!result.details.is_vbr, "Should detect CBR file");
    }

    // ==========================================================================
    // BINARY DETAILS STRUCTURE TESTS
    // ==========================================================================

    #[test]
    fn test_binary_details_populated() {
        // Verify all relevant details are captured for reporting

        let data = create_test_mp3_data("LAME3.100", 18500, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 192);

        // Lowpass should be recorded
        assert_eq!(result.details.lowpass, Some(18500));

        // Expected lowpass should be calculated
        assert!(result.details.expected_lowpass.is_some());

        // Encoder version should be captured
        assert_eq!(result.details.encoder_version, Some("LAME3.100".to_string()));
    }

    #[test]
    fn test_no_lame_header_fallback() {
        // Files without LAME headers should still get basic analysis
        // The result should have default/unknown values

        let data = vec![0xFF, 0xFB, 0x90, 0x00, 0x00, 0x00]; // Just MP3 sync
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 128);

        // Should have zero score (no evidence)
        assert_eq!(result.score, 0);

        // No lowpass data available
        assert!(result.lowpass.is_none());
    }

    // ==========================================================================
    // SCORING BREAKDOWN TESTS
    // ==========================================================================
    //
    // The scoring system is designed to give clear verdicts:
    //   0-34:  CLEAN (no evidence of transcoding)
    //   35-64: SUSPECT (some indicators, needs investigation)
    //   65+:   TRANSCODE (strong evidence of fake file)
    //
    // Each indicator contributes:
    //   - Lowpass mismatch: +35 (strong evidence)
    //   - Multiple encoders: +20 (moderate evidence)
    //   - Frame irregularities: +10 (weak evidence)
    // ==========================================================================

    #[test]
    fn test_score_lowpass_only() {
        // Just lowpass mismatch = 35 points (SUSPECT range)

        let data = create_test_mp3_data("LAME3.100", 16000, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 320);

        assert!(
            result.score >= 35 && result.score < 65,
            "Lowpass mismatch alone should be SUSPECT (35-64), got {}",
            result.score
        );
    }

    #[test]
    fn test_default_result() {
        // Default result should be clean (no evidence)

        let result = BinaryResult::default();

        assert_eq!(result.score, 0);
        assert!(result.flags.is_empty());
        assert_eq!(result.encoder, "unknown");
        assert!(result.lowpass.is_none());
    }

    // ==========================================================================
    // REAL-WORLD SCENARIO TESTS
    // ==========================================================================

    #[test]
    fn test_scenario_youtube_to_320_transcode() {
        // SCENARIO: Someone rips audio from YouTube (typically ~128kbps AAC)
        // and re-encodes as "320kbps MP3" for uploading elsewhere.
        //
        // EVIDENCE: YouTube audio has ~17kHz cutoff, so lowpass=17000Hz

        let data = create_test_mp3_data("LAME3.100", 17000, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 320);

        assert!(
            result.score >= 35,
            "YouTube transcode should be flagged, got score {}",
            result.score
        );
    }

    #[test]
    fn test_scenario_legitimate_v0_vbr() {
        // SCENARIO: Legitimate V0 VBR encoding from CD rip
        // V0 averages ~245kbps with lowpass ~19.5-20.5kHz

        let data = create_test_mp3_data("LAME3.99r", 20000, true);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 245);

        assert!(
            result.score < 35,
            "Legitimate V0 should not be flagged, got score {}",
            result.score
        );
    }

    #[test]
    fn test_scenario_legitimate_128_cbr() {
        // SCENARIO: Legitimate 128kbps encoding from lossless source
        // 128kbps has lowpass ~16kHz, which is EXPECTED for this bitrate

        let data = create_test_mp3_data("LAME3.100", 16000, false);
        let mut cursor = Cursor::new(data.clone());

        let result = analyze(&data, &mut cursor, 128);

        // 16kHz is expected for 128kbps - should NOT be flagged
        assert!(
            !result.flags.iter().any(|f| f.contains("lowpass_mismatch")),
            "128kbps with 16kHz lowpass is normal, should not flag"
        );
    }

    // ==========================================================================
    // RE-ENCODING DETECTION TESTS (Binary Analysis)
    // ==========================================================================
    //
    // These tests verify that the binary analyzer correctly populates the
    // re-encoding detection fields in BinaryDetails.
    //
    // The key insight: Even if a file is re-encoded at a HIGHER bitrate,
    // the encoding chain reveals it was processed multiple times, indicating
    // quality degradation that cannot be recovered.
    // ==========================================================================

    #[test]
    fn test_binary_details_default() {
        let details = BinaryDetails::default();

        assert_eq!(details.lame_occurrences, 0);
        assert_eq!(details.ffmpeg_occurrences, 0);
        assert!(details.encoding_chain.is_none());
        assert!(!details.reencoded);
    }

    #[test]
    fn test_reencoding_details_populated() {
        // Create test data with multiple encoder signatures
        let mut data = vec![0u8; 65536];

        // Add MP3 header
        data[0..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);

        // Add multiple LAME signatures (simulating re-encoding)
        data[100..109].copy_from_slice(b"LAME3.99r");
        data[500..509].copy_from_slice(b"LAME3.100");

        // Add FFmpeg signature
        data[1000..1004].copy_from_slice(b"Lavf");

        let mut cursor = Cursor::new(data.clone());
        let result = analyze(&data, &mut cursor, 320);

        // Should detect re-encoding
        assert!(result.details.reencoded, "Should detect re-encoding");
        assert!(result.details.lame_occurrences >= 2, "Should count LAME occurrences");
        assert!(result.details.ffmpeg_occurrences >= 1, "Should count FFmpeg occurrences");
        assert!(result.details.encoding_chain.is_some(), "Should have encoding chain");
    }

    #[test]
    fn test_reencoding_flags() {
        // Create test data with multiple encoder signatures
        let mut data = vec![0u8; 65536];
        data[0..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        data[100..109].copy_from_slice(b"LAME3.100");
        data[500..504].copy_from_slice(b"Lavf");

        let mut cursor = Cursor::new(data.clone());
        let result = analyze(&data, &mut cursor, 320);

        // Should have multi_encoder_sigs flag
        assert!(
            result.flags.iter().any(|f| f.contains("multi_encoder_sigs")),
            "Should flag multiple encoder signatures: {:?}",
            result.flags
        );

        // Should have encoding chain in flags
        assert!(
            result.flags.iter().any(|f| f.contains("encoding_chain")),
            "Should include encoding chain in flags: {:?}",
            result.flags
        );
    }

    #[test]
    fn test_multiple_lame_passes_flag() {
        // Create test data with multiple LAME signatures
        let mut data = vec![0u8; 65536];
        data[0..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        data[100..109].copy_from_slice(b"LAME3.99r");
        data[500..509].copy_from_slice(b"LAME3.100");
        data[1000..1009].copy_from_slice(b"LAME3.100");

        let mut cursor = Cursor::new(data.clone());
        let result = analyze(&data, &mut cursor, 320);

        // Should flag multiple LAME passes
        assert!(
            result.flags.iter().any(|f| f.contains("lame_reencoded")),
            "Should flag multiple LAME passes: {:?}",
            result.flags
        );
    }

    #[test]
    fn test_reencoding_scoring() {
        // Re-encoding should contribute to the score
        let mut data = vec![0u8; 65536];
        data[0..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        data[100..109].copy_from_slice(b"LAME3.100");
        data[500..504].copy_from_slice(b"Lavf");

        let mut cursor = Cursor::new(data.clone());
        let result = analyze(&data, &mut cursor, 320);

        // Multi-encoder should add 20 points
        assert!(
            result.score >= 20,
            "Re-encoding should add to score, got {}",
            result.score
        );
    }

    // ==========================================================================
    // SCENARIO TESTS: Higher bitrate re-encoding
    // ==========================================================================

    #[test]
    fn test_scenario_320_reencoded_to_320() {
        // SCENARIO: Someone has a 320kbps MP3, converts to WAV, then
        // re-encodes as 320kbps with a different encoder.
        // Result: Same bitrate, but WORSE quality (double lossy compression)
        //
        // This is detectable via multiple encoder signatures!

        let mut data = vec![0u8; 65536];
        data[0..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        data[100..109].copy_from_slice(b"LAME3.99r"); // Original encoder
        data[500..504].copy_from_slice(b"Lavf");      // FFmpeg re-encode

        let mut cursor = Cursor::new(data.clone());
        let result = analyze(&data, &mut cursor, 320);

        assert!(result.details.reencoded);
        assert!(result.score >= 20, "Double-compressed file should be flagged");
    }

    #[test]
    fn test_scenario_single_encode_not_flagged() {
        // SCENARIO: Clean single-pass encode
        // Should NOT be flagged as re-encoded

        let mut data = vec![0u8; 65536];
        data[0..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        data[100..109].copy_from_slice(b"LAME3.100"); // Single encoder

        let mut cursor = Cursor::new(data.clone());
        let result = analyze(&data, &mut cursor, 320);

        assert!(!result.details.reencoded);
        assert!(
            !result.flags.iter().any(|f| f.contains("lame_reencoded")),
            "Single encode should not be flagged as re-encoded"
        );
    }

    // ==========================================================================
    // BITRATE TIMELINE TESTS
    // ==========================================================================
    //
    // BitrateTimeline stores per-frame bitrate data for visualization.
    // This enables the bitrate timeline graph that shows how bitrate
    // varies over time, useful for identifying:
    //   - VBR vs CBR encoding
    //   - Suspicious bitrate patterns in re-encoded files
    //   - Whether a "320kbps" file actually has consistent bitrate
    // ==========================================================================

    #[test]
    fn test_bitrate_timeline_default() {
        let bt = BitrateTimeline::default();

        assert!(bt.times.is_empty());
        assert!(bt.bitrates.is_empty());
        assert!(!bt.is_vbr);
        assert_eq!(bt.min_bitrate, 0);
        assert_eq!(bt.max_bitrate, 0);
        assert_eq!(bt.avg_bitrate, 0);
    }

    #[test]
    fn test_bitrate_timeline_cbr_structure() {
        // CBR file has constant bitrate across all frames
        let bt = BitrateTimeline {
            times: vec![0.0, 0.026, 0.052, 0.078, 0.104],
            bitrates: vec![320, 320, 320, 320, 320],
            is_vbr: false,
            min_bitrate: 320,
            max_bitrate: 320,
            avg_bitrate: 320,
        };

        assert!(!bt.is_vbr, "Should be CBR");
        assert_eq!(bt.min_bitrate, bt.max_bitrate, "CBR has uniform bitrate");
        assert_eq!(bt.times.len(), bt.bitrates.len(), "Must have same length");
    }

    #[test]
    fn test_bitrate_timeline_vbr_structure() {
        // VBR file has varying bitrate
        let bt = BitrateTimeline {
            times: vec![0.0, 0.026, 0.052, 0.078],
            bitrates: vec![192, 256, 128, 320],
            is_vbr: true,
            min_bitrate: 128,
            max_bitrate: 320,
            avg_bitrate: 224,
        };

        assert!(bt.is_vbr, "Should be VBR");
        assert!(bt.min_bitrate < bt.max_bitrate, "VBR has varying bitrate");
    }

    #[test]
    fn test_bitrate_timeline_time_calculation() {
        // Time should increase by ~26ms per frame (1152 samples at 44.1kHz)
        const SAMPLES_PER_FRAME: f64 = 1152.0;
        const SAMPLE_RATE: f64 = 44100.0;
        let frame_duration = SAMPLES_PER_FRAME / SAMPLE_RATE;

        assert!((frame_duration - 0.026).abs() < 0.001, "Frame duration ~26ms");

        let times: Vec<f64> = (0..5).map(|i| i as f64 * frame_duration).collect();
        assert!((times[1] - 0.026).abs() < 0.001);
        assert!((times[4] - 0.104).abs() < 0.001);
    }

    #[test]
    fn test_bitrate_timeline_in_binary_details() {
        // BinaryDetails should be able to hold bitrate timeline
        let details = BinaryDetails {
            bitrate_timeline: Some(BitrateTimeline {
                times: vec![0.0, 0.026],
                bitrates: vec![320, 320],
                is_vbr: false,
                min_bitrate: 320,
                max_bitrate: 320,
                avg_bitrate: 320,
            }),
            ..Default::default()
        };

        assert!(details.bitrate_timeline.is_some());
        let bt = details.bitrate_timeline.unwrap();
        assert_eq!(bt.avg_bitrate, 320);
    }
}
