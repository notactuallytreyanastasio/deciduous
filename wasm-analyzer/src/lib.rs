use wasm_bindgen::prelude::*;
use rustfft::{FftPlanner, num_complex::Complex};
use serde::{Serialize, Deserialize};

const FFT_SIZE: usize = 8192;
const SAMPLE_RATE: f32 = 44100.0;

#[derive(Serialize, Deserialize)]
pub struct SpectralResult {
    pub verdict: String,
    pub score: u8,
    pub flags: Vec<String>,
    pub avg_cutoff_freq: f32,
    pub cutoff_variance: f32,
    pub rolloff_slope: f32,
    pub cfcc_cliff_detected: bool,
    pub frequency_response: Vec<f32>,
}

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

    /// Analyze PCM samples (f32 array from Web Audio API)
    #[wasm_bindgen]
    pub fn analyze(&mut self, samples: &[f32]) -> JsValue {
        let result = self.analyze_samples(samples);
        serde_wasm_bindgen::to_value(&result).unwrap()
    }

    fn analyze_samples(&mut self, samples: &[f32]) -> SpectralResult {
        let mut flags = Vec::new();
        let mut all_magnitudes: Vec<Vec<f32>> = Vec::new();
        let mut cutoff_freqs: Vec<f32> = Vec::new();

        // Process in windows
        let fft = self.fft_planner.plan_fft_forward(FFT_SIZE);
        let window = hann_window(FFT_SIZE);

        for chunk in samples.chunks(FFT_SIZE) {
            if chunk.len() < FFT_SIZE {
                break;
            }

            // Apply window and convert to complex
            let mut buffer: Vec<Complex<f32>> = chunk
                .iter()
                .zip(window.iter())
                .map(|(s, w)| Complex::new(s * w, 0.0))
                .collect();

            fft.process(&mut buffer);

            // Get magnitudes (only positive frequencies)
            let magnitudes: Vec<f32> = buffer[..FFT_SIZE / 2]
                .iter()
                .map(|c| (c.norm() / FFT_SIZE as f32).max(1e-10).log10() * 20.0)
                .collect();

            // Find cutoff frequency for this window
            let cutoff = find_cutoff_freq(&magnitudes, SAMPLE_RATE, FFT_SIZE);
            cutoff_freqs.push(cutoff);

            all_magnitudes.push(magnitudes);
        }

        if all_magnitudes.is_empty() {
            return SpectralResult {
                verdict: "ERROR".to_string(),
                score: 0,
                flags: vec!["insufficient_data".to_string()],
                avg_cutoff_freq: 0.0,
                cutoff_variance: 0.0,
                rolloff_slope: 0.0,
                cfcc_cliff_detected: false,
                frequency_response: vec![],
            };
        }

        // Average frequency response
        let num_bins = all_magnitudes[0].len();
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

        // CFCC analysis
        let cfcc_cliff = detect_cfcc_cliff(&avg_response, SAMPLE_RATE, FFT_SIZE);

        // Scoring
        let mut score: u8 = 0;

        // Check for brick-wall cutoff (low variance = MP3)
        if cutoff_variance < 500.0 && avg_cutoff < 20000.0 {
            score += 20;
            flags.push("low_cutoff_variance".to_string());
        }

        // Check for steep rolloff
        if rolloff_slope < -2.0 {
            score += 15;
            flags.push("steep_hf_rolloff".to_string());
        }

        // CFCC cliff detection
        if cfcc_cliff {
            score += 25;
            flags.push("cfcc_cliff".to_string());
        }

        // Check frequency bands
        let hf_energy = band_energy(&avg_response, 15000.0, 20000.0, SAMPLE_RATE, FFT_SIZE);
        let ultrasonic_energy = band_energy(&avg_response, 20000.0, 22000.0, SAMPLE_RATE, FFT_SIZE);

        if hf_energy < -60.0 {
            score += 15;
            flags.push("weak_hf_content".to_string());
        }

        if ultrasonic_energy < -70.0 {
            score += 10;
            flags.push("dead_ultrasonic".to_string());
        }

        // Natural rolloff detection (reduces score for lo-fi)
        if cutoff_variance > 1500.0 && rolloff_slope > -1.5 {
            score = score.saturating_sub(15);
            flags.push("lofi_safe_natural_rolloff".to_string());
        }

        let verdict = if score >= 65 {
            "TRANSCODE"
        } else if score >= 35 {
            "SUSPECT"
        } else {
            "CLEAN"
        }.to_string();

        SpectralResult {
            verdict,
            score,
            flags,
            avg_cutoff_freq: avg_cutoff,
            cutoff_variance,
            rolloff_slope,
            cfcc_cliff_detected: cfcc_cliff,
            frequency_response: avg_response,
        }
    }
}

fn hann_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / size as f32).cos()))
        .collect()
}

fn find_cutoff_freq(magnitudes: &[f32], sample_rate: f32, fft_size: usize) -> f32 {
    let bin_freq = sample_rate / fft_size as f32;
    let peak = magnitudes.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let threshold = peak - 20.0; // -20dB from peak

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
    let freq_diff = (end_bin - start_bin) as f32 * bin_freq / 1000.0; // in kHz

    (end_mag - start_mag) / freq_diff // dB per kHz
}

fn detect_cfcc_cliff(magnitudes: &[f32], sample_rate: f32, fft_size: usize) -> bool {
    let bin_freq = sample_rate / fft_size as f32;

    // Check known codec cutoff frequencies
    let cutoff_freqs = [16000.0, 17000.0, 18000.0, 19000.0, 20000.0];

    for &freq in &cutoff_freqs {
        let bin = (freq / bin_freq) as usize;
        if bin + 5 >= magnitudes.len() || bin < 5 {
            continue;
        }

        // Compare energy before and after potential cutoff
        let before: f32 = magnitudes[bin - 5..bin].iter().sum::<f32>() / 5.0;
        let after: f32 = magnitudes[bin..bin + 5].iter().sum::<f32>() / 5.0;

        // Sharp drop indicates codec cliff
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

#[wasm_bindgen(start)]
pub fn init() {
    // Set panic hook for better error messages
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}
