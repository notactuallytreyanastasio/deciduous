# Losselot - Audio Forensics Tool

Losselot detects fake "lossless" audio files—files claiming to be lossless (FLAC, WAV, AIFF) but actually created from lossy sources (MP3, AAC). It uses dual analysis: binary metadata inspection and FFT-based spectral analysis.

---

## ⚠️ MANDATORY: Decision Graph Workflow

**THIS IS NOT OPTIONAL. The decision graph is watched live by the user. Every step must be logged IN REAL-TIME, not retroactively.**

### The Core Rule

```
BEFORE you do something → Log what you're ABOUT to do
AFTER it succeeds/fails → Log the outcome
ALWAYS → Sync frequently so the live graph updates
```

### Behavioral Triggers - MUST LOG WHEN:

| Trigger | Log Type | Example |
|---------|----------|---------|
| User asks for a new feature | `goal` | "Add dark mode to UI" |
| You're choosing between approaches | `decision` | "Choose state management approach" |
| You identify multiple ways to do something | `option` (for each) | "Option A: Redux", "Option B: Context" |
| You're about to write/edit code | `action` | "Implementing Redux store" |
| You notice something interesting | `observation` | "Existing code uses hooks pattern" |
| Something worked or failed | `outcome` | "Redux integration successful" |
| You complete a git commit | `action` with `--commit` | Include the commit hash |

### The Loop - Follow This EVERY Time

```
1. USER REQUEST RECEIVED
   ↓
   Log: goal or decision (what are we trying to do?)

2. BEFORE WRITING ANY CODE
   ↓
   Log: action "About to implement X"

3. AFTER EACH SIGNIFICANT CHANGE
   ↓
   Log: outcome "X completed" or observation "Found Y"
   Link: Connect to related nodes

4. BEFORE EVERY GIT PUSH
   ↓
   Run: make sync-graph
   Commit: Include graph-data.json

5. REPEAT - The user is watching the graph live
```

### Quick Commands

```bash
# Log nodes (use --confidence 0-100, --commit HASH when applicable)
./target/release/losselot db add-node -t goal "Title" --confidence 90
./target/release/losselot db add-node -t decision "Title" --confidence 75
./target/release/losselot db add-node -t action "Title" --confidence 85 --commit abc123
./target/release/losselot db add-node -t observation "Title" --confidence 70
./target/release/losselot db add-node -t outcome "Title" --confidence 95

# Link nodes
./target/release/losselot db add-edge FROM_ID TO_ID -r "Reason for connection"

# Sync to live site (DO THIS FREQUENTLY)
make sync-graph

# Makefile shortcuts
make goal T="Title" C=90
make decision T="Title" C=75
make action T="Title" C=85
make obs T="Title" C=70
make outcome T="Title" C=95
make link FROM=1 TO=2 REASON="why"
```

### Confidence Levels

- **90-100**: Certain, proven, tested
- **70-89**: High confidence, standard approach
- **50-69**: Moderate confidence, some unknowns
- **30-49**: Experimental, might change
- **0-29**: Speculative, likely to revisit

### Why This Matters

1. **The user watches the graph live** - They see your reasoning as you work
2. **Context WILL be lost** - The graph survives compaction, you don't
3. **Retroactive logging misses details** - Log in the moment or lose nuance
4. **Future sessions need this** - Your future self (or another session) will query this
5. **Public accountability** - The graph is published at the live URL

**Live graph**: https://notactuallytreyanastasio.github.io/losselot/demo/

---

## Session Start Checklist

Every new session or after context recovery, run `/context` or:

```bash
./target/release/losselot db nodes    # What decisions exist?
./target/release/losselot db edges    # How are they connected?
./target/release/losselot db commands # What happened recently?
git log --oneline -10                 # Recent commits
git status                            # Current state
```

---

## Quick Reference

```bash
# Build
cargo build --release

# Run tests
cargo test

# Analyze a file
cargo run -- path/to/file.flac

# Analyze a directory
cargo run -- ~/Music/

# Interactive web UI
cargo run -- serve ~/Music/ --port 3000

# Skip spectral analysis (faster)
cargo run -- --no-spectral path/to/file.flac

# Generate test files (requires ffmpeg, lame, sox)
./examples/generate_test_files.sh
```

## Architecture

```
src/
├── main.rs              # CLI entry, argument parsing, parallel execution
├── lib.rs               # Public API exports
├── serve.rs             # HTTP server for web UI
├── ui.html              # Embedded web UI (D3.js visualizations)
├── analyzer/
│   ├── mod.rs           # Core orchestration, score combination
│   ├── spectral.rs      # FFT frequency analysis (8192-sample windows)
│   └── binary.rs        # MP3 metadata, LAME headers, encoder signatures
├── mp3/
│   ├── mod.rs           # MP3 module exports
│   ├── frame.rs         # MP3 frame header parsing
│   └── lame.rs          # LAME/Xing header extraction
└── report/
    ├── mod.rs           # Report dispatcher
    ├── html.rs          # Interactive HTML report
    ├── json.rs          # JSON output
    └── csv.rs           # CSV output
```

## Scoring System

| Component | Max Points | Key Indicators |
|-----------|------------|----------------|
| Binary    | ~50        | Lowpass mismatch, multiple encoder signatures, frame variance |
| Spectral  | ~50        | High-frequency drops, missing ultrasonic, steep rolloff |
| Agreement | +15        | Bonus when both methods agree on transcode |

**Verdicts:**
- `OK` (0-34): Clean file
- `SUSPECT` (35-64): Possibly transcoded
- `TRANSCODE` (65-100): Definitely transcoded

## Key Detection Flags

**Spectral:** `severe_hf_damage`, `hf_cutoff_detected`, `weak_ultrasonic_content`, `dead_ultrasonic_band`, `silent_17k+`, `steep_hf_rolloff`

**Re-encoding:** `multi_encoder_sigs`, `encoding_chain(LAME → FFmpeg)`, `lame_reencoded_x2`, `ffmpeg_processed_x2`

**Binary:** `lowpass_bitrate_mismatch`, `encoder_quality_mismatch`

## Testing

### Rust Tests
```bash
cargo test
cargo test -- --nocapture
cargo test test_threshold_boundaries
```

### TypeScript/Frontend Tests
```bash
cd docs && npm run test
cd docs && npm run typecheck
```

## Database Rules

**CRITICAL: NEVER delete the SQLite database (`losselot.db`)**

The database contains the decision graph. If you need to clear data:
1. `losselot db backup` first
2. Ask the user before any destructive operation

```bash
losselot db nodes      # List decision nodes
losselot db edges      # List edges
losselot db graph      # Full graph as JSON
losselot db add-node   # Add a decision node
losselot db add-edge   # Add an edge between nodes
losselot db status     # Update node status
losselot db commands   # Show recent command log
losselot db backup     # Create timestamped backup
```
