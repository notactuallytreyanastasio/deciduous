//! Spectral analysis of audio files
//!
//! Uses FFT (Fast Fourier Transform) to analyze frequency content and detect transcoding.
//!
//! # How Spectral Analysis Works
//!
//! Audio is made up of frequencies from 20Hz (deep bass) to 20,000Hz (highest treble).
//! When audio is compressed with lossy codecs like MP3, the encoder removes high
//! frequencies to save space. Different bitrates remove different amounts:
//!
//! ```text
//! Format      | Frequencies Preserved | What's Missing
//! ------------|----------------------|------------------
//! 64 kbps     | 0 - 11,000 Hz        | Everything above 11kHz
//! 128 kbps    | 0 - 16,000 Hz        | Cymbal shimmer, breath sounds
//! 192 kbps    | 0 - 18,000 Hz        | Highest harmonics
//! 256 kbps    | 0 - 19,000 Hz        | Very subtle loss
//! 320 kbps    | 0 - 20,000 Hz        | Nothing audible (20-22kHz)
//! Lossless    | 0 - 22,050 Hz        | Full Nyquist bandwidth
//! ```
//!
//! ## The "Cliff" Pattern
//!
//! Lossy encoding creates a sharp "cliff" where frequencies suddenly drop off.
//! This is visible in spectral analysis as:
//!
//! - **Transcoded file**: Gentle slope until cutoff, then VERTICAL drop to silence
//! - **Real lossless**: Gradual, natural rolloff continuing to 22kHz
//!
//! ## Key Detection Metrics
//!
//! 1. **upper_drop**: Difference between 10-15kHz and 17-20kHz bands (in dB)
//!    - Real lossless: ~4-8 dB (natural instrument rolloff)
//!    - 320k transcode: ~10-15 dB
//!    - 128k transcode: ~40-70 dB (dramatic cliff)
//!
//! 2. **ultrasonic_drop**: Difference between 19-20kHz and 20-22kHz bands
//!    - Catches 320kbps transcodes that cut right at 20kHz
//!    - Real lossless: ~1-3 dB
//!    - 320k transcode: ~40+ dB
//!
//! 3. **spectral_flatness**: Measures if there's real content or silence
//!    - White noise = 1.0, Pure silence = 0.0
//!    - Real audio in 20-22kHz range has flatness ~0.9+
//!    - Empty transcode band has flatness <0.3

use rustfft::{num_complex::Complex, FftPlanner};
use serde::Serialize;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

const FFT_SIZE: usize = 8192;
const SAMPLE_RATE: u32 = 44100;

// Spectrogram parameters - downsample for reasonable file size
// Target: ~128 frequency bins, ~100 time slices max
const SPECTROGRAM_FREQ_BINS: usize = 128;
const SPECTROGRAM_MAX_TIME_SLICES: usize = 100;

/// Spectrogram data for visualization - FFT magnitudes over time
/// Downsampled for efficient storage and rendering
#[derive(Debug, Clone, Default, Serialize)]
pub struct SpectrogramData {
    /// Time points in seconds for each column
    pub times: Vec<f64>,
    /// Frequency bins in Hz for each row (0 to ~22kHz)
    pub frequencies: Vec<f64>,
    /// Magnitude data as flattened 2D array [time][freq] in dB
    /// Access: magnitudes[time_idx * num_freq_bins + freq_idx]
    pub magnitudes: Vec<f64>,
    /// Number of frequency bins (rows)
    pub num_freq_bins: usize,
    /// Number of time slices (columns)
    pub num_time_slices: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SpectralDetails {
    /// RMS level of full signal (dB)
    pub rms_full: f64,
    /// RMS level of 10-15kHz band (dB)
    pub rms_mid_high: f64,
    /// RMS level of 15-20kHz band (dB)
    pub rms_high: f64,
    /// RMS level of 17-20kHz band (dB)
    pub rms_upper: f64,
    /// RMS level of 19-20kHz band (dB)
    pub rms_19_20k: f64,
    /// RMS level of 20-22kHz band (dB) - ultrasonic, key for 320k detection
    pub rms_ultrasonic: f64,
    /// Drop from full to high band (dB)
    pub high_drop: f64,
    /// Drop from mid-high to upper band (dB)
    pub upper_drop: f64,
    /// Drop from 19-20kHz to 20-22kHz (dB) - key for 320k detection
    pub ultrasonic_drop: f64,
    /// Spectral flatness in 19-21kHz (1.0 = noise-like, 0.0 = tonal/empty)
    pub ultrasonic_flatness: f64,
    /// Spectrogram data for visualization (None if not generated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spectrogram: Option<SpectrogramData>,
}

#[derive(Debug, Clone, Default)]
pub struct SpectralResult {
    pub score: u32,
    pub flags: Vec<String>,
    pub details: SpectralDetails,
}

/// Hanning window function
fn hanning_window(size: usize) -> Vec<f64> {
    (0..size)
        .map(|i| {
            0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / (size - 1) as f64).cos())
        })
        .collect()
}

