use wasm_bindgen::prelude::*;
use rustfft::{FftPlanner, num_complex::Complex};
use serde::{Serialize, Deserialize};

const FFT_SIZE: usize = 8192;
const SAMPLE_RATE: f32 = 44100.0;

// =============================================================================
// RESULT STRUCTURES
// =============================================================================

#[derive(Serialize, Deserialize, Default)]
pub struct BinaryAnalysis {
    pub encoder: Option<String>,
    pub lowpass: Option<u32>,
    pub is_vbr: bool,
    pub bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub frame_count: usize,
    pub frame_size_cv: f32,
    pub lame_count: usize,
    pub ffmpeg_count: usize,
    pub reencoded: bool,
    pub encoding_chain: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct BandEnergy {
    pub rms_full: f32,
    pub rms_mid_high: f32,
    pub rms_high: f32,
    pub rms_upper: f32,
    pub rms_19_20k: f32,
    pub rms_ultrasonic: f32,
}

#[derive(Serialize, Deserialize)]
pub struct AnalysisResult {
    pub verdict: String,
    pub score: u8,
    pub flags: Vec<String>,
    pub avg_cutoff_freq: f32,
    pub cutoff_variance: f32,
    pub rolloff_slope: f32,
    pub cfcc_cliff_detected: bool,
    pub natural_rolloff: bool,
    pub frequency_response: Vec<f32>,
    pub spectrogram_data: Vec<f32>,
    pub spectrogram_times: Vec<f32>,
    pub spectrogram_freqs: Vec<f32>,
    pub band_energy: BandEnergy,
    pub binary: BinaryAnalysis,
}

// =============================================================================
// MP3 FRAME PARSING
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MpegVersion { Mpeg1, Mpeg2, Mpeg25 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer { Layer1, Layer2, Layer3 }

#[derive(Debug, Clone, Copy)]
pub struct FrameHeader {
    pub version: MpegVersion,
    pub layer: Layer,
    pub bitrate: u32,
    pub sample_rate: u32,
    pub padding: bool,
    pub frame_size: u32,
}

// Bitrate tables
const BITRATES_V1_L3: [u32; 16] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0];
const SAMPLE_RATES_V1: [u32; 4] = [44100, 48000, 32000, 0];
const SAMPLE_RATES_V2: [u32; 4] = [22050, 24000, 16000, 0];
const SAMPLE_RATES_V25: [u32; 4] = [11025, 12000, 8000, 0];

impl FrameHeader {
    pub fn parse(header: [u8; 4]) -> Option<Self> {
        // Check sync word
        if header[0] != 0xFF || (header[1] & 0xE0) != 0xE0 {
            return None;
        }

        let version = match (header[1] >> 3) & 0x03 {
            0 => MpegVersion::Mpeg25,
            2 => MpegVersion::Mpeg2,
            3 => MpegVersion::Mpeg1,
            _ => return None,
        };

        let layer = match (header[1] >> 1) & 0x03 {
            1 => Layer::Layer3,
            2 => Layer::Layer2,
            3 => Layer::Layer1,
            _ => return None,
        };

        let bitrate_idx = ((header[2] >> 4) & 0x0F) as usize;
        let bitrate = BITRATES_V1_L3[bitrate_idx]; // Simplified for Layer3
        if bitrate == 0 {
            return None;
        }

        let sample_rate_idx = ((header[2] >> 2) & 0x03) as usize;
        let sample_rate = match version {
            MpegVersion::Mpeg1 => SAMPLE_RATES_V1[sample_rate_idx],
            MpegVersion::Mpeg2 => SAMPLE_RATES_V2[sample_rate_idx],
            MpegVersion::Mpeg25 => SAMPLE_RATES_V25[sample_rate_idx],
        };
        if sample_rate == 0 {
            return None;
        }

        let padding = (header[2] & 0x02) != 0;
        let padding_size = if padding { 1 } else { 0 };
        let frame_size = 144 * bitrate * 1000 / sample_rate + padding_size;

        Some(FrameHeader {
            version,
            layer,
            bitrate,
            sample_rate,
            padding,
            frame_size,
        })
    }
}

// =============================================================================
// LAME HEADER EXTRACTION
// =============================================================================

#[derive(Default)]
struct LameInfo {
    encoder: Option<String>,
    lowpass: Option<u32>,
    is_vbr: bool,
}

fn extract_lame_info(data: &[u8]) -> LameInfo {
    let mut info = LameInfo::default();
    let search_region = &data[..data.len().min(4096)];

    // Check for Xing or Info header
    if let Some(pos) = find_pattern(search_region, b"Xing") {
        info.is_vbr = true;
        find_lame_after_xing(search_region, pos, &mut info);
    } else if let Some(pos) = find_pattern(search_region, b"Info") {
        find_lame_after_xing(search_region, pos, &mut info);
    } else if let Some(pos) = find_pattern(&search_region[..search_region.len().min(500)], b"LAME") {
        extract_lame_tag(search_region, pos, &mut info);
    }

    info
}

fn find_lame_after_xing(data: &[u8], xing_pos: usize, info: &mut LameInfo) {
    // Skip Xing header (4 bytes) + flags (4 bytes) + optional fields
    let mut offset = xing_pos + 8;

    if offset + 4 <= data.len() {
        let flags = u32::from_be_bytes([data[xing_pos + 4], data[xing_pos + 5], data[xing_pos + 6], data[xing_pos + 7]]);
        if flags & 0x01 != 0 { offset += 4; } // frames
        if flags & 0x02 != 0 { offset += 4; } // bytes
        if flags & 0x04 != 0 { offset += 100; } // TOC
        if flags & 0x08 != 0 { offset += 4; } // quality
    }

    // Look for LAME after Xing data
    let search_end = (offset + 50).min(data.len());
    if let Some(rel_pos) = find_pattern(&data[offset..search_end], b"LAME") {
        extract_lame_tag(data, offset + rel_pos, info);
    } else if let Some(rel_pos) = find_pattern(&data[offset..search_end], b"Lavc") {
        let lavc_pos = offset + rel_pos;
        let version_end = (lavc_pos + 12).min(data.len());
        if let Ok(version) = std::str::from_utf8(&data[lavc_pos..version_end]) {
            info.encoder = Some(version.trim_end_matches('\0').to_string());
        }
    }
}

fn extract_lame_tag(data: &[u8], lame_pos: usize, info: &mut LameInfo) {
    let version_end = (lame_pos + 9).min(data.len());
    if let Ok(version) = std::str::from_utf8(&data[lame_pos..version_end]) {
        info.encoder = Some(version.trim_end_matches('\0').to_string());
    }

    // Lowpass at offset 10 from LAME string
    if lame_pos + 10 < data.len() {
        let lowpass_byte = data[lame_pos + 10];
        if lowpass_byte >= 50 && lowpass_byte <= 220 {
            info.lowpass = Some(lowpass_byte as u32 * 100);
        }
    }
}

// =============================================================================
// ENCODER SIGNATURE SCANNING
// =============================================================================

fn scan_encoder_signatures(data: &[u8]) -> (usize, usize, bool) {
    let header_region = &data[..data.len().min(4096)];

    // Count LAME signatures
    let lame_count = count_pattern(header_region, b"LAME3.")
        + count_pattern(header_region, b"LAME ")
        .max(if find_pattern(header_region, b"LAME").is_some() { 1 } else { 0 });

    // Count FFmpeg signatures
    let lavf_count = count_pattern(header_region, b"Lavf");
    let lavc_count = count_pattern(header_region, b"Lavc");
    let ffmpeg_count = lavf_count.max(lavc_count);

    let reencoded = lame_count > 1 || ffmpeg_count > 1 || (lame_count > 0 && ffmpeg_count > 0);

    (lame_count, ffmpeg_count, reencoded)
}

fn build_encoding_chain(lame_count: usize, ffmpeg_count: usize) -> Option<String> {
    let mut parts = Vec::new();

    if lame_count > 1 {
        parts.push(format!("LAME x{}", lame_count));
    } else if lame_count == 1 {
        parts.push("LAME".to_string());
    }

    if ffmpeg_count > 1 {
        parts.push(format!("FFmpeg x{}", ffmpeg_count));
    } else if ffmpeg_count == 1 {
        parts.push("FFmpeg".to_string());
    }

    if parts.len() > 1 {
        Some(parts.join(" → "))
    } else if lame_count > 1 || ffmpeg_count > 1 {
        Some(parts.join(""))
    } else {
        None
    }
}

// =============================================================================
// FRAME STATISTICS
// =============================================================================

fn scan_frames(data: &[u8], max_frames: usize) -> (usize, Vec<u32>, Vec<u32>, u32) {
    let mut frame_count = 0;
    let mut bitrates = Vec::new();
    let mut frame_sizes = Vec::new();
    let mut sample_rate = 0u32;

    // Skip ID3v2 if present
    let mut pos = 0;
    if data.len() > 10 && &data[0..3] == b"ID3" {
        let size = ((data[6] as u32 & 0x7F) << 21)
            | ((data[7] as u32 & 0x7F) << 14)
            | ((data[8] as u32 & 0x7F) << 7)
            | (data[9] as u32 & 0x7F);
        pos = 10 + size as usize;
    }

    while pos + 4 <= data.len() && frame_count < max_frames {
        let header = [data[pos], data[pos + 1], data[pos + 2], data[pos + 3]];

        if let Some(frame) = FrameHeader::parse(header) {
            frame_count += 1;
            bitrates.push(frame.bitrate);
            frame_sizes.push(frame.frame_size);
            if sample_rate == 0 {
                sample_rate = frame.sample_rate;
            }
            pos += frame.frame_size as usize;
        } else {
            pos += 1;
        }
    }

    (frame_count, bitrates, frame_sizes, sample_rate)
}

fn frame_size_cv(sizes: &[u32]) -> f32 {
    if sizes.is_empty() {
        return 0.0;
    }
    let mean: f64 = sizes.iter().map(|&x| x as f64).sum::<f64>() / sizes.len() as f64;
    if mean == 0.0 {
        return 0.0;
    }
    let variance: f64 = sizes.iter()
        .map(|&x| (x as f64 - mean).powi(2))
        .sum::<f64>() / sizes.len() as f64;
    ((variance.sqrt() / mean) * 100.0) as f32
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

fn find_pattern(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn count_pattern(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0;
    }
    let mut count = 0;
    let mut pos = 0;
    while pos <= haystack.len() - needle.len() {
        if let Some(found) = find_pattern(&haystack[pos..], needle) {
            count += 1;
            pos += found + needle.len();
        } else {
            break;
        }
    }
    count
}

// =============================================================================
// MAIN ANALYZER
// =============================================================================

#[wasm_bindgen]
pub struct Analyzer {
    fft_planner: FftPlanner<f32>,
}

#[wasm_bindgen]
impl Analyzer {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Analyzer {
        Analyzer {
            fft_planner: FftPlanner::new(),
        }
    }

    /// Analyze PCM samples only (legacy method)
    #[wasm_bindgen]
    pub fn analyze(&mut self, samples: &[f32]) -> JsValue {
        let result = self.full_analyze(samples, &[]);
        serde_wasm_bindgen::to_value(&result).unwrap()
    }

    /// Full analysis with both PCM samples and raw file bytes
    #[wasm_bindgen]
    pub fn analyze_full(&mut self, samples: &[f32], raw_bytes: &[u8]) -> JsValue {
        let result = self.full_analyze(samples, raw_bytes);
        serde_wasm_bindgen::to_value(&result).unwrap()
    }

    fn full_analyze(&mut self, samples: &[f32], raw_bytes: &[u8]) -> AnalysisResult {
        // Binary analysis (if raw bytes provided)
        let binary = if !raw_bytes.is_empty() {
            self.analyze_binary(raw_bytes)
        } else {
            BinaryAnalysis::default()
        };

        // Spectral analysis
        let spectral = self.analyze_spectral(samples);

        // Combine scores
        let mut score = spectral.score;
        let mut flags = spectral.flags;

        // Add binary flags
        if binary.reencoded {
            score = score.saturating_add(20);
            flags.push("multi_encoder_sigs".to_string());
        }

        if let Some(lowpass) = binary.lowpass {
            if let Some(bitrate) = binary.bitrate {
                if lowpass < expected_lowpass(bitrate) - 2000 {
                    score = score.saturating_add(15);
                    flags.push("lowpass_bitrate_mismatch".to_string());
                }
            }
        }

        let verdict = if score >= 65 {
            "TRANSCODE"
        } else if score >= 35 {
            "SUSPECT"
        } else {
            "OK"
        }.to_string();

        AnalysisResult {
            verdict,
            score,
            flags,
            avg_cutoff_freq: spectral.avg_cutoff_freq,
            cutoff_variance: spectral.cutoff_variance,
            rolloff_slope: spectral.rolloff_slope,
            cfcc_cliff_detected: spectral.cfcc_cliff_detected,
            natural_rolloff: spectral.natural_rolloff,
            frequency_response: spectral.frequency_response,
            spectrogram_data: spectral.spectrogram_data,
            spectrogram_times: spectral.spectrogram_times,
            spectrogram_freqs: spectral.spectrogram_freqs,
            band_energy: spectral.band_energy,
            binary,
        }
    }

    fn analyze_binary(&self, data: &[u8]) -> BinaryAnalysis {
        // Check if this is an MP3
        let is_mp3 = find_pattern(&data[..data.len().min(4096)], b"\xFF\xFB").is_some()
            || find_pattern(&data[..data.len().min(4096)], b"\xFF\xFA").is_some()
            || find_pattern(&data[..data.len().min(10)], b"ID3").is_some();

        if !is_mp3 {
            return BinaryAnalysis::default();
        }

        // Extract LAME info
        let lame = extract_lame_info(data);

        // Scan encoder signatures
        let (lame_count, ffmpeg_count, reencoded) = scan_encoder_signatures(data);

        // Scan frames
        let (frame_count, bitrates, frame_sizes, sample_rate) = scan_frames(data, 1000);

        let avg_bitrate = if !bitrates.is_empty() {
            Some(bitrates.iter().sum::<u32>() / bitrates.len() as u32)
        } else {
            None
        };

        let cv = frame_size_cv(&frame_sizes);
        let chain = build_encoding_chain(lame_count, ffmpeg_count);

        BinaryAnalysis {
            encoder: lame.encoder,
            lowpass: lame.lowpass,
            is_vbr: lame.is_vbr || bitrates.iter().collect::<std::collections::HashSet<_>>().len() > 1,
            bitrate: avg_bitrate,
            sample_rate: Some(sample_rate).filter(|&r| r > 0),
            frame_count,
            frame_size_cv: cv,
            lame_count,
            ffmpeg_count,
            reencoded,
            encoding_chain: chain,
        }
    }

    fn analyze_spectral(&mut self, samples: &[f32]) -> SpectralAnalysis {
        let mut flags = Vec::new();
        let mut all_magnitudes: Vec<Vec<f32>> = Vec::new();
        let mut cutoff_freqs: Vec<f32> = Vec::new();

        let fft = self.fft_planner.plan_fft_forward(FFT_SIZE);
        let window = hann_window(FFT_SIZE);

        for chunk in samples.chunks(FFT_SIZE) {
            if chunk.len() < FFT_SIZE {
                break;
            }

            let mut buffer: Vec<Complex<f32>> = chunk
                .iter()
                .zip(window.iter())
                .map(|(s, w)| Complex::new(s * w, 0.0))
                .collect();

            fft.process(&mut buffer);

            let magnitudes: Vec<f32> = buffer[..FFT_SIZE / 2]
                .iter()
                .map(|c| (c.norm() / FFT_SIZE as f32).max(1e-10).log10() * 20.0)
                .collect();

            let cutoff = find_cutoff_freq(&magnitudes, SAMPLE_RATE, FFT_SIZE);
            cutoff_freqs.push(cutoff);
            all_magnitudes.push(magnitudes);
        }

        if all_magnitudes.is_empty() {
            return SpectralAnalysis::error();
        }

        // Build spectrogram data
        let num_windows = all_magnitudes.len().min(100); // Limit for performance
        let step = all_magnitudes.len() / num_windows;
        let num_bins = all_magnitudes[0].len();

        let mut spectrogram_data = Vec::with_capacity(num_windows * num_bins);
        let mut spectrogram_times = Vec::with_capacity(num_windows);

        for (i, idx) in (0..all_magnitudes.len()).step_by(step.max(1)).take(num_windows).enumerate() {
            spectrogram_times.push(i as f32 * (samples.len() as f32 / SAMPLE_RATE) / num_windows as f32);
            spectrogram_data.extend_from_slice(&all_magnitudes[idx]);
        }

        let spectrogram_freqs: Vec<f32> = (0..num_bins)
            .map(|i| i as f32 * SAMPLE_RATE / FFT_SIZE as f32)
            .collect();

        // Average frequency response
        let mut avg_response = vec![0.0f32; num_bins];
        for mags in &all_magnitudes {
            for (i, &m) in mags.iter().enumerate() {
                avg_response[i] += m;
            }
        }
        for m in &mut avg_response {
            *m /= all_magnitudes.len() as f32;
        }

        // Calculate metrics
        let avg_cutoff = cutoff_freqs.iter().sum::<f32>() / cutoff_freqs.len() as f32;
        let cutoff_variance = variance(&cutoff_freqs);
        let rolloff_slope = calculate_rolloff_slope(&avg_response, SAMPLE_RATE, FFT_SIZE);
        let cfcc_cliff = detect_cfcc_cliff(&avg_response, SAMPLE_RATE, FFT_SIZE);

        // Band energy calculations
        let rms_full = band_energy(&avg_response, 0.0, 20000.0, SAMPLE_RATE, FFT_SIZE);
        let rms_mid_high = band_energy(&avg_response, 10000.0, 15000.0, SAMPLE_RATE, FFT_SIZE);
        let rms_high = band_energy(&avg_response, 15000.0, 20000.0, SAMPLE_RATE, FFT_SIZE);
        let rms_upper = band_energy(&avg_response, 17000.0, 20000.0, SAMPLE_RATE, FFT_SIZE);
        let rms_19_20k = band_energy(&avg_response, 19000.0, 20000.0, SAMPLE_RATE, FFT_SIZE);
        let rms_ultrasonic = band_energy(&avg_response, 20000.0, 22000.0, SAMPLE_RATE, FFT_SIZE);

        let band_energy_result = BandEnergy {
            rms_full,
            rms_mid_high,
            rms_high,
            rms_upper,
            rms_19_20k,
            rms_ultrasonic,
        };

        // Scoring
        let mut score: u8 = 0;

        if cutoff_variance < 500.0 && avg_cutoff < 20000.0 {
            score += 20;
            flags.push("low_cutoff_variance".to_string());
        }

        if rolloff_slope < -2.0 {
            score += 15;
            flags.push("steep_hf_rolloff".to_string());
        }

        if cfcc_cliff {
            score += 25;
            flags.push("cfcc_cliff".to_string());
        }

        if rms_high < -60.0 {
            score += 15;
            flags.push("weak_hf_content".to_string());
        }

        if rms_ultrasonic < -70.0 {
            score += 10;
            flags.push("dead_ultrasonic".to_string());
        }

        // Natural rolloff detection
        let natural_rolloff = cutoff_variance > 1500.0 && rolloff_slope > -1.5;
        if natural_rolloff {
            score = score.saturating_sub(15);
            flags.push("lofi_safe_natural_rolloff".to_string());
        }

        SpectralAnalysis {
            score,
            flags,
            avg_cutoff_freq: avg_cutoff,
            cutoff_variance,
            rolloff_slope,
            cfcc_cliff_detected: cfcc_cliff,
            natural_rolloff,
            frequency_response: avg_response,
            spectrogram_data,
            spectrogram_times,
            spectrogram_freqs,
            band_energy: band_energy_result,
        }
    }
}

struct SpectralAnalysis {
    score: u8,
    flags: Vec<String>,
    avg_cutoff_freq: f32,
    cutoff_variance: f32,
    rolloff_slope: f32,
    cfcc_cliff_detected: bool,
    natural_rolloff: bool,
    frequency_response: Vec<f32>,
    spectrogram_data: Vec<f32>,
    spectrogram_times: Vec<f32>,
    spectrogram_freqs: Vec<f32>,
    band_energy: BandEnergy,
}

impl SpectralAnalysis {
    fn error() -> Self {
        SpectralAnalysis {
            score: 0,
            flags: vec!["insufficient_data".to_string()],
            avg_cutoff_freq: 0.0,
            cutoff_variance: 0.0,
            rolloff_slope: 0.0,
            cfcc_cliff_detected: false,
            natural_rolloff: false,
            frequency_response: vec![],
            spectrogram_data: vec![],
            spectrogram_times: vec![],
            spectrogram_freqs: vec![],
            band_energy: BandEnergy::default(),
        }
    }
}

// =============================================================================
// SPECTRAL HELPER FUNCTIONS
// =============================================================================

fn hann_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / size as f32).cos()))
        .collect()
}

