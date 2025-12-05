# Losselot

Detect "lossless" audio files that were actually created from lossy sources.

## The Problem

You download a FLAC or WAV file labeled as "lossless" - but how do you know it wasn't just an MP3 that someone converted? Once audio goes through lossy compression (MP3, AAC, etc.), the lost frequencies are gone forever. Converting to FLAC doesn't bring them back.

**Losselot detects these fake lossless files.**

## How It Works

Lossy codecs like MP3 work by removing high frequencies that are "less audible." A 128kbps MP3 typically cuts everything above ~16kHz. When you convert that MP3 to FLAC, the cutoff remains - it's a permanent scar.

Losselot performs **spectral analysis** to measure energy in different frequency bands:
- Real lossless audio has gradual, natural high-frequency rolloff
- Fake lossless (from MP3/AAC) has a sharp cliff where the original encoder cut frequencies

### What It Detects

| Source | Detection |
|--------|-----------|
| MP3 128kbps → FLAC | Easily detected (hard cutoff at ~16kHz) |
| MP3 192kbps → FLAC | Usually detected (cutoff at ~18kHz) |
| MP3 320kbps → FLAC | Difficult (cutoff at ~20kHz, near natural rolloff) |
| AAC 128kbps → FLAC | Sometimes detected (AAC is more efficient) |
| Real lossless | Shows 0% score, natural rolloff |

## Supported Formats

**Input formats:** FLAC, WAV, AIFF, MP3, M4A, AAC, OGG, Opus, WMA, ALAC

The primary use case is analyzing FLAC/WAV files, but Losselot can also detect MP3→MP3 transcodes using binary forensics (LAME header analysis).

## Installation

**No dependencies required** - binaries are fully self-contained.

### Pre-built Binaries (Recommended)

Download from [Releases](https://github.com/notactuallytreyanastasio/losselot/releases):

| Platform | Binary |
|----------|--------|
| macOS Apple Silicon | `losselot-darwin-arm64` |
| macOS Intel | `losselot-darwin-amd64` |
| Linux x86_64 | `losselot-linux-amd64` |
| Windows x86_64 | `losselot-windows-amd64.exe` |

```bash
# macOS/Linux: make executable and run
chmod +x losselot-*
./losselot-darwin-arm64 --help

# Or move to PATH
sudo mv losselot-darwin-arm64 /usr/local/bin/losselot
losselot --help
```

### From Source

Requires [Rust](https://rustup.rs/) (no other dependencies):

```bash
git clone https://github.com/notactuallytreyanastasio/losselot.git
cd losselot
cargo build --release
./target/release/losselot --help
```

### Via Cargo

```bash
cargo install --git https://github.com/notactuallytreyanastasio/losselot
```

## Usage

```
losselot [OPTIONS] <PATH>

Arguments:
  <PATH>  File or directory to analyze

Options:
  -o, --output <FILE>      Output report file (.html, .csv, .json)
  -j, --jobs <NUM>         Number of parallel workers (default: CPU count)
      --no-spectral        Skip spectral analysis (faster, binary-only)
  -v, --verbose            Show detailed analysis
  -q, --quiet              Only show summary
      --threshold <NUM>    Transcode threshold percentage [default: 65]
  -h, --help               Print help
  -V, --version            Print version
```

### Examples

```bash
# Check if a FLAC is really lossless
losselot album.flac

# Scan your entire lossless library
losselot ~/Music/FLAC/

# Generate HTML report
losselot -o report.html ~/Music/

# Show detailed spectral info
losselot -v suspicious.flac

# Parallel processing with 8 workers
losselot -j 8 ~/Music/
```

### Exit Codes

- `0`: All files clean
- `1`: Some files suspect
- `2`: Transcodes detected

## Understanding Results

### Verdicts

- **OK (0-34%)**: Appears to be genuine lossless
- **SUSPECT (35-64%)**: Might have lossy origins, worth investigating
- **TRANSCODE (65-100%)**: Almost certainly from a lossy source

### Flags

| Flag | Meaning |
|------|---------|
| `severe_hf_damage` | Major high frequency loss (probably from 128kbps or lower) |
| `hf_cutoff_detected` | Clear lossy cutoff pattern detected |
| `possible_lossy_origin` | Mild HF damage, possibly from high-bitrate lossy |
| `cliff_at_20khz` | Sharp cutoff at 20kHz (320kbps MP3 signature) |
| `steep_20khz_cutoff` | Significant drop at 20kHz boundary |
| `possible_320k_origin` | May have originated from 320kbps MP3 |
| `dead_ultrasonic_band` | No content above 20kHz (strong 320k indicator) |
| `weak_ultrasonic_content` | Low energy above 20kHz |
| `steep_hf_rolloff` | High frequencies drop off too sharply |
| `silent_17k+` | Upper frequencies (17-20kHz) are essentially silent |
| `silent_20k+` | Ultrasonic frequencies (20-22kHz) are silent |
| `lowpass_mismatch` | (MP3 only) LAME header lowpass doesn't match bitrate |

### Verbose Output

Use `-v` to see spectral details:
```
Spectral: full=-12.3dB high=-45.1dB upper=-68.2dB | drops: high=32.8 upper=42.1
```

- **full**: Overall signal level (20Hz-20kHz)
- **high**: 15-20kHz band level
- **upper**: 17-20kHz band level
- **drops**: Difference between bands (higher = more suspicious)

A real lossless file typically has `upper_drop` of 4-8 dB. A fake from MP3 128k might have 40-70 dB.

## Report Formats

### HTML
Dark-mode report with summary statistics, color-coded verdicts, and flag reference.

### CSV
```csv
verdict,filepath,bitrate_kbps,combined_score,spectral_score,binary_score,flags,encoder,lowpass
TRANSCODE,/path/to/fake.flac,0,80,80,0,severe_hf_damage,,
OK,/path/to/real.flac,0,0,0,0,,,
```

### JSON
```json
{
  "generated": "2024-01-01T00:00:00Z",
  "summary": {"total": 100, "ok": 85, "suspect": 10, "transcode": 5},
  "files": [...]
}
```

## Technical Details

### Spectral Analysis Method

1. Decode audio to PCM (using symphonia - pure Rust, no ffmpeg)
2. Apply Hanning window to ~15 seconds of audio
3. Perform FFT (8192-point) on overlapping windows
4. Measure average energy in frequency bands:
   - Full: 20Hz - 20kHz
   - Mid-high: 10kHz - 15kHz
   - High: 15kHz - 20kHz
   - Upper: 17kHz - 20kHz
5. Calculate drop between mid-high and upper bands
6. Score based on how severe the drop is

### MP3-Specific Analysis

For MP3 files, Losselot also performs binary forensics:
- Parses LAME/Xing headers for lowpass frequency
- If a "320kbps" MP3 has lowpass=16000Hz, it was transcoded from 128kbps
- Detects multiple encoder signatures (suggests re-encoding)

## Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test
```

## Limitations

- **High-bitrate lossy is hard to detect**: MP3 320kbps has cutoff near 20kHz, similar to natural rolloff
- **Some codecs are stealthier**: AAC and Vorbis are more efficient than MP3, leaving less obvious damage
- **Dark/quiet recordings**: Low energy in high frequencies is normal for some content
- **Not 100% definitive**: Use as one data point, not absolute proof

## License

MIT