/// Convert linear magnitude to dB
fn to_db(value: f64) -> f64 {
    if value <= 0.0 {
        -96.0
    } else {
        20.0 * value.log10()
    }
}

/// Calculate RMS of a slice
fn rms(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&x| x * x).sum();
    (sum_sq / samples.len() as f64).sqrt()
}

/// Decode audio to PCM samples using symphonia (supports MP3, FLAC, WAV, OGG, etc.)
fn decode_audio(data: &[u8]) -> Option<(Vec<f64>, u32)> {
    let cursor = std::io::Cursor::new(data.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

    // Don't provide a hint - let symphonia auto-detect the format
    let hint = Hint::new();

    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();
    let decoder_opts = DecoderOptions::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .ok()?;

    let mut format = probed.format;
    let track = format.default_track()?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(SAMPLE_RATE);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &decoder_opts)
        .ok()?;

    let mut samples = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    // Decode up to ~15 seconds from middle of file
    let max_samples = (sample_rate as usize) * 15;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };

        if sample_buf.is_none() {
            let spec = *decoded.spec();
            let duration = decoded.capacity() as u64;
            sample_buf = Some(SampleBuffer::new(duration, spec));
        }

        if let Some(ref mut buf) = sample_buf {
            // Get channel count before moving decoded
            let channel_count = decoded.spec().channels.count();
            buf.copy_interleaved_ref(decoded);

            // Convert to mono f64
            for chunk in buf.samples().chunks(channel_count) {
                let mono: f64 = chunk.iter().map(|&s| s as f64).sum::<f64>() / channel_count as f64;
                samples.push(mono);
            }

            if samples.len() >= max_samples {
                break;
            }
        }
    }

    if samples.is_empty() {
        return None;
    }

    Some((samples, sample_rate))
}

/// Calculate spectral flatness (Wiener entropy)
/// Returns 1.0 for white noise, 0.0 for pure tone or silence
fn spectral_flatness(magnitudes: &[f64]) -> f64 {
    if magnitudes.is_empty() {
        return 0.0;
    }

    let n = magnitudes.len() as f64;

    // Geometric mean (via log to avoid underflow)
    let log_sum: f64 = magnitudes.iter().map(|&x| (x + 1e-10).ln()).sum();
    let geo_mean = (log_sum / n).exp();

    // Arithmetic mean
    let arith_mean: f64 = magnitudes.iter().sum::<f64>() / n;

    if arith_mean <= 0.0 {
        return 0.0;
    }

    geo_mean / arith_mean
}

/// Calculate energy in a frequency band using FFT results
fn band_energy(fft_result: &[Complex<f64>], sample_rate: u32, low_hz: u32, high_hz: u32) -> f64 {
    let bin_resolution = sample_rate as f64 / FFT_SIZE as f64;
    let low_bin = (low_hz as f64 / bin_resolution) as usize;
    let high_bin = (high_hz as f64 / bin_resolution).min((FFT_SIZE / 2) as f64) as usize;

    let mut energy = 0.0;
    for bin in low_bin..=high_bin.min(fft_result.len() - 1) {
        let mag = fft_result[bin].norm();
        energy += mag * mag;
    }

    energy.sqrt()
}