fn find_cutoff_freq(magnitudes: &[f32], sample_rate: f32, fft_size: usize) -> f32 {
    let bin_freq = sample_rate / fft_size as f32;
    let peak = magnitudes.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let threshold = peak - 20.0;

    for (i, &mag) in magnitudes.iter().enumerate().rev() {
        if mag > threshold {
            return i as f32 * bin_freq;
        }
    }
    0.0
}

fn variance(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32
}

fn calculate_rolloff_slope(magnitudes: &[f32], sample_rate: f32, fft_size: usize) -> f32 {
    let bin_freq = sample_rate / fft_size as f32;
    let start_bin = (15000.0 / bin_freq) as usize;
    let end_bin = (20000.0 / bin_freq) as usize;

    if end_bin >= magnitudes.len() || start_bin >= end_bin {
        return 0.0;
    }

    let start_mag = magnitudes[start_bin];
    let end_mag = magnitudes[end_bin];
    let freq_diff = (end_bin - start_bin) as f32 * bin_freq / 1000.0;

    (end_mag - start_mag) / freq_diff
}

fn detect_cfcc_cliff(magnitudes: &[f32], sample_rate: f32, fft_size: usize) -> bool {
    let bin_freq = sample_rate / fft_size as f32;
    let cutoff_freqs = [16000.0, 17000.0, 18000.0, 19000.0, 20000.0];

    for &freq in &cutoff_freqs {
        let bin = (freq / bin_freq) as usize;
        if bin + 5 >= magnitudes.len() || bin < 5 {
            continue;
        }

        let before: f32 = magnitudes[bin - 5..bin].iter().sum::<f32>() / 5.0;
        let after: f32 = magnitudes[bin..bin + 5].iter().sum::<f32>() / 5.0;

        if before - after > 15.0 {
            return true;
        }
    }
    false
}

fn band_energy(magnitudes: &[f32], low_freq: f32, high_freq: f32, sample_rate: f32, fft_size: usize) -> f32 {
    let bin_freq = sample_rate / fft_size as f32;
    let start = (low_freq / bin_freq) as usize;
    let end = (high_freq / bin_freq).min(magnitudes.len() as f32) as usize;

    if start >= end || start >= magnitudes.len() {
        return -100.0;
    }

    magnitudes[start..end].iter().sum::<f32>() / (end - start) as f32
}

fn expected_lowpass(bitrate: u32) -> u32 {
    if bitrate >= 320 { 20500 }
    else if bitrate >= 256 { 20000 }
    else if bitrate >= 224 { 19500 }
    else if bitrate >= 192 { 18500 }
    else if bitrate >= 160 { 17500 }
    else if bitrate >= 128 { 16000 }
    else { 15000 }
}

#[wasm_bindgen(start)]
pub fn init() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}
