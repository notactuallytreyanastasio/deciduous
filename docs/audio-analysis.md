---
layout: default
title: Audio Analysis
---

# Audio Analysis: Finding Fake Lossless

Losselot uses dual analysis to detect transcodes: **binary metadata inspection** and **FFT-based spectral analysis**.

---

## The Quick Version

Lossy codecs like MP3 remove high frequencies. This creates permanent scars:

| Source Quality | Typical Cutoff | What's Missing |
|----------------|----------------|----------------|
| 128kbps MP3 | ~16kHz | Cymbals, air, sparkle |
| 192kbps MP3 | ~18kHz | Some high-end detail |
| 320kbps MP3 | ~20kHz | Only ultrasonic |
| True Lossless | ~22kHz | Nothing |

Converting an MP3 to FLAC doesn't restore lost frequencies. The scars remain.

---

## Spectral Analysis

### The Spectrogram

<img src="spectro.png" alt="Spectrogram Example" style="max-width: 100%; border-radius: 4px;">

A spectrogram shows frequency (vertical) over time (horizontal). Brightness = energy.

**What to look for:**
- **Horizontal cutoff lines** - Where frequencies suddenly stop
- **Dark bands at top** - Missing high frequencies
- **Consistent cutoff** - Same frequency across entire track

### The Frequency Response

<img src="everything.png" alt="Frequency Response" style="max-width: 100%;">

The frequency response curve shows exactly where audio cuts off. A sharp drop around 17-20kHz with steep rolloff is the telltale sign of lossy compression.

---

## Binary Analysis (MP3)

For MP3 files, Losselot reads encoder metadata embedded in the file.

### LAME Headers

The LAME encoder stores a "lowpass" value that reveals original encoding settings:

```
Header: LAME3.99r
Lowpass: 16000 Hz
Bitrate: 320 kbps

⚠️ MISMATCH: 320kbps should have ~20kHz lowpass
```

**The smoking gun:** A "320kbps" MP3 with a lowpass of 16kHz was definitely transcoded from a 128kbps source.

### Re-encoding Detection

Multiple encoder signatures in one file indicate re-encoding:

```
Found: LAME3.99r (at offset 0x100)
Found: LAME3.100 (at offset 0x2a0)
Found: Lavf58.29.100 (at offset 0x340)

⚠️ ENCODING CHAIN: LAME → LAME → FFmpeg
```

Each lossy pass causes cumulative damage.

---

## CFCC: Lo-Fi Detection {#cfcc}

Not all high-frequency rolloff indicates lossy compression. Tape recordings and lo-fi productions have natural rolloff.

### The Problem

```
MP3 @ 160kbps:          Tape Recording:
       |                        \
       |                         \
       |_____                     \___
      16kHz                      varies
   (brick wall)              (gradual rolloff)
```

Both have missing high frequencies - but for different reasons.

### The Solution

**Cross-Frequency Coherence Coefficient (CFCC)** measures correlation between adjacent frequency bands:

- **MP3**: Sudden decorrelation at cutoff (cliff)
- **Tape**: Gradual decorrelation following dynamics

### How It Works

1. Divide spectrum into adjacent bands
2. Measure correlation between each pair
3. Look for sudden correlation drops

```
Band Pair    | Correlation | Change
-------------|-------------|--------
14-15 kHz    | 0.92        | -
15-16 kHz    | 0.89        | -0.03
16-17 kHz    | 0.31        | -0.58  ← CLIFF DETECTED
17-18 kHz    | 0.28        | -0.03
```

A correlation drop > 0.25 between adjacent bands signals a codec cliff.

### Scoring

| Signal | Score Impact |
|--------|--------------|
| `cfcc_cliff` detected | +25 |
| `decorrelation_spike` | +15 |
| `lofi_safe_natural_rolloff` | -15 |

The negative score for natural rolloff prevents false positives on legitimate lo-fi recordings.

---

## Mixed-Source Detection

Some productions use both lossy and lossless sources:

```
22kHz  |    ██     ██  ██      |  ← Lossless elements
       |    ██     ██  ██      |
16kHz  |████████████████████████|  ← MP3 samples
       |████████████████████████|
0 Hz   |████████████████████████|
       0:00              4:00
```

The "pillars" of high-frequency content punching through indicate:
- Base track from MP3 samples
- Some elements (cymbals, synths) from lossless sources

This is **not a fake** - the file honestly contains what it contains. But it reveals the production used pre-lossy material.

---

## Verdicts

| Verdict | Score | Meaning |
|---------|-------|---------|
| **CLEAN** | 0-34 | Looks like genuine lossless |
| **SUSPECT** | 35-64 | Something's off - investigate |
| **TRANSCODE** | 65-100 | Almost certainly from lossy source |

---

## Detection Flags

### Spectral Flags
- `severe_hf_damage` - Major frequency loss
- `hf_cutoff_detected` - Clear lossy cutoff pattern
- `weak_ultrasonic_content` - Not enough content above 20kHz
- `dead_ultrasonic_band` - Virtually nothing above 20kHz
- `steep_hf_rolloff` - Unnaturally sharp cutoff
- `cfcc_cliff` - CFCC detected codec cliff
- `lofi_safe_natural_rolloff` - Natural rolloff (reduces score)

### Re-encoding Flags
- `multi_encoder_sigs` - Multiple encoders detected
- `encoding_chain(LAME → FFmpeg)` - Specific chain identified
- `lame_reencoded_x2` - Double LAME encoding
- `lowpass_bitrate_mismatch` - Lowpass doesn't match bitrate

---

## Try It

```bash
# Analyze a single file
./losselot suspicious-file.flac

# Analyze a folder
./losselot ~/Music/

# Interactive web UI
./losselot serve ~/Music/ --port 3000

# Quick scan (binary only, no FFT)
./losselot --no-spectral ~/Music/
```

---

[← Back to Home](/) | [Next: Decision Graph →](decision-graph)
