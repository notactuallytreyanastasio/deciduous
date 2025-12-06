---
layout: default
title: Decision Graph
---

# Decision Graph: A Living Record

The decision graph is not documentation written after the fact. It's a real-time record of how this project evolved - captured as decisions were made.

---

## What You're Looking At

<div class="graph-demo">
  <img src="../knowledge_graph.png" alt="Decision Graph Overview" style="max-width: 100%; border-radius: 8px; box-shadow: 0 4px 12px rgba(0,0,0,0.3);">
</div>

This is the actual graph stored in `losselot.db`. Every node represents a real decision point, observation, or action taken during development.

---

## Node Types

| Type | Color | Purpose |
|------|-------|---------|
| **Goal** | 🟢 Green | High-level objectives we're trying to achieve |
| **Decision** | 🟡 Yellow | Choice points with multiple possible approaches |
| **Option** | 🔵 Cyan | Possible approaches we considered |
| **Action** | 🔴 Red | Things we actually implemented (with commit refs) |
| **Outcome** | 🟣 Purple | Results of actions - what worked, what didn't |
| **Observation** | ⚪ Gray | Data points, findings, technical insights |

---

## Edge Types

- **leads_to** (gray) → Natural progression
- **chosen** (green) → We picked this option
- **rejected** (red dashed) → We didn't pick this, and here's why
- **requires** → Dependency
- **blocks** / **enables** → Impediments or enablers

---

## A Real Example: Lo-Fi Detection

Let's walk through an actual decision from this project.

### The Problem

We had a file called `charlie.flac` that was flagged as a transcode - but it was actually a legitimate lo-fi recording. The high-frequency rolloff was natural, not from MP3 compression.

<div class="node-detail">
  <img src="../goal.png" alt="Goal Node" style="max-width: 400px;">
  <p><strong>Goal:</strong> Test lo-fi detection on charlie.flac</p>
</div>

### The Decision Point

How do we distinguish MP3 brick-wall cutoff from natural tape/lo-fi rolloff?

<div class="node-detail">
  <img src="../decision.png" alt="Decision Node" style="max-width: 500px;">
  <p>Click any node to see its description and status</p>
</div>

### The Options

**Option A: Temporal Cutoff Variance**
- Measure how cutoff frequency varies over time
- MP3 = fixed cutoff, Tape = varies with dynamics
- *Rejected: More complex, requires per-window tracking*

**Option B: Cross-Frequency Coherence (CFCC)**
- Measure correlation between adjacent frequency bands
- MP3 = sudden decorrelation at cliff
- Tape = gradual decorrelation following the music
- *Chosen: Works with existing FFT structure*

### The Implementation

<div class="node-detail">
  <img src="../action.png" alt="Action Node" style="max-width: 500px;">
  <p>Actions include commit references for traceability</p>
</div>

### The Outcome

- CFCC passes 157 tests
- Detects 25/29 transcodes correctly
- Charlie.flac now correctly identified as lo-fi, not transcode

---

## Why This Matters

### Traditional Documentation

```
CHANGELOG.md:
- Added CFCC detection for lo-fi files
```

This tells you *what* changed but not *why* or *what else was considered*.

### Decision Graph

The graph preserves:
- **The problem we were solving** (Goal)
- **Options we considered** (including rejected ones)
- **Why we chose what we chose** (edge rationale)
- **What happened** (Outcome)

Six months from now, when someone asks "why didn't you use temporal variance?", the answer is in the graph.

---

## Interact With the Graph

The decision graph is queryable from the command line:

```bash
# List all nodes
./losselot db nodes

# Show edges (relationships)
./losselot db edges

# Full graph as JSON
./losselot db graph

# Add your own observations
./losselot db add-node -t observation "Your finding here"
```

Or view it in the web UI:

```bash
./losselot serve . --port 3000
# Open http://localhost:3000/graph
```

---

## The Meta Layer

Here's where it gets interesting. The decision to *build this decision graph system* is itself documented in the graph:

- **Goal 13**: Create GitHub Pages living museum site
- **Decision 14**: Site structure and content organization
- **Observation 16**: Four pillars of value (audio, decisions, Claude tooling, story)

The system documents itself. When you're reading this page, you're reading the output of decisions that are tracked in the very system being described.

---

## Current Graph State

<div id="live-graph-data">
  <p><em>The following is a snapshot of the actual graph data:</em></p>
</div>

```
ID    TYPE         STATUS     TITLE
------------------------------------------------------------
1     goal         pending    Test lo-fi detection on charlie.flac
2     decision     pending    Lo-fi detection approach
3     option       pending    Approach A: Temporal Cutoff Variance
4     option       pending    Approach B: Cross-Frequency Coherence (CFCC)
5     observation  pending    Approach A requires per-window cutoff detection
6     observation  pending    Approach B detects cliff via decorrelation
7     action       pending    Implemented CFCC in commit aa464b6
8     outcome      pending    CFCC passes 157 tests, detects 25/29 transcodes
9     observation  pending    Code organization: consider splitting large files
10    observation  pending    MP3 vs Tape: brick-wall vs gradual rolloff
11    observation  pending    CFCC scoring thresholds
12    observation  pending    Known codec cutoff frequencies
13    goal         pending    Create GitHub Pages living museum site
14    decision     pending    Site structure and content organization
15    option       pending    Section-based site
16    observation  pending    Four pillars of value
```

This isn't fake data for the docs. This is the real state of development as you read this.

---

[← Back to Home](/) | [Next: Claude Tooling →](claude-tooling)
