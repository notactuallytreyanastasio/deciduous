# Losselot

**Audio forensics meets AI-assisted development.**

Losselot started as a tool to detect fake lossless audio files. It evolved into something more: a living experiment in how AI and humans can build software together, with every decision tracked and queryable.

![Losselot Demo](docs/demo.gif)

---

## What This Project Is

**Two things at once:**

1. **An Audio Forensics Tool** - Detect if your FLAC/WAV files are actually transcoded from MP3s. Uses spectral analysis, binary metadata parsing, and lo-fi detection to expose the truth.

2. **A Living Museum** - Every algorithm choice, every rejected approach, every "why did we do it this way?" is captured in a queryable decision graph. Watch the project evolve in real-time.

**[Explore the Live Site](https://notactuallytreyanastasio.github.io/losselot/)** | **[Browse the Decision Graph](https://notactuallytreyanastasio.github.io/losselot/demo/)**

---

## Quick Start

```bash
git clone https://github.com/notactuallytreyanastasio/losselot.git
cd losselot
cargo build --release
./target/release/losselot serve examples/ --port 3000
```

Open [localhost:3000](http://localhost:3000) - you'll see the interactive analysis UI.

**No test files?** Generate them:
```bash
cd examples && ./generate_test_files.sh
```

---

## Downloads

| Platform | Download |
|----------|----------|
| **Mac (Apple Silicon)** | [Download](https://github.com/notactuallytreyanastasio/losselot/releases/latest/download/losselot-darwin-arm64) |
| **Mac (Intel)** | [Download](https://github.com/notactuallytreyanastasio/losselot/releases/latest/download/losselot-darwin-amd64) |
| **Windows** | [Download](https://github.com/notactuallytreyanastasio/losselot/releases/latest/download/losselot-windows-amd64.exe) |
| **Linux (AppImage)** | [Download](https://github.com/notactuallytreyanastasio/losselot/releases/latest/download/losselot-linux-amd64.AppImage) |
| **Linux (CLI)** | [Download](https://github.com/notactuallytreyanastasio/losselot/releases/latest/download/losselot-linux-amd64) |

---

## The Verdicts

| Verdict | Score | Meaning |
|---------|-------|---------|
| **CLEAN** | 0-34 | Genuine lossless - natural frequency content |
| **SUSPECT** | 35-64 | Something's off - investigate further |
| **TRANSCODE** | 65-100 | Fake lossless - clear compression damage |

---

<details>
<summary><b>Audio Analysis Deep Dive</b></summary>

### How Detection Works

#### Spectral Analysis

Lossy codecs remove high frequencies to save space. The "scars" are permanent:

| Source | Typical Cutoff |
|--------|----------------|
| 128kbps MP3 | ~16kHz |
| 192kbps MP3 | ~18kHz |
| 320kbps MP3 | ~20kHz |
| True Lossless | ~22kHz |

#### Binary Analysis (MP3)

For MP3 files, we read encoder metadata (LAME headers). A "320kbps" file with a 16kHz lowpass was definitely transcoded from 128kbps - the encoder honestly reports what it kept.

#### Lo-Fi Detection (CFCC)

Not all high-frequency rolloff is compression damage. Tape recordings have natural rolloff.

**The difference:**
- **MP3**: Brick-wall cutoff at fixed frequency
- **Tape**: Gradual rolloff that varies with dynamics

We use Cross-Frequency Coherence (CFCC) to distinguish them.

#### Re-encoding Detection

Multiple encoder signatures = multiple lossy passes = cumulative damage. We detect LAME, FFmpeg, Fraunhofer, and chains between them.

### Detection Flags

**Spectral:**
- `severe_hf_damage` - Major frequency loss
- `hf_cutoff_detected` - Clear lossy cutoff found
- `dead_ultrasonic_band` - No content above 20kHz

**Re-encoding:**
- `multi_encoder_sigs` - Multiple encoders detected
- `encoding_chain(LAME → FFmpeg)` - Specific chain identified
- `lame_reencoded_x2` - Re-encoded through LAME twice

### Supported Formats

FLAC, WAV, AIFF, MP3, M4A, AAC, OGG, Opus, ALAC

</details>

---

<details>
<summary><b>Decision Graph & Memory System</b></summary>

### The Problem

Claude (and LLMs generally) lose context. Sessions end, memory compacts, decisions evaporate. Six months later, no one remembers *why* we chose CFCC over temporal variance analysis.

### The Solution

Every decision is tracked in a queryable graph that persists forever:

```bash
# See all decisions ever made
./target/release/losselot db nodes

# See how they connect
./target/release/losselot db edges

# Full graph as JSON
./target/release/losselot db graph
```

### Node Types

| Type | Purpose |
|------|---------|
| **Goal** | High-level objectives |
| **Decision** | Choice points with options |
| **Option** | Approaches considered |
| **Action** | What was implemented |
| **Outcome** | What happened |
| **Observation** | Technical insights |

### Confidence Weights

Every node can have a confidence score (0-100):

```bash
# Add with confidence
./target/release/losselot db add-node -t decision "Use CFCC" -c 85
```

- **70-100**: High confidence - proven approach
- **40-69**: Medium - reasonable, some uncertainty
- **0-39**: Low - experimental

### Commit Linking

Link decisions to specific code changes:

```bash
./target/release/losselot db add-node -t action "Implemented feature" -c 90 --commit abc123
```

The demo UI shows clickable commit badges that link to GitHub.

### Why This Matters

- **Queryable history**: "Why didn't we use temporal variance?" → search the graph
- **Rejected approaches preserved**: What *didn't* work is as valuable as what did
- **Code-decision traceability**: Every commit linked to its reasoning
- **Survives context loss**: The graph outlives any session

### The Live Graph

**[Browse it here](https://notactuallytreyanastasio.github.io/losselot/demo/)** - 75+ nodes, chains grouped by topic, confidence badges, commit links.

</details>

---

<details>
<summary><b>Claude Integration & CLAUDE.md</b></summary>

### The CLAUDE.md Contract

The project includes a `CLAUDE.md` file that tells Claude:

1. **Always start by reading the decision graph** - recover context
2. **Log everything** - observations, decisions, actions, outcomes
3. **Link commits to decisions** - full traceability
4. **Sync the graph before deploying** - keep the live site current

### Makefile Shortcuts

```bash
make obs T="Found interesting pattern" C=80     # Observation
make decision T="Choose approach" C=70          # Decision point
make action T="Implemented fix" C=95            # Implementation
make outcome T="Tests pass" C=90                # Result
make link FROM=1 TO=2 REASON="because"          # Connect nodes
make sync-graph                                 # Export to live site
```

### TypeScript Types

The frontend has TypeScript types mirroring the Rust backend:

- `DecisionNode`, `DecisionEdge`, `GraphData` (graph.ts)
- `AnalysisResult`, `BinaryAnalysis`, `SpectralAnalysis` (analysis.ts)

Tests run in CI alongside Rust tests.

### The Living Museum Concept

This isn't documentation written after the fact. It's a real-time record of how software gets built - captured as decisions happen, not reconstructed from memory later.

</details>

---

<details>
<summary><b>Command Line Reference</b></summary>

```bash
# Analyze files
./losselot ~/Music/                    # Folder
./losselot file.flac                   # Single file
./losselot -o report.html ~/Music/     # Custom output

# Web UI
./losselot serve ~/Music/ --port 3000

# Quick scan (no spectral)
./losselot --no-spectral ~/Music/

# Decision graph
./losselot db nodes                    # List nodes
./losselot db edges                    # List edges
./losselot db graph                    # Full JSON
./losselot db add-node -t TYPE "Title" [-c CONF] [--commit HASH]
./losselot db add-edge FROM TO [-t TYPE] [-r REASON]
```

**Exit codes:** 0=clean, 1=suspect, 2=transcode

</details>

---

<details>
<summary><b>Build from Source</b></summary>

```bash
# Requires Rust (rustup.rs)
git clone https://github.com/notactuallytreyanastasio/losselot.git
cd losselot
cargo build --release
```

### Run Tests

```bash
cargo test                    # Rust tests
cd docs && npm test           # TypeScript tests
```

### Generate Test Files

```bash
cd examples && ./generate_test_files.sh
```

Creates files demonstrating:
- Clean 320kbps and V0 VBR encodes
- 128kbps → 320kbps transcodes
- Multiple re-encoding passes
- Mixed encoder chains

</details>

---

## The Story

Losselot started as "does this FLAC actually contain lossless audio?" and evolved into "can we build software in a way where every decision is preserved, queryable, and linked to code?"

The audio forensics works great. But the real experiment is whether decision graphs can solve the "why did we do it this way?" problem that plagues every long-running project.

**[See for yourself](https://notactuallytreyanastasio.github.io/losselot/)**

---

## License

MIT - Do whatever you want with it.
