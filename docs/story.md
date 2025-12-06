---
layout: default
title: Development Story
---

# Development Story: Building in Public with AI

This project is being developed in public, with AI assistance, while documenting the process as it happens.

---

## The Cast

**Human**: The one asking questions, making final decisions, and providing domain expertise about audio.

**Claude**: AI assistant handling code generation, research, and documentation. Working within explicit constraints and using external memory systems.

---

## Timeline

### Phase 1: Core Detection

The project started as a simple transcode detector:
- Binary analysis of MP3 headers
- Basic spectral analysis with FFT
- Terminal-based output

**Key decisions:**
- Use Rust for performance and safety
- Symphonia for audio decoding (handles many formats)
- RustFFT for spectral analysis

### Phase 2: Web UI

Terminal output wasn't enough. We needed visualization:
- D3.js-based interactive UI
- Spectrograms, frequency response curves
- File comparison and batch analysis

**Key decisions:**
- Embed HTML directly in binary (no separate server)
- Real-time analysis via WebSocket-like polling
- React-style state management in vanilla JS

### Phase 3: Lo-Fi Detection (CFCC)

A file called `charlie.flac` was incorrectly flagged as a transcode. It was actually a legitimate lo-fi recording.

**The problem:** How to distinguish MP3 brick-wall cutoff from natural tape rolloff?

**The solution:** Cross-Frequency Coherence Coefficient (CFCC)
- Measure correlation between adjacent frequency bands
- MP3 has sudden decorrelation at cliff
- Tape has gradual decorrelation following dynamics

This decision is documented in the graph: nodes 1-8.

### Phase 4: Decision Graph

We realized decisions were getting lost between sessions. The CFCC decision involved:
- Multiple approaches considered
- Technical tradeoffs evaluated
- Test results analyzed

All of this lived only in chat history that would eventually be lost.

**The solution:** SQLite-backed decision graph
- Persistent across sessions
- Queryable via CLI
- Visualizable in web UI

### Phase 5: Claude Tooling

With the decision graph in place, we built tooling around it:
- `/context` command for session start
- `/decision` command for graph management
- git.log for operation auditing
- CLAUDE.md with explicit rules

### Phase 6: This Site

The project had become more than an audio tool. It was also:
- A methodology for AI-assisted development
- A case study in persistent context
- A living documentation system

Hence: the "living museum" you're reading now.

---

## What Didn't Work

### Approach A: Temporal Cutoff Variance

Before CFCC, we considered measuring how cutoff frequency varies over time:
- MP3: fixed cutoff across all windows
- Tape: cutoff varies with dynamics

**Why we rejected it:**
- Required per-window cutoff detection
- Added significant complexity
- CFCC achieved same goal more simply

This is documented in the graph as node 3 (rejected via edge 7).

### Early UI Approaches

The first web UI was a single massive HTML file. Problems:
- 2800+ lines of embedded JavaScript
- Difficult to maintain
- No component reuse

**What we learned:**
- Embedded HTML is convenient but doesn't scale
- May need to split out CSS/JS eventually
- Graph node 9 captures this observation

### Context Loss

Multiple times, sessions ended with important context lost:
- Decisions made but not recorded
- Rationale forgotten
- Work repeated

**What we learned:**
- External state > internal memory
- Query before starting work
- Document decisions as they happen

---

## The Meta Experiment

This project is simultaneously:

1. **A useful tool** - Losselot actually detects fake lossless audio
2. **A development methodology** - The decision graph approach works
3. **A documentation style** - Living docs that update with the code
4. **An AI collaboration model** - Human decides, AI executes within constraints

### What Makes It Work

**Clear constraints:**
- Git rules prevent destructive operations
- Decision graph enforces documentation
- Explicit staging prevents accidents

**External memory:**
- Decisions in SQLite
- Operations in git.log
- Instructions in CLAUDE.md

**Session continuity:**
- `/context` command at session start
- Graph queries for current state
- Recent commits for recent work

### What's Still Hard

**Context window limits:**
- Large files still strain context
- May need smarter chunking
- Decision graph helps but doesn't solve completely

**Knowing when to document:**
- Not every thought needs a node
- Finding the right granularity
- Avoiding graph clutter

**Maintaining the system:**
- The tooling itself needs maintenance
- Meta-documentation can become stale
- Graph needs occasional pruning

---

## Current State

As of writing this page:

```
Nodes: 16
Edges: 15
Pending decisions: Multiple site structure choices
Recent work: GitHub Pages setup
```

The graph shows we're in the middle of building this very documentation site. Node 13 (goal), node 14 (decision), node 15 (structure option).

---

## What's Next

Ideas in the pipeline (not yet in the graph):

- **Mixed-source detection** - The "pillars" problem we discussed
- **Automated graph updates** - Hook into git commits
- **Interactive graph explorer** - More than just visualization
- **Session transcripts** - Archive full sessions

Whether these happen depends on what's useful. The graph will document the decisions either way.

---

## Try the Workflow

If you want to try this approach in your own project:

1. **Set up a decision graph** - SQLite is simple enough
2. **Create context recovery** - Whatever helps you resume
3. **Document as you go** - Not after the fact
4. **Constrain the AI** - Explicit rules, external state
5. **Build in public** - Accountability helps

The specific tools matter less than the principles:
- Decisions should survive session boundaries
- Context should be queryable, not remembered
- Process should be visible, not just results

---

[← Claude Tooling](claude-tooling) | [Back to Home →](/)
