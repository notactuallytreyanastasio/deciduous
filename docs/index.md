---
layout: default
title: Home
---

# Losselot

**Find out if your "lossless" audio files are actually lossless.**

Ever downloaded a FLAC or WAV and wondered if it's the real deal, or just an MP3 someone converted? Losselot tells you the truth in seconds.

---

## Quick Start (30 seconds)

```bash
git clone https://github.com/notactuallytreyanastasio/losselot.git
cd losselot
cargo build --release
./target/release/losselot serve examples/ --port 3000
```

Open [http://localhost:3000](http://localhost:3000) and explore the interactive UI.

---

## What Makes This Project Unique

Losselot is both a powerful audio forensics tool AND a living experiment in AI-assisted development. There are four things worth exploring here:

<div class="pillars">

### 1. Audio Forensics
Detect fake lossless files using dual analysis: binary metadata inspection and FFT-based spectral analysis. Identify transcodes, re-encodes, and laundered audio.

[Explore Audio Analysis →](audio-analysis)

### 2. Decision Graph
A persistent knowledge graph that tracks WHY decisions were made. Every algorithm choice, every rejected approach, documented and visualized.

[Explore Decision Graph →](decision-graph)

### 3. Claude Tooling
Slash commands, context recovery, and session continuity tools that let Claude maintain understanding across long-running development.

[Explore Claude Tooling →](claude-tooling)

### 4. Development Story
This project is being built in public with AI assistance. See how decisions evolved, what didn't work, and what we learned.

[Read the Story →](story)

</div>

---

## Try the Demo

We've included a vendored analysis of sample files so you can explore without building anything:

<div id="demo-embed">
  <a href="demo/" class="demo-button">Launch Interactive Demo →</a>
</div>

The demo includes:
- Pre-analyzed audio files (clean, transcode, re-encoded)
- Live decision graph with real development history
- Example spectrograms and detection explanations

---

## Downloads

| Platform | Download |
|----------|----------|
| **Mac (Apple Silicon)** | [Download](https://github.com/notactuallytreyanastasio/losselot/releases/latest/download/losselot-darwin-arm64) |
| **Mac (Intel)** | [Download](https://github.com/notactuallytreyanastasio/losselot/releases/latest/download/losselot-darwin-amd64) |
| **Windows** | [Download](https://github.com/notactuallytreyanastasio/losselot/releases/latest/download/losselot-windows-amd64.exe) |
| **Linux** | [Download](https://github.com/notactuallytreyanastasio/losselot/releases/latest/download/losselot-linux-amd64) |

---

## How Detection Works

Lossy codecs like MP3 remove high frequencies to save space. This creates permanent "scars":

| Source Quality | Typical Cutoff |
|----------------|----------------|
| 128kbps MP3 | ~16kHz |
| 192kbps MP3 | ~18kHz |
| 320kbps MP3 | ~20kHz |
| True Lossless | ~22kHz (full range) |

Converting an MP3 to FLAC doesn't bring back lost frequencies. Losselot finds the scars.

**But it's not that simple.** Tape recordings and lo-fi productions also have high-frequency rolloff - but for different reasons. That's why we developed [CFCC (Cross-Frequency Coherence)](audio-analysis#cfcc) to distinguish digital brick-walls from natural analog rolloff.

---

## The Bigger Picture

This project demonstrates a workflow where:

1. **Decisions are documented as they happen** - not after the fact
2. **Context survives session boundaries** - through persistent graphs
3. **AI assists without taking over** - the human makes the calls
4. **The process is visible** - not just the result

Whether you're here for audio forensics or development methodology, welcome to the museum.