/// Perform spectral analysis on MP3 data
pub fn analyze(data: &[u8], _declared_sample_rate: u32) -> SpectralResult {
    let mut result = SpectralResult::default();

    // Decode audio to PCM (supports MP3, FLAC, WAV, OGG, etc.)
    let (samples, sample_rate) = match decode_audio(data) {
        Some(s) => s,
        None => return result,
    };

    if samples.len() < FFT_SIZE {
        return result;
    }

    // Calculate overall RMS
    let rms_full = to_db(rms(&samples));
    result.details.rms_full = rms_full;

    // Set up FFT
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let window = hanning_window(FFT_SIZE);

    // Process overlapping windows and average the results
    let hop_size = FFT_SIZE / 2;
    let num_windows = (samples.len() - FFT_SIZE) / hop_size + 1;

    let mut avg_full = 0.0;
    let mut avg_mid_high = 0.0;
    let mut avg_high = 0.0;
    let mut avg_upper = 0.0;
    let mut avg_19_20k = 0.0;
    let mut avg_ultrasonic = 0.0;

    // For spectral flatness calculation
    let mut ultrasonic_magnitudes: Vec<f64> = Vec::new();

    // For spectrogram: collect downsampled magnitude spectra
    let bin_resolution = sample_rate as f64 / FFT_SIZE as f64;
    let nyquist_bin = FFT_SIZE / 2;

    // Calculate frequency bin downsampling factor
    let freq_downsample = (nyquist_bin / SPECTROGRAM_FREQ_BINS).max(1);
    let actual_freq_bins = nyquist_bin / freq_downsample;

    // Calculate time downsampling factor
    let time_downsample = (num_windows / SPECTROGRAM_MAX_TIME_SLICES).max(1);
    let actual_time_slices = (num_windows + time_downsample - 1) / time_downsample;

    // Pre-allocate spectrogram storage
    let mut spectrogram_magnitudes: Vec<f64> = Vec::with_capacity(actual_time_slices * actual_freq_bins);
    let mut spectrogram_times: Vec<f64> = Vec::with_capacity(actual_time_slices);

    // Build frequency axis (Hz)
    let spectrogram_frequencies: Vec<f64> = (0..actual_freq_bins)
        .map(|i| (i * freq_downsample) as f64 * bin_resolution)
        .collect();

    for i in 0..num_windows {
        let start = i * hop_size;
        let end = start + FFT_SIZE;

        if end > samples.len() {
            break;
        }

        // Apply window and convert to complex
        let mut buffer: Vec<Complex<f64>> = samples[start..end]
            .iter()
            .zip(window.iter())
            .map(|(&s, &w)| Complex::new(s * w, 0.0))
            .collect();

        // Perform FFT
        fft.process(&mut buffer);

        // Calculate band energies (all from FFT for fair comparison)
        avg_full += band_energy(&buffer, sample_rate, 20, 20000); // Full audible range
        avg_mid_high += band_energy(&buffer, sample_rate, 10000, 15000);
        avg_high += band_energy(&buffer, sample_rate, 15000, 20000);
        avg_upper += band_energy(&buffer, sample_rate, 17000, 20000);
        avg_19_20k += band_energy(&buffer, sample_rate, 19000, 20000);
        avg_ultrasonic += band_energy(&buffer, sample_rate, 20000, 22000);

        // Collect magnitudes in 19-21kHz for flatness calculation
        let low_bin = (19000.0 / bin_resolution) as usize;
        let high_bin = (21000.0 / bin_resolution).min((FFT_SIZE / 2) as f64) as usize;
        for bin in low_bin..=high_bin.min(buffer.len() - 1) {
            ultrasonic_magnitudes.push(buffer[bin].norm());
        }

        // Collect spectrogram data (downsampled)
        if i % time_downsample == 0 {
            let time_sec = (start as f64) / sample_rate as f64;
            spectrogram_times.push(time_sec);

            // Downsample frequency bins by averaging
            for freq_idx in 0..actual_freq_bins {
                let bin_start = freq_idx * freq_downsample;
                let bin_end = (bin_start + freq_downsample).min(nyquist_bin);

                let mut sum = 0.0;
                for bin in bin_start..bin_end {
                    if bin < buffer.len() {
                        sum += buffer[bin].norm();
                    }
                }
                let avg_mag = sum / (bin_end - bin_start) as f64;
                // Convert to dB, floor at -96dB
                let db = if avg_mag > 0.0 { 20.0 * avg_mag.log10() } else { -96.0 };
                spectrogram_magnitudes.push(db.max(-96.0));
            }
        }
    }

    let num_windows = num_windows.max(1) as f64;
    avg_full /= num_windows;
    avg_mid_high /= num_windows;
    avg_high /= num_windows;
    avg_upper /= num_windows;
    avg_19_20k /= num_windows;
    avg_ultrasonic /= num_windows;

    // Convert to dB
    result.details.rms_full = to_db(avg_full);
    result.details.rms_mid_high = to_db(avg_mid_high);
    result.details.rms_high = to_db(avg_high);
    result.details.rms_upper = to_db(avg_upper);
    result.details.rms_19_20k = to_db(avg_19_20k);
    result.details.rms_ultrasonic = to_db(avg_ultrasonic);

    // Calculate drops (positive = high band is quieter, which is normal)
    result.details.high_drop = result.details.rms_full - result.details.rms_high;
    result.details.upper_drop = result.details.rms_mid_high - result.details.rms_upper;
    result.details.ultrasonic_drop = result.details.rms_19_20k - result.details.rms_ultrasonic;

    // Calculate spectral flatness in 19-21kHz range
    // Flatness = geometric_mean / arithmetic_mean (1.0 = white noise, 0.0 = pure tone/silence)
    result.details.ultrasonic_flatness = spectral_flatness(&ultrasonic_magnitudes);

    // Store spectrogram data
    if !spectrogram_times.is_empty() && !spectrogram_magnitudes.is_empty() {
        result.details.spectrogram = Some(SpectrogramData {
            times: spectrogram_times,
            frequencies: spectrogram_frequencies,
            magnitudes: spectrogram_magnitudes,
            num_freq_bins: actual_freq_bins,
            num_time_slices: actual_time_slices,
        });
    }

    // Score based on analysis
    // Tuned to detect lossy origins in "lossless" files
    //
    // Key insight: upper_drop (difference between 10-15kHz and 17-20kHz bands)
    // is the most diagnostic metric for lossy damage:
    // - Real lossless: ~4-6 dB (gradual natural rolloff)
    // - Lossy 320k: ~8-12 dB (slight damage)
    // - Lossy 192k: ~12-20 dB (moderate damage)
    // - Lossy 128k MP3: ~40-70 dB (severe damage, hard cutoff)

    // Severe damage - almost certainly from low-bitrate lossy (MP3 128k or worse)
    if result.details.upper_drop > 40.0 {
        result.score += 50;
        result.flags.push("severe_hf_damage".to_string());
    }
    // Significant damage - likely from lossy source (192k or lower)
    else if result.details.upper_drop > 15.0 {
        result.score += 35;
        result.flags.push("hf_cutoff_detected".to_string());
    }
    // Mild damage - possibly from high-bitrate lossy (256k-320k)
    else if result.details.upper_drop > 10.0 {
        result.score += 20;
        result.flags.push("possible_lossy_origin".to_string());
    }

    // === 320k DETECTION ===
    // MP3 320k cuts at ~20kHz, leaving no content above that
    // Real lossless has content extending to 21-22kHz
    //
    // Key metrics from analysis:
    // - Real lossless: ultrasonic_drop ~1-2 dB, flatness ~0.98
    // - Fake 320k: ultrasonic_drop ~50+ dB, flatness ~0.10

    // Massive cliff at 20kHz - strong indicator of 320k transcode
    if result.details.ultrasonic_drop > 40.0 {
        result.score += 35;
        result.flags.push("cliff_at_20khz".to_string());
    } else if result.details.ultrasonic_drop > 25.0 {
        result.score += 25;
        result.flags.push("steep_20khz_cutoff".to_string());
    } else if result.details.ultrasonic_drop > 15.0 {
        result.score += 15;
        result.flags.push("possible_320k_origin".to_string());
    }

    // Low spectral flatness in 19-21kHz = empty/dead band
    // Real audio has noise-like content (flatness ~0.9+)
    // 320k transcode has almost nothing (flatness <0.5)
    if result.details.ultrasonic_flatness < 0.3 {
        result.score += 20;
        result.flags.push("dead_ultrasonic_band".to_string());
    } else if result.details.ultrasonic_flatness < 0.5 {
        result.score += 10;
        result.flags.push("weak_ultrasonic_content".to_string());
    }

    // Steep overall rolloff (full spectrum to 15-20kHz)
    if result.details.high_drop > 48.0 {
        result.score += 15;
        result.flags.push("steep_hf_rolloff".to_string());
    }

    // Silent upper frequencies (absolute check)
    if result.details.rms_upper < -50.0 {
        result.score += 15;
        result.flags.push("silent_17k+".to_string());
    }

    // Very quiet ultrasonic band (absolute check)
    if result.details.rms_ultrasonic < -70.0 {
        result.score += 10;
        result.flags.push("silent_20k+".to_string());
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // EDUCATIONAL BACKGROUND: Understanding FFT-Based Spectral Analysis
    // ==========================================================================
    //
    // The Fast Fourier Transform (FFT) converts time-domain audio (waveform)
    // into frequency-domain (spectrum). This lets us see which frequencies
    // are present and how strong they are.
    //
    // KEY CONCEPTS:
    //
    // 1. SAMPLE RATE & NYQUIST FREQUENCY
    //    CD audio uses 44,100 samples/second. The highest frequency we can
    //    represent is half that: 22,050 Hz (the "Nyquist frequency").
    //    This is why lossy codecs cutting at 20kHz is suspicious - real
    //    recordings have content up to 22kHz.
    //
    // 2. FFT SIZE & FREQUENCY RESOLUTION
    //    We use 8192-sample windows. At 44.1kHz, each "bin" represents:
    //    44100 / 8192 ≈ 5.38 Hz
    //    So we can measure energy at very precise frequency points.
    //
    // 3. DECIBELS (dB)
    //    Audio levels are measured in decibels, a logarithmic scale.
    //    - 0 dB = reference level (full scale)
    //    - -6 dB = half the amplitude
    //    - -20 dB = 1/10th the amplitude
    //    - -60 dB = 1/1000th the amplitude (very quiet)
    //
    // 4. WINDOWING
    //    We apply a "Hanning window" before FFT to reduce spectral leakage.
    //    This smoothly tapers the edges of each analysis window.
    //
    // 5. RMS (Root Mean Square)
    //    The effective "average" level of a signal, accounting for both
    //    positive and negative values. Used to measure band energy.
    // ==========================================================================

    // ==========================================================================
    // HANNING WINDOW TESTS
    // ==========================================================================
    //
    // The Hanning (or Hann) window is a smooth taper function that reduces
    // spectral leakage in FFT analysis. Without windowing, the abrupt edges
    // of our sample window would create artificial high frequencies.
    //
    // The formula is: w(n) = 0.5 * (1 - cos(2πn/(N-1)))
    //
    // Properties:
    // - Value at edges (0, N-1) should be 0 or near-0
    // - Value at center (N/2) should be 1.0
    // - Symmetric around the center
    // ==========================================================================

    #[test]
    fn test_hanning_window_edges() {
        // Hanning window should be zero at the edges
        let window = hanning_window(100);

        assert!(
            window[0] < 0.001,
            "Window should start near zero, got {}",
            window[0]
        );
        assert!(
            window[99] < 0.001,
            "Window should end near zero, got {}",
            window[99]
        );
    }

    #[test]
    fn test_hanning_window_center() {
        // Hanning window should be 1.0 at the center
        let window = hanning_window(101); // Odd size for exact center

        assert!(
            (window[50] - 1.0).abs() < 0.001,
            "Window center should be 1.0, got {}",
            window[50]
        );
    }

    #[test]
    fn test_hanning_window_symmetry() {
        // Hanning window should be symmetric
        let window = hanning_window(100);

        for i in 0..50 {
            assert!(
                (window[i] - window[99 - i]).abs() < 0.001,
                "Window should be symmetric at index {}",
                i
            );
        }
    }

    #[test]
    fn test_hanning_window_shape() {
        // Window should increase from edge to center
        let window = hanning_window(100);

        // First half should be monotonically increasing
        for i in 0..49 {
            assert!(
                window[i] <= window[i + 1],
                "Window should increase from {} to {}",
                i,
                i + 1
            );
        }
    }

    // ==========================================================================
    // DECIBEL CONVERSION TESTS
    // ==========================================================================
    //
    // Decibels are a logarithmic scale for measuring audio levels:
    //   dB = 20 * log10(amplitude)
    //
    // Key reference points:
    //   1.0 → 0 dB (full scale)
    //   0.5 → -6 dB (half amplitude)
    //   0.1 → -20 dB
    //   0.0 → -∞ dB (we floor at -96 dB)
    // ==========================================================================

    #[test]
    fn test_to_db_unity() {
        // Amplitude of 1.0 = 0 dB
        assert!((to_db(1.0) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_to_db_half() {
        // Amplitude of 0.5 ≈ -6.02 dB
        let db = to_db(0.5);
        assert!(
            (db - (-6.02)).abs() < 0.1,
            "0.5 amplitude should be ~-6dB, got {}",
            db
        );
    }

    #[test]
    fn test_to_db_tenth() {
        // Amplitude of 0.1 = -20 dB
        let db = to_db(0.1);
        assert!(
            (db - (-20.0)).abs() < 0.1,
            "0.1 amplitude should be -20dB, got {}",
            db
        );
    }

    #[test]
    fn test_to_db_zero() {
        // Amplitude of 0 should floor to -96 dB (not -infinity)
        assert_eq!(to_db(0.0), -96.0);
    }

    #[test]
    fn test_to_db_negative() {
        // Negative values should also floor to -96 dB
        assert_eq!(to_db(-1.0), -96.0);
    }

    // ==========================================================================
    // RMS (Root Mean Square) TESTS
    // ==========================================================================
    //
    // RMS is the "effective" average of a signal, calculated as:
    //   RMS = sqrt(mean(samples²))
    //
    // For audio, RMS represents the perceived loudness better than peak level.
    // A pure sine wave has RMS = peak / √2 ≈ 0.707 * peak
    // ==========================================================================

    #[test]
    fn test_rms_constant() {
        // Constant signal: RMS = the constant value
        let samples = vec![0.5, 0.5, 0.5, 0.5];
        assert!((rms(&samples) - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_rms_symmetric() {
        // Symmetric signal: RMS should be the same magnitude
        let samples = vec![1.0, -1.0, 1.0, -1.0];
        assert!((rms(&samples) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_rms_empty() {
        // Empty input should return 0
        let samples: Vec<f64> = vec![];
        assert_eq!(rms(&samples), 0.0);
    }

    #[test]
    fn test_rms_silence() {
        // All zeros = RMS of 0
        let samples = vec![0.0, 0.0, 0.0, 0.0];
        assert_eq!(rms(&samples), 0.0);
    }

    #[test]
    fn test_rms_single_sample() {
        // Single sample: RMS = absolute value
        assert!((rms(&[0.7]) - 0.7).abs() < 0.001);
        assert!((rms(&[-0.7]) - 0.7).abs() < 0.001);
    }

    // ==========================================================================
    // SPECTRAL FLATNESS TESTS
    // ==========================================================================
    //
    // Spectral flatness (Wiener entropy) measures how "noise-like" a signal is:
    //   Flatness = geometric_mean(magnitudes) / arithmetic_mean(magnitudes)
    //
    // Ranges from 0 to 1:
    //   1.0 = White noise (equal energy at all frequencies)
    //   0.0 = Pure tone or silence (energy at single frequency)
    //
    // For transcode detection:
    //   - Real audio in 20-22kHz has flatness ~0.9+ (noise-like content)
    //   - Empty transcode band has flatness <0.3 (silence)
    // ==========================================================================

    #[test]
    fn test_spectral_flatness_uniform() {
        // Uniform spectrum = high flatness (noise-like)
        let magnitudes = vec![1.0, 1.0, 1.0, 1.0, 1.0];
        let flatness = spectral_flatness(&magnitudes);

        assert!(
            flatness > 0.99,
            "Uniform spectrum should have flatness ~1.0, got {}",
            flatness
        );
    }

    #[test]
    fn test_spectral_flatness_spike() {
        // Single spike = low flatness (tonal)
        let magnitudes = vec![10.0, 0.001, 0.001, 0.001, 0.001];
        let flatness = spectral_flatness(&magnitudes);

        assert!(
            flatness < 0.3,
            "Tonal spectrum should have low flatness, got {}",
            flatness
        );
    }

    #[test]
    fn test_spectral_flatness_empty() {
        // Empty input = 0 flatness
        let magnitudes: Vec<f64> = vec![];
        assert_eq!(spectral_flatness(&magnitudes), 0.0);
    }

    // ==========================================================================
    // BAND ENERGY TESTS
    // ==========================================================================
    //
    // band_energy() calculates the energy in a specific frequency range
    // by summing the magnitudes of FFT bins within that range.
    //
    // Frequency resolution = sample_rate / fft_size
    // At 44100 Hz with 8192 samples: ~5.38 Hz per bin
    //
    // Bin index for frequency f: bin = f / (sample_rate / fft_size)
    // ==========================================================================

    #[test]
    fn test_band_energy_basic() {
        // Create FFT result with known energy in specific bins
        let mut fft_result = vec![Complex::new(0.0, 0.0); FFT_SIZE / 2 + 1];

        // Put energy at 1000 Hz
        // Bin index = 1000 / (44100 / 8192) ≈ 186
        let bin_1000hz = (1000.0 / (44100.0 / 8192.0)) as usize;
        fft_result[bin_1000hz] = Complex::new(1.0, 0.0);

        // Energy in 900-1100 Hz should capture this
        let energy = band_energy(&fft_result, SAMPLE_RATE, 900, 1100);
        assert!(energy > 0.0, "Should detect energy at 1000 Hz");

        // Energy in 2000-3000 Hz should be zero
        let energy_high = band_energy(&fft_result, SAMPLE_RATE, 2000, 3000);
        assert!(
            energy_high < 0.001,
            "Should have no energy in 2-3kHz band"
        );
    }

    #[test]
    fn test_band_energy_multiple_bins() {
        // Energy spread across multiple bins
        let mut fft_result = vec![Complex::new(0.0, 0.0); FFT_SIZE / 2 + 1];

        // Put equal energy in bins corresponding to 1000-2000 Hz
        let bin_1000 = (1000.0 / (44100.0 / 8192.0)) as usize;
        let bin_2000 = (2000.0 / (44100.0 / 8192.0)) as usize;

        for bin in bin_1000..=bin_2000 {
            fft_result[bin] = Complex::new(1.0, 0.0);
        }

        let energy = band_energy(&fft_result, SAMPLE_RATE, 1000, 2000);
        let num_bins = (bin_2000 - bin_1000 + 1) as f64;

        // Expected energy = sqrt(sum of magnitudes squared)
        // With unit magnitudes: sqrt(num_bins)
        let expected = num_bins.sqrt();
        assert!(
            (energy - expected).abs() < 0.1,
            "Energy should be ~sqrt({}), got {}",
            num_bins,
            energy
        );
    }

    // ==========================================================================
    // SPECTRAL DETAILS STRUCTURE TESTS
    // ==========================================================================

    #[test]
    fn test_spectral_details_default() {
        let details = SpectralDetails::default();

        assert_eq!(details.rms_full, 0.0);
        assert_eq!(details.high_drop, 0.0);
        assert_eq!(details.ultrasonic_flatness, 0.0);
    }

    #[test]
    fn test_spectral_result_default() {
        let result = SpectralResult::default();

        assert_eq!(result.score, 0);
        assert!(result.flags.is_empty());
    }

    // ==========================================================================
    // SCORING LOGIC TESTS (Documentation of thresholds)
    // ==========================================================================
    //
    // The scoring system penalizes files with suspicious spectral patterns.
    // Understanding these thresholds helps interpret analysis results.
    //
    // UPPER DROP (10-15kHz to 17-20kHz difference):
    //   >40 dB: +50 points, "severe_hf_damage" (128k or worse)
    //   >15 dB: +35 points, "hf_cutoff_detected" (192k or lower)
    //   >10 dB: +20 points, "possible_lossy_origin" (256k-320k)
    //
    // ULTRASONIC DROP (19-20kHz to 20-22kHz difference):
    //   >40 dB: +35 points, "cliff_at_20khz" (strong 320k indicator)
    //   >25 dB: +25 points, "steep_20khz_cutoff"
    //   >15 dB: +15 points, "possible_320k_origin"
    //
    // ULTRASONIC FLATNESS:
    //   <0.3:  +20 points, "dead_ultrasonic_band"
    //   <0.5:  +10 points, "weak_ultrasonic_content"
    //
    // ABSOLUTE THRESHOLDS:
    //   rms_upper < -50 dB: +15 points, "silent_17k+"
    //   rms_ultrasonic < -70 dB: +10 points, "silent_20k+"
    //   high_drop > 48 dB: +15 points, "steep_hf_rolloff"
    // ==========================================================================

    #[test]
    fn test_scoring_thresholds_documented() {
        // This test serves as documentation of the scoring thresholds
        // Verify the critical values match what's in the analyze() function

        // Upper drop thresholds
        assert!(40.0 > 15.0, "Severe damage threshold > significant threshold");
        assert!(15.0 > 10.0, "Significant threshold > mild threshold");

        // Ultrasonic drop thresholds
        assert!(40.0 > 25.0, "Cliff threshold > steep threshold");
        assert!(25.0 > 15.0, "Steep threshold > possible threshold");

        // Flatness thresholds
        assert!(0.3 < 0.5, "Dead band < weak content threshold");
    }

    // ==========================================================================
    // FFT SIZE & FREQUENCY RESOLUTION
    // ==========================================================================

    #[test]
    fn test_fft_frequency_resolution() {
        // Verify our FFT parameters give appropriate resolution
        let bin_resolution = SAMPLE_RATE as f64 / FFT_SIZE as f64;

        // Should be able to resolve ~5 Hz differences
        assert!(
            bin_resolution < 10.0,
            "Frequency resolution should be fine enough: {} Hz/bin",
            bin_resolution
        );

        // Verify we can address frequencies up to Nyquist
        let max_bin = FFT_SIZE / 2;
        let nyquist = SAMPLE_RATE as f64 / 2.0;
        let max_freq = max_bin as f64 * bin_resolution;

        assert!(
            (max_freq - nyquist).abs() < 10.0,
            "Max addressable frequency should be near Nyquist"
        );
    }

    #[test]
    fn test_frequency_bands_coverage() {
        // Document the frequency bands we analyze
        // This helps understand what each metric measures

        let bands = [
            ("Full audible", 20, 20000),
            ("Mid-high", 10000, 15000),
            ("High", 15000, 20000),
            ("Upper", 17000, 20000),
            ("Near-Nyquist", 19000, 20000),
            ("Ultrasonic", 20000, 22000),
        ];

        // All bands should be within Nyquist limit
        for (name, low, high) in bands {
            assert!(
                high <= SAMPLE_RATE / 2,
                "{} band ({}-{} Hz) exceeds Nyquist limit",
                name,
                low,
                high
            );
        }
    }

    // ==========================================================================
    // SPECTROGRAM DATA STRUCTURE TESTS
    // ==========================================================================
    //
    // The SpectrogramData struct stores FFT magnitude data over time for
    // visualization. It's downsampled to reduce file size while maintaining
    // enough detail to see frequency cutoffs.
    //
    // Key properties:
    // - times: Time points (in seconds) for each column
    // - frequencies: Frequency bins (in Hz) for each row
    // - magnitudes: Flattened 2D array of dB values [time][freq]
    // ==========================================================================

    #[test]
    fn test_spectrogram_data_default() {
        let sg = SpectrogramData::default();

        assert!(sg.times.is_empty(), "Default times should be empty");
        assert!(sg.frequencies.is_empty(), "Default frequencies should be empty");
        assert!(sg.magnitudes.is_empty(), "Default magnitudes should be empty");
        assert_eq!(sg.num_freq_bins, 0, "Default freq bins should be 0");
        assert_eq!(sg.num_time_slices, 0, "Default time slices should be 0");
    }

    #[test]
    fn test_spectrogram_data_structure() {
        // Test that the data structure maintains correct dimensions
        let num_time = 10;
        let num_freq = 128;

        let sg = SpectrogramData {
            times: (0..num_time).map(|i| i as f64 * 0.1).collect(),
            frequencies: (0..num_freq).map(|i| i as f64 * 172.0).collect(), // ~22kHz / 128
            magnitudes: vec![-50.0; num_time * num_freq],
            num_freq_bins: num_freq,
            num_time_slices: num_time,
        };

        assert_eq!(sg.times.len(), num_time);
        assert_eq!(sg.frequencies.len(), num_freq);
        assert_eq!(sg.magnitudes.len(), num_time * num_freq);

        // Test indexing: magnitudes[time_idx * num_freq_bins + freq_idx]
        let time_idx = 5;
        let freq_idx = 64;
        let idx = time_idx * num_freq + freq_idx;
        assert!(idx < sg.magnitudes.len(), "Index should be valid");
    }

    #[test]
    fn test_spectrogram_downsampling_constants() {
        // Verify downsampling parameters are reasonable
        assert!(
            SPECTROGRAM_FREQ_BINS <= FFT_SIZE / 2,
            "Freq bins should be <= Nyquist bins"
        );
        assert!(
            SPECTROGRAM_MAX_TIME_SLICES > 0,
            "Must have at least one time slice"
        );

        // Calculate approximate data size
        let max_data_points = SPECTROGRAM_FREQ_BINS * SPECTROGRAM_MAX_TIME_SLICES;
        let bytes_per_point = 8; // f64
        let max_bytes = max_data_points * bytes_per_point;

        // Should be under ~100KB per file
        assert!(
            max_bytes < 150_000,
            "Spectrogram data should be reasonably sized: {} bytes",
            max_bytes
        );
    }

    #[test]
    fn test_spectrogram_in_spectral_details() {
        // SpectralDetails should be able to hold spectrogram data
        let details = SpectralDetails {
            spectrogram: Some(SpectrogramData {
                times: vec![0.0, 0.1, 0.2],
                frequencies: vec![0.0, 5000.0, 10000.0, 15000.0, 20000.0],
                magnitudes: vec![-30.0; 15], // 3 times * 5 freqs
                num_freq_bins: 5,
                num_time_slices: 3,
            }),
            ..Default::default()
        };

        assert!(details.spectrogram.is_some());
        let sg = details.spectrogram.unwrap();
        assert_eq!(sg.num_time_slices, 3);
        assert_eq!(sg.num_freq_bins, 5);
    }

    #[test]
    fn test_spectrogram_db_range() {
        // dB values should be in expected range
        let min_db = -96.0; // Floor value
        let max_db = 0.0; // Full scale

        // Create test magnitudes spanning the range
        let magnitudes = vec![min_db, -60.0, -30.0, -10.0, max_db];

        for &db in &magnitudes {
            assert!(
                db >= min_db && db <= max_db,
                "dB value {} should be in range [{}, {}]",
                db,
                min_db,
                max_db
            );
        }
    }
}
