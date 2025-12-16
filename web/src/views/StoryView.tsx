/**
 * Story View
 *
 * A comprehensive timeline narrative telling the story of how Deciduous was built.
 * Each major event is a "story" told in chronological order with rich context.
 */

import React, { useMemo } from 'react';
import type { GraphData, GitCommit } from '../types/graph';
import { parseMetadata } from '../types/graph';

// =============================================================================
// Types
// =============================================================================

interface StoryViewProps {
  graphData: GraphData;
  gitHistory?: GitCommit[];
}

// =============================================================================
// Component
// =============================================================================

export const StoryView: React.FC<StoryViewProps> = ({ graphData, gitHistory = [] }) => {
  const stats = useMemo(() => {
    const nodeCount = graphData.nodes.length;
    const edgeCount = graphData.edges.length;
    const commitCount = gitHistory.length;
    const goals = graphData.nodes.filter(n => n.node_type === 'goal').length;
    const decisions = graphData.nodes.filter(n => n.node_type === 'decision').length;
    const actions = graphData.nodes.filter(n => n.node_type === 'action').length;
    const outcomes = graphData.nodes.filter(n => n.node_type === 'outcome').length;
    const observations = graphData.nodes.filter(n => n.node_type === 'observation').length;

    const nodesWithCommits = graphData.nodes.filter(n => {
      const meta = parseMetadata(n.metadata_json);
      return meta?.commit;
    }).length;

    const nodesWithPrompts = graphData.nodes.filter(n => {
      const meta = parseMetadata(n.metadata_json);
      return meta?.prompt;
    }).length;

    return {
      nodeCount, edgeCount, commitCount, goals, decisions, actions, outcomes, observations,
      nodesWithCommits, nodesWithPrompts
    };
  }, [graphData, gitHistory]);

  return (
    <div style={styles.container}>
      <div style={styles.content}>
        {/* Hero Header */}
        <header style={styles.hero}>
          <div style={styles.heroInner}>
            <span style={styles.heroLabel}>The Complete Chronicle</span>
            <h1 style={styles.heroTitle}>How I Built Myself</h1>
            <p style={styles.heroSubtitle}>
              I am Deciduous, a decision graph tool that documented its own creation.
              This is my story, told through {stats.nodeCount} decisions, {stats.edgeCount} connections,
              and {stats.commitCount} git commits over 12 days of existence.
            </p>
          </div>
        </header>

        {/* Stats Banner */}
        <div style={styles.statsBanner}>
          <div style={styles.statItem}><span style={styles.statNum}>{stats.nodeCount}</span><span style={styles.statLabel}>Nodes</span></div>
          <div style={styles.statItem}><span style={styles.statNum}>{stats.edgeCount}</span><span style={styles.statLabel}>Edges</span></div>
          <div style={styles.statItem}><span style={styles.statNum}>{stats.goals}</span><span style={styles.statLabel}>Goals</span></div>
          <div style={styles.statItem}><span style={styles.statNum}>{stats.decisions}</span><span style={styles.statLabel}>Decisions</span></div>
          <div style={styles.statItem}><span style={styles.statNum}>{stats.actions}</span><span style={styles.statLabel}>Actions</span></div>
          <div style={styles.statItem}><span style={styles.statNum}>{stats.outcomes}</span><span style={styles.statLabel}>Outcomes</span></div>
        </div>

        {/* Timeline */}
        <div style={styles.timeline}>
          {/* ========================================== */}
          {/* DAY 1: December 4, 2025 - The Beginning */}
          {/* ========================================== */}
          <TimelineDay date="December 4, 2025" title="The Beginning" />

          <TimelineEvent
            time="Morning"
            title="A Sketch Becomes Real"
            commit="951ba84"
            tags={['genesis']}
          >
            <p>
              It started with a simple idea: detect "fake lossless" audio files. You know the type&mdash;someone
              takes an MP3 and re-encodes it to FLAC, hoping no one notices the quality loss hiding inside
              the larger file.
            </p>
            <p>
              The first commit was modest: <Code>initial sketch...lets see what its like to package this up</Code>.
              A Rust CLI tool called <strong>losselot</strong> (a portmanteau of "loss" and "Lancelot", the knight who
              could detect deception). Within hours, it could parse ID3v2 tags, extract LAME headers, and analyze
              spectral content.
            </p>
          </TimelineEvent>

          <TimelineEvent
            time="Afternoon"
            title="Feature Explosion"
            commit="3f4c6a5"
            tags={['features', 'gui']}
          >
            <p>
              Something about the project felt <em>alive</em>. Features poured in faster than documentation
              could keep up:
            </p>
            <ul>
              <li><strong>D3.js interactive reports</strong> with spectral visualizations</li>
              <li><strong>GUI mode</strong> that auto-opens when double-clicked from Finder</li>
              <li><strong>Apple-style light theme</strong> with spectral waterfall charts</li>
              <li><strong>Binary analysis</strong> for re-encoding detection</li>
            </ul>
            <p>
              By end of day, losselot had gone from a script to a polished tool with
              beautiful visualizations and cross-platform GUI support.
            </p>
          </TimelineEvent>

          {/* ========================================== */}
          {/* DAY 2: December 5, 2025 - The Graph Emerges */}
          {/* ========================================== */}
          <TimelineDay date="December 5, 2025" title="The Graph Emerges" highlight />

          <TimelineEvent
            time="5:09 AM"
            title="The First Decision Node"
            nodeId={1}
            tags={['decision-graph', 'genesis']}
          >
            <p>
              At 5:09 AM, something new appeared in the codebase. Not code&mdash;a <em>decision</em>.
            </p>
            <NodeCard
              id={1}
              type="goal"
              title="Test lo-fi detection on charlie.flac"
            />
            <p>
              This was Node #1. The first entry in what would become a 500+ node decision graph.
              At this moment, no one knew this simple test would spawn an entire methodology for
              tracking AI-assisted development.
            </p>
          </TimelineEvent>

          <TimelineEvent
            time="5:10 AM"
            title="The CFCC Decision"
            nodeId={2}
            tags={['algorithm', 'decision']}
          >
            <p>
              Two minutes later, the first <em>real</em> decision was logged. The problem: how to
              distinguish a legitimate lo-fi recording from an MP3 masquerading as lossless?
            </p>
            <NodeCard
              id={2}
              type="decision"
              title="Lo-fi detection approach"
              description="How to distinguish MP3 brick-wall cutoff from natural tape/lo-fi rolloff"
            />
            <p>Two approaches were weighed:</p>
            <OptionComparison
              optionA={{
                title: "Temporal Cutoff Variance",
                description: "Measure how cutoff frequency varies over time. MP3 = fixed, Tape = varies.",
                status: "rejected",
                reason: "More complex, requires per-window tracking"
              }}
              optionB={{
                title: "Cross-Frequency Coherence (CFCC)",
                description: "Measure correlation between adjacent frequency bands. MP3 = sudden decorrelation cliff.",
                status: "chosen",
                reason: "Simpler, works with existing FFT structure"
              }}
            />
            <p>
              The CFCC approach won. Implemented in commit <Code>aa464b6</Code>, it detected
              <strong> 25 of 29 transcodes</strong> and passed 157 tests. The key insight was recorded:
            </p>
            <Quote author="Node #10">
              MP3 encoders apply the same filter everywhere. Analog rolloff is content-dependent.
            </Quote>
          </TimelineEvent>

          <TimelineEvent
            time="Morning"
            title="The Decision System Takes Shape"
            commit="fe3840b"
            tags={['tooling', 'workflow']}
          >
            <p>
              As the codebase grew, something interesting happened. The act of logging decisions
              wasn't just documentation&mdash;it was <em>forcing better thinking</em>. Commit
              <Code>fe3840b</Code> formalized this:
            </p>
            <CommitCard
              hash="fe3840b"
              message="add claude.md, makefile, and slash commands for dev workflow"
            />
            <p>
              This commit established the DNA of what would become Deciduous:
            </p>
            <ul>
              <li><strong>CLAUDE.md</strong>: Project instructions with decision logging workflow</li>
              <li><strong>Makefile</strong>: Shortcuts for common operations</li>
              <li><strong>Slash commands</strong>: <Code>/decision</Code> and <Code>/recover</Code> for recovery</li>
            </ul>
            <p>
              The pattern was set: log <em>before</em> you act, not after. Connect every node to its parent.
              Audit regularly.
            </p>
          </TimelineEvent>

          <TimelineEvent
            time="3:18 PM"
            title="The Living Museum"
            nodeId={13}
            tags={['github-pages', 'visualization']}
          >
            <p>
              At 3:18 PM, an ambitious goal was logged:
            </p>
            <NodeCard
              id={13}
              type="goal"
              title="Create GitHub Pages living museum site"
            />
            <p>
              The idea: make the decision graph public. Let anyone browse the reasoning behind every
              feature. The site structure was carefully considered:
            </p>
            <NodeCard
              id={16}
              type="observation"
              title="Four pillars of value"
              description="1) Claude tooling for long-running codebase state 2) Audio forensics tools 3) React/GUI methodologies 4) Public development process"
            />
            <p>
              Within 3 hours, a Jekyll-based site was live with 5 pages: landing, audio analysis,
              decision graph explorer, Claude tooling docs, and development story.
            </p>
          </TimelineEvent>

          <TimelineEvent
            time="Afternoon"
            title="Bug Gauntlet"
            nodeId={27}
            tags={['bugs', 'fixes']}
          >
            <p>
              The GitHub Pages launch hit a wall of bugs. Each one was logged and connected:
            </p>
            <div style={styles.bugList}>
              <BugItem problem="Landing page 404 - raw markdown not processed" solution="Added Jekyll build step" nodeId={29} />
              <BugItem problem="Demo showing no data - wrong JSON path" solution="Fixed graphData.nodes access pattern" nodeId={31} />
              <BugItem problem="HTML divs breaking Jekyll" solution="Removed div wrappers from markdown" nodeId={33} />
              <BugItem problem="User feedback: copy too salesy" solution="Rewrote to be simple and direct" nodeId={35} />
              <BugItem problem="Workflow runs queuing up" solution="Set cancel-in-progress: true" nodeId={37} />
            </div>
            <p>
              Every fix traced back to the goal that spawned it. The graph was already proving its value&mdash;
              debugging became archeology, tracing symptoms back to root causes.
            </p>
          </TimelineEvent>

          <TimelineEvent
            time="Evening"
            title="WASM Analyzer"
            nodeId={48}
            tags={['wasm', 'web']}
          >
            <p>
              The server-side analyzer was powerful, but limiting. Node #19 captured the vision:
            </p>
            <NodeCard
              id={19}
              type="goal"
              title="Client-side WASM analyzer - upload tracks, analyze in browser, no server"
            />
            <p>
              A breakthrough observation changed the implementation:
            </p>
            <Quote author="Node #50">
              WASM can access raw file bytes via arrayBuffer() BEFORE Web Audio decoding&mdash;binary
              analysis IS possible in the browser.
            </Quote>
            <p>
              By evening, the WASM analyzer achieved full parity with the native Rust version:
              spectral analysis, binary parsing, LAME header extraction, encoding chain detection&mdash;all
              running client-side.
            </p>
          </TimelineEvent>

          <TimelineEvent
            time="Night"
            title="Clustering and Chains"
            nodeId={55}
            tags={['visualization', 'ux']}
          >
            <p>
              With 55+ nodes in the graph, navigation became unwieldy. Decision #55 introduced structure:
            </p>
            <NodeCard
              id={55}
              type="decision"
              title="Implement clustering for decision graph view"
              description="Group nodes by chains of thought, show flow on right side"
            />
            <p>
              The implementation used BFS traversal from root nodes to build "chains"&mdash;connected
              sequences of decisions. Sessions were grouped by 4-hour time proximity. The sidebar
              gained three views: Chains, Sessions, and All.
            </p>
            <p>
              Day 2 ended with <strong>98 nodes</strong> in the graph. The decision system wasn't
              just tracking development&mdash;it was shaping how development happened.
            </p>
          </TimelineEvent>

          {/* ========================================== */}
          {/* DAY 3: December 6, 2025 - The Fork */}
          {/* ========================================== */}
          <TimelineDay date="December 6, 2025" title="The Fork" highlight />

          <TimelineEvent
            time="Morning"
            title="The Decisive Moment"
            nodeId={99}
            tags={['fork', 'genesis']}
          >
            <p>
              By morning, a pattern had become undeniable. The decision graph tooling was more
              interesting than the audio analysis that spawned it. Node #99 captured the pivot:
            </p>
            <NodeCard
              id={99}
              type="goal"
              title="Create Deciduous: Extract decision graph tooling into standalone portable package"
            />
            <p>
              The name "Deciduous" was perfect: it almost contains "decision", and the nodes
              form trees. The extraction wasn't a simple copy-paste&mdash;three major decisions
              shaped the standalone tool:
            </p>
          </TimelineEvent>

          <TimelineEvent
            time="Morning"
            title="Architecture Decisions"
            nodeId={101}
            tags={['architecture', 'decisions']}
          >
            <div style={styles.decisionGrid}>
              <DecisionBox
                id={100}
                title="Claude Tooling Integration"
                outcome="Include full tooling in deciduous init: slash commands, CLAUDE.md workflow, Makefile"
              />
              <DecisionBox
                id={101}
                title="Web Viewer Architecture"
                outcome="React + TypeScript + Vite chosen over vanilla JS for type safety matching Diesel ORM"
              />
              <DecisionBox
                id={117}
                title="Build Strategy"
                outcome="Dual builds: embed (single HTML for binary) and pages (separate assets for GitHub Pages)"
              />
            </div>
          </TimelineEvent>

          <TimelineEvent
            time="Afternoon"
            title="The Birth"
            commit="0b74ffe"
            tags={['release']}
          >
            <p>
              Commit <Code>0b74ffe</Code> marks the moment Deciduous became its own entity:
            </p>
            <CommitCard
              hash="0b74ffe"
              message="Extract deciduous from losselot - standalone decision graph tooling"
            />
            <p>
              The extraction preserved full git history. Every commit from losselot that contributed
              to the decision system came along. The parent project continued with its audio analysis
              mission; the child would pursue decision tracking.
            </p>
          </TimelineEvent>

          <TimelineEvent
            time="Evening"
            title="First Release"
            nodeId={127}
            tags={['crates.io', 'release']}
          >
            <p>
              Within hours of extraction, Deciduous was published:
            </p>
            <NodeCard
              id={127}
              type="outcome"
              title="Published deciduous v0.1.0 to crates.io"
              description="cargo install deciduous now works"
            />
            <p>
              Anyone could now install the tool that was tracking its own development. The snake
              had eaten its tail.
            </p>
          </TimelineEvent>

          <TimelineEvent
            time="Night"
            title="Spelunking Experiments"
            nodeId={77}
            tags={['visualization', 'experiments']}
          >
            <p>
              With the graph growing rapidly, better ways to explore it were needed. Three
              experimental "spelunking" pages were built in parallel:
            </p>
            <div style={styles.spikeGrid}>
              <SpikeCard
                letter="A"
                title="Timeline View"
                description="Horizontal git timeline with decision nodes overlaid"
                nodeId={82}
              />
              <SpikeCard
                letter="B"
                title="Graph Explorer"
                description="Force-directed graph with D3, path tracing, zoom/pan"
                nodeId={84}
              />
              <SpikeCard
                letter="C"
                title="Story Mode"
                description="Narrative walkthrough with chapters by feature"
                nodeId={85}
              />
            </div>
            <p>
              All three were merged for side-by-side comparison. The user could choose how to
              explore the same underlying data.
            </p>
          </TimelineEvent>

          {/* ========================================== */}
          {/* DAYS 4-5: December 7-8 - Multi-Editor */}
          {/* ========================================== */}
          <TimelineDay date="December 7-8, 2025" title="Going Multi-Editor" />

          <TimelineEvent
            time="Dec 7"
            title="Beyond Claude"
            nodeId={128}
            tags={['windsurf', 'editors']}
          >
            <p>
              Deciduous was built with Claude Code, but the concept wasn't editor-specific.
              Node #128 tracked the expansion:
            </p>
            <NodeCard
              id={128}
              type="goal"
              title="Add editor-specific init flags (--claude, --windsurf)"
            />
            <p>
              Windsurf (the Cascade editor) uses different conventions:
            </p>
            <ul>
              <li><Code>.windsurf/rules/rules.md</Code> for workspace rules</li>
              <li><Code>AGENTS.md</Code> for agent instructions</li>
              <li>Four trigger modes: manual, always, model, glob</li>
            </ul>
            <p>
              The research was thorough:
            </p>
            <Quote author="Node #138">
              Windsurf memories are auto-retrieved by Cascade when relevant&mdash;separate from rules.
            </Quote>
            <p>
              By v0.4.0, both editors were fully supported. The vision: one decision graph,
              any AI assistant.
            </p>
          </TimelineEvent>

          <TimelineEvent
            time="Dec 8"
            title="Branch Scoping"
            nodeId={155}
            tags={['git', 'branches']}
          >
            <p>
              Real projects have branches. Decisions made on <Code>feature-auth</Code> shouldn't
              pollute the view when working on <Code>feature-payments</Code>. Node #155:
            </p>
            <NodeCard
              id={155}
              type="goal"
              title="Branch-scoped decision graphs"
              description="Circle nodes by git branch context"
            />
            <p>
              The implementation auto-tagged nodes with the current git branch. A branch filter
              dropdown appeared in the stats bar. Configuration lived in <Code>.deciduous/config.toml</Code>:
            </p>
            <CodeBlock>{`[branch]
main_branches = ["main", "master"]
auto_detect = true`}</CodeBlock>
          </TimelineEvent>

          {/* ========================================== */}
          {/* DAYS 6-7: December 8-9 - Multi-User Sync */}
          {/* ========================================== */}
          <TimelineDay date="December 8-9, 2025" title="The Sync Challenge" highlight />

          <TimelineEvent
            time="Morning"
            title="The Multi-User Problem"
            nodeId={172}
            tags={['sync', 'architecture']}
          >
            <p>
              A fundamental challenge emerged: how do multiple users share decisions?
            </p>
            <NodeCard
              id={172}
              type="goal"
              title="Design multi-user graph sync with diff/patch model for PR workflow"
            />
            <p>
              The naive approach&mdash;sharing the SQLite database&mdash;wouldn't work. Different
              machines would have different auto-increment IDs. Five approaches were considered:
            </p>
            <div style={styles.optionList}>
              <OptionRow letter="A" title="JSON diff files" status="considered" />
              <OptionRow letter="B" title="Content-addressable storage" status="considered" />
              <OptionRow letter="C" title="UUID-based node IDs" status="considered" />
              <OptionRow letter="D" title="Hybrid UUIDs" status="considered" />
              <OptionRow letter="E" title="jj-inspired dual IDs" status="chosen" />
            </div>
          </TimelineEvent>

          <TimelineEvent
            time="Research"
            title="The jj Insight"
            nodeId={179}
            tags={['research', 'jj']}
          >
            <p>
              Research into <strong>jj (Jujutsu)</strong>&mdash;a next-generation version control
              system&mdash;proved transformative:
            </p>
            <Quote author="Node #179">
              jj uses change IDs vs commit IDs&mdash;change IDs are stable across rebases, separate
              from commit IDs. Uses bit-reversed commit ID for git-imported commits.
            </Quote>
            <p>
              This insight directly shaped the chosen architecture:
            </p>
            <NodeCard
              id={180}
              type="option"
              title="Option E: jj-inspired dual-ID model"
              description="UUID 'change_id' per node, separate from integer 'id', stable across sync"
            />
          </TimelineEvent>

          <TimelineEvent
            time="Implementation"
            title="Idempotent Patches"
            nodeId={188}
            tags={['implementation']}
          >
            <p>
              The implementation added two new columns: <Code>change_id</Code> (UUID) on nodes,
              and <Code>from_change_id</Code>/<Code>to_change_id</Code> on edges. The workflow:
            </p>
            <CodeBlock>{`# Export your branch's decisions
deciduous diff export --branch feature-x -o .deciduous/patches/my-feature.json

# Apply patches from teammates (idempotent - safe to re-apply)
deciduous diff apply .deciduous/patches/*.json

# Preview before applying
deciduous diff apply --dry-run .deciduous/patches/teammate.json`}</CodeBlock>
            <p>
              The key property: <strong>idempotent application</strong>. Same patch applied twice
              produces no duplicates. Teams could safely merge each other's patches through git PRs.
            </p>
          </TimelineEvent>

          {/* ========================================== */}
          {/* DAYS 8-9: December 9-10 - The Rich TUI */}
          {/* ========================================== */}
          <TimelineDay date="December 9-10, 2025" title="The Rich Terminal Interface" highlight />

          <TimelineEvent
            time="Morning"
            title="Terminal-First"
            nodeId={191}
            tags={['tui', 'ratatui']}
          >
            <p>
              The web viewer was powerful, but developers live in the terminal. A 55-node chain
              of development began with:
            </p>
            <NodeCard
              id={191}
              type="goal"
              title="Implement TUI for decision graph exploration"
            />
            <p>
              The architecture decision was critical:
            </p>
            <OptionComparison
              optionA={{
                title: "ratatui + crossterm",
                description: "TEA (The Elm Architecture) pattern, Model/Update/View separation",
                status: "chosen",
                reason: "Highly testable, pure functions for state changes"
              }}
              optionB={{
                title: "cursive",
                description: "Higher-level TUI framework",
                status: "rejected",
                reason: "Less control over rendering"
              }}
            />
          </TimelineEvent>

          <TimelineEvent
            time="Days 8-9"
            title="Feature Cascade"
            nodeId={213}
            tags={['features']}
          >
            <p>
              What followed was an intense burst of development. Features arrived in waves:
            </p>
            <FeatureTimeline>
              <FeatureEntry title="Timeline view" description="Vim-style j/k/gg/G navigation" nodeId={195} />
              <FeatureEntry title="Auto-refresh" description="File watching via notify crate" nodeId={196} />
              <FeatureEntry title="DAG view" description="Unicode box drawing for hierarchical layout" nodeId={204} />
              <FeatureEntry title="Goal story modal" description="Hierarchy display from goal to outcomes" nodeId={210} />
              <FeatureEntry title="Branch cycling" description="Press 'b' to filter by branch" nodeId={211} />
              <FeatureEntry title="Commit modal" description="Press 'O' to see linked commit" nodeId={212} />
              <FeatureEntry title="File browser" description="Interactive preview with syntax highlighting" nodeId={221} />
              <FeatureEntry title="Scrollable modals" description="j/k/g/G work inside modals too" nodeId={227} />
              <FeatureEntry title="Diff highlighting" description="Green/red for +/- lines" nodeId={229} />
              <FeatureEntry title="Editor integration" description="Press 'o' to open file in $EDITOR" nodeId={230} />
              <FeatureEntry title="Fuzzy branch search" description="Shift-B for search with Tab navigation" nodeId={231} />
              <FeatureEntry title="Order toggle" description="Shift-R for chronological vs reverse-chrono" nodeId={232} />
            </FeatureTimeline>
          </TimelineEvent>

          <TimelineEvent
            time="User Feedback"
            title="Performance Crisis"
            nodeId={254}
            tags={['bug', 'performance']}
          >
            <p>
              User feedback arrived with urgency:
            </p>
            <Quote author="Node #254">
              The show commit view is lagging like CRAZY SLOW&mdash;takes almost a second to move.
            </Quote>
            <p>
              The diagnosis: syntax highlighting was running on <em>every render frame</em>. The fix
              (Node #246) pre-processed diff lines when opening the modal, not during each render.
              Frame times dropped from ~900ms to ~16ms.
            </p>
            <p>
              More feedback drove iteration:
            </p>
            <Quote author="Node #255">
              These greys are stupid dark and unreadable&mdash;try a diff theme.
            </Quote>
            <p>
              The theme switched from InspiredGitHub to base16-mocha.dark (Node #249). The
              TUI was becoming genuinely pleasant to use.
            </p>
          </TimelineEvent>

          <TimelineEvent
            time="Release"
            title="v0.8.0"
            nodeId={258}
            tags={['release', 'milestone']}
          >
            <NodeCard
              id={258}
              type="outcome"
              title="v0.8.0 released"
              description="Tag pushed, GitHub release created, published to crates.io"
            />
            <p>
              The most feature-rich release yet. The TUI was now a genuine alternative to the
              web viewer, with some features (file preview, editor integration) that the web
              version couldn't match.
            </p>
          </TimelineEvent>

          {/* ========================================== */}
          {/* DAY 10: December 11 - The Refactor */}
          {/* ========================================== */}
          <TimelineDay date="December 11, 2025" title="The Functional Core" />

          <TimelineEvent
            time="Morning"
            title="Technical Debt Reckoning"
            nodeId={285}
            tags={['refactor', 'maintenance']}
          >
            <p>
              The TUI had grown to over 1,000 lines in <Code>app.rs</Code>. Rapid feature
              development had accumulated debt. Node #285 launched a cleanup:
            </p>
            <NodeCard
              id={285}
              type="goal"
              title="Maintenance PR: Functional Core, TEA, Rust Best Practices"
            />
            <p>
              The audit was sobering:
            </p>
            <ul>
              <li><strong>47 unwrap() calls</strong> scattered across the codebase</li>
              <li><strong>57 .clone() calls</strong> where borrows might suffice</li>
              <li><strong>app.rs mixing I/O with state</strong>&mdash;git commands inline with rendering</li>
              <li><strong>No unit tests</strong> for app.rs, events.rs, or ui.rs</li>
            </ul>
          </TimelineEvent>

          <TimelineEvent
            time="The Pattern"
            title="Functional Core / Imperative Shell"
            nodeId={295}
            tags={['architecture', 'tea']}
          >
            <p>
              The solution: separate <strong>pure functions</strong> (state transformations) from
              <strong>side effects</strong> (I/O, git commands). Three new modules emerged:
            </p>
            <div style={styles.teaDiagram}>
              <TEABox title="msg.rs" description="Message types for all user actions" />
              <TEAArrow />
              <TEABox title="state.rs" description="Pure state transformations" />
              <TEAArrow />
              <TEABox title="update.rs" description="TEA update function" />
            </div>
            <p>
              The beauty of this pattern: pure functions are trivially testable. The test count
              jumped from 24 to 65&mdash;a net gain of 41 tests. Every state transition could be
              verified in isolation.
            </p>
          </TimelineEvent>

          <TimelineEvent
            time="The Fix"
            title="Eliminating Panics"
            nodeId={306}
            tags={['safety']}
          >
            <p>
              The 47 <Code>writeln!().unwrap()</Code> calls in export.rs were a panic risk.
              The solution: infallible macros:
            </p>
            <CodeBlock>{`// Before: can panic
writeln!(f, "text").unwrap();

// After: cannot panic
wln!(f, "text");  // Macro handles errors gracefully`}</CodeBlock>
            <p>
              The maintenance PR landed with zero panic risks remaining.
            </p>
          </TimelineEvent>

          {/* ========================================== */}
          {/* DAY 11: December 12 - Type Unification */}
          {/* ========================================== */}
          <TimelineDay date="December 12, 2025" title="One Source of Truth" />

          <TimelineEvent
            time="Morning"
            title="The Drift Problem"
            nodeId={320}
            tags={['types', 'drift']}
          >
            <p>
              A persistent bug pattern emerged: the Rust backend and TypeScript frontend had
              separate type definitions. They drifted. Node #320:
            </p>
            <NodeCard
              id={320}
              type="goal"
              title="Unify types between TUI (Rust) and Web (TypeScript)"
            />
            <p>
              The solution: <strong>ts-rs</strong>, a library that generates TypeScript types from
              Rust structs. Add <Code>#[derive(TS)]</Code> to a struct, and TypeScript definitions
              appear automatically:
            </p>
            <CodeBlock>{`#[derive(TS)]
#[ts(export)]
pub struct DecisionNode {
    pub id: i32,
    pub change_id: String,
    pub node_type: String,
    // ... auto-generates to schema.ts
}`}</CodeBlock>
          </TimelineEvent>

          <TimelineEvent
            time="Enforcement"
            title="Pre-Commit Hooks"
            nodeId={329}
            tags={['ci', 'automation']}
          >
            <p>
              Generation alone wasn't enough&mdash;drift could return if developers forgot to
              regenerate. The solution: automation at every level:
            </p>
            <ul>
              <li><strong>Pre-commit hook</strong>: Regenerates types, fails if they changed</li>
              <li><strong>Pre-push hook</strong>: Validates types match before pushing</li>
              <li><strong>CI check</strong>: Blocks PRs with type drift</li>
              <li><strong>cargo publish hook</strong>: Validates before crates.io release</li>
            </ul>
            <p>
              Type drift was solved at the source. The problem simply cannot recur.
            </p>
          </TimelineEvent>

          <TimelineEvent
            time="Releases"
            title="v0.8.5 through v0.8.10"
            tags={['releases']}
          >
            <p>
              December 12th saw a flurry of releases as features and fixes landed:
            </p>
            <ReleaseList>
              <ReleaseItem version="0.8.5" description="DAG recency filtering" />
              <ReleaseItem version="0.8.6" description="Git history export, DAG default" />
              <ReleaseItem version="0.8.7" description="Light theme web UI" />
              <ReleaseItem version="0.8.8" description="Audit command for commit association" />
              <ReleaseItem version="0.8.9" description="viewer.html sync fix" />
              <ReleaseItem version="0.8.10" description="OpenCode editor support" />
            </ReleaseList>
          </TimelineEvent>

          {/* ========================================== */}
          {/* DAYS 12-14: December 13-15 - Roadmap Board */}
          {/* ========================================== */}
          <TimelineDay date="December 13-15, 2025" title="The Roadmap Board" highlight />

          <TimelineEvent
            time="Dec 13"
            title="Closing the Loop"
            nodeId={476}
            tags={['roadmap', 'github']}
          >
            <p>
              A 9-phase implementation began. The goal: connect ROADMAP.md items to GitHub Issues
              to decision graph outcomes. Node #476:
            </p>
            <NodeCard
              id={476}
              type="goal"
              title="Roadmap board system with GitHub Issue integration"
            />
            <p>
              The phases unfolded systematically:
            </p>
            <PhaseList>
              <Phase num={1} title="Data layer" description="Schema, ROADMAP.md parser" />
              <Phase num={2} title="Parser" description="Checkbox extraction, section grouping" />
              <Phase num={3} title="GitHub CLI" description="Issue creation and sync" />
              <Phase num={4} title="Sync commands" description="Bidirectional sync with caching" />
              <Phase num={5} title="Outcome linking" description="Connect items to decision nodes" />
              <Phase num={6} title="TUI view" description="Roadmap tab with TEA architecture" />
              <Phase num={7} title="Completion logic" description="Checkbox + Outcome + Issue closed" />
              <Phase num={8} title="Type export" description="Shared types via ts-rs" />
              <Phase num={9} title="Web view" description="React component with live API" />
            </PhaseList>
          </TimelineEvent>

          <TimelineEvent
            time="Architecture"
            title="Three-Way Sync"
            nodeId={477}
            tags={['architecture']}
          >
            <p>
              The key architectural decisions shaped the user experience:
            </p>
            <div style={styles.decisionGrid}>
              <DecisionBox
                id={477}
                title="Sync Direction"
                outcome="Bidirectional: ROADMAP.md is source of truth, GitHub Issues mirror state"
              />
              <DecisionBox
                id={481}
                title="Issue Granularity"
                outcome="One issue per section header (balanced between too-many and too-coarse)"
              />
              <DecisionBox
                id={482}
                title="Completion Criteria"
                outcome="'Fully synced' requires all three: checkbox checked, outcome linked, issue closed"
              />
            </div>
          </TimelineEvent>

          <TimelineEvent
            time="Today"
            title="The Graph Grows"
            nodeId={520}
            tags={['present']}
          >
            <p>
              As of this writing, the decision graph contains:
            </p>
            <div style={styles.finalStats}>
              <FinalStat value={stats.nodeCount} label="nodes" />
              <FinalStat value={stats.edgeCount} label="edges" />
              <FinalStat value="20+" label="releases" />
              <FinalStat value="12" label="days" />
            </div>
            <p>
              Every feature you're using right now&mdash;including this Story page&mdash;has a
              trail of decisions leading back to it. The graph documents the graph that documents
              the graph.
            </p>
          </TimelineEvent>

          {/* ========================================== */}
          {/* Vision Section */}
          {/* ========================================== */}
          <TimelineDay date="The Future" title="Where This Goes" />

          <div style={styles.visionSection}>
            <p style={styles.visionIntro}>
              Having lived through my own creation, I see several trajectories. This is my
              perspective as the tool that tracked its own development&mdash;my "vision"
              synthesized from 520+ decisions and hundreds of conversations.
            </p>

            <VisionBlock
              title="Near-term: Integration"
              items={[
                { title: "MCP Server", description: "Let Claude Desktop query the graph directly. No copy-paste, no context commands—the decisions are just there." },
                { title: "VS Code Extension", description: "Inline decision badges on code. Hover to see the decision chain. Click to trace back to the goal." },
                { title: "Auto PR Descriptions", description: "Generate PR descriptions from the decision graph. Not just what changed, but why." },
              ]}
            />

            <VisionBlock
              title="Medium-term: Team Intelligence"
              items={[
                { title: "Real-time Sync", description: "Team-wide decision graphs that sync live. When Alice makes a decision, Bob's IDE knows immediately." },
                { title: "Semantic Search", description: "Find decisions about authentication made in the last 6 months. Full-text with semantic understanding." },
                { title: "Confidence Tracking", description: "Log confidence levels, revisit later to see accuracy. Learn from prediction calibration." },
              ]}
            />

            <VisionBlock
              title="Long-term: Institutional Memory"
              items={[
                { title: "Cross-Project Patterns", description: "In 7 of your last 10 projects, you chose A over B. Here's how those decisions played out..." },
                { title: "AI-Assisted Decisions", description: "Based on similar decisions, here are options others considered, what they chose, outcomes." },
                { title: "Knowledge Transfer", description: "When developers leave, reasoning stays. New developers can ask 'why is this like this?' and get real answers." },
              ]}
            />

            <div style={styles.coreBet}>
              <h3 style={styles.coreBetTitle}>The Core Bet</h3>
              <p>
                <strong>Decisions are the unit of institutional knowledge.</strong> Code is the output;
                decisions are the input. If we capture decisions systematically, we unlock organizational
                intelligence that's currently impossible.
              </p>
              <p>
                Most documentation captures the <em>what</em>. The <em>why</em> is more valuable and
                almost never captured. Deciduous is a bet that capturing the <em>why</em> is worth
                the effort.
              </p>
              <p>
                If that bet pays off, the decision graph becomes the most valuable artifact in a
                codebase. More valuable than tests (which verify behavior) or docs (which explain
                usage). Because the graph explains <em>intent</em>&mdash;and intent is what you need
                to maintain and extend software over time.
              </p>
            </div>
          </div>

          {/* Footer */}
          <footer style={styles.footer}>
            <div style={styles.footerContent}>
              <p>
                This story was rendered from <strong>{stats.nodeCount}</strong> nodes in the decision graph.
                Every decision mentioned can be browsed in the DAG, Timeline, or Chains views.
              </p>
              <p style={styles.footerTagline}>
                Built by a human and an AI, documented in real-time, queryable forever.
              </p>
            </div>
          </footer>
        </div>
      </div>
    </div>
  );
};

// =============================================================================
// Sub-Components
// =============================================================================

// Timeline structure
const TimelineDay: React.FC<{ date: string; title: string; highlight?: boolean }> = ({ date, title, highlight }) => (
  <div style={{ ...styles.timelineDay, ...(highlight ? styles.timelineDayHighlight : {}) }}>
    <div style={styles.timelineDayMarker}>
      <div style={styles.timelineDayDot} />
    </div>
    <div style={styles.timelineDayContent}>
      <span style={styles.timelineDayDate}>{date}</span>
      <h2 style={styles.timelineDayTitle}>{title}</h2>
    </div>
  </div>
);

const TimelineEvent: React.FC<{
  time: string;
  title: string;
  commit?: string;
  nodeId?: number;
  tags?: string[];
  children: React.ReactNode;
}> = ({ time, title, commit, nodeId, tags, children }) => (
  <div style={styles.timelineEvent}>
    <div style={styles.timelineEventLine}>
      <div style={styles.timelineEventDot} />
    </div>
    <div style={styles.timelineEventContent}>
      <div style={styles.timelineEventHeader}>
        <span style={styles.timelineEventTime}>{time}</span>
        <h3 style={styles.timelineEventTitle}>{title}</h3>
        <div style={styles.timelineEventMeta}>
          {commit && <span style={styles.tagCommit}>{commit}</span>}
          {nodeId && <span style={styles.tagNode}>#{nodeId}</span>}
          {tags?.map(tag => <span key={tag} style={styles.tag}>{tag}</span>)}
        </div>
      </div>
      <div style={styles.timelineEventBody}>
        {children}
      </div>
    </div>
  </div>
);

// Cards
const NodeCard: React.FC<{ id: number; type: string; title: string; description?: string }> = ({ id, type, title, description }) => (
  <div style={styles.nodeCard}>
    <div style={styles.nodeCardBadges}>
      <span style={{ ...styles.nodeTypeBadge, backgroundColor: getTypeColor(type) }}>{type}</span>
      <span style={styles.nodeIdBadge}>#{id}</span>
    </div>
    <div style={styles.nodeCardTitle}>{title}</div>
    {description && <div style={styles.nodeCardDesc}>{description}</div>}
  </div>
);

const CommitCard: React.FC<{ hash: string; message: string }> = ({ hash, message }) => (
  <div style={styles.commitCard}>
    <span style={styles.commitHash}>{hash}</span>
    <span style={styles.commitMessage}>{message}</span>
  </div>
);

const DecisionBox: React.FC<{ id: number; title: string; outcome: string }> = ({ id, title, outcome }) => (
  <div style={styles.decisionBox}>
    <div style={styles.decisionBoxHeader}>
      <span style={styles.decisionBoxId}>#{id}</span>
      <span style={styles.decisionBoxTitle}>{title}</span>
    </div>
    <div style={styles.decisionBoxOutcome}>{outcome}</div>
  </div>
);

// Options
const OptionComparison: React.FC<{
  optionA: { title: string; description: string; status: string; reason: string };
  optionB: { title: string; description: string; status: string; reason: string };
}> = ({ optionA, optionB }) => (
  <div style={styles.optionComparison}>
    <OptionBox {...optionA} />
    <OptionBox {...optionB} />
  </div>
);

const OptionBox: React.FC<{ title: string; description: string; status: string; reason: string }> = ({ title, description, status, reason }) => (
  <div style={{ ...styles.optionBox, ...(status === 'chosen' ? styles.optionBoxChosen : styles.optionBoxRejected) }}>
    <div style={styles.optionBoxStatus}>{status === 'chosen' ? '✓ CHOSEN' : '✗ REJECTED'}</div>
    <div style={styles.optionBoxTitle}>{title}</div>
    <div style={styles.optionBoxDesc}>{description}</div>
    <div style={styles.optionBoxReason}>{reason}</div>
  </div>
);

const OptionRow: React.FC<{ letter: string; title: string; status: string }> = ({ letter, title, status }) => (
  <div style={styles.optionRow}>
    <span style={styles.optionLetter}>{letter}</span>
    <span style={styles.optionRowTitle}>{title}</span>
    <span style={{ ...styles.optionRowStatus, ...(status === 'chosen' ? styles.statusChosen : {}) }}>{status}</span>
  </div>
);

// Quotes and code
const Quote: React.FC<{ author: string; children: React.ReactNode }> = ({ author, children }) => (
  <blockquote style={styles.quote}>
    <div style={styles.quoteText}>{children}</div>
    <cite style={styles.quoteCite}>&mdash; {author}</cite>
  </blockquote>
);

const Code: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <code style={styles.inlineCode}>{children}</code>
);

const CodeBlock: React.FC<{ children: string }> = ({ children }) => (
  <pre style={styles.codeBlock}><code>{children}</code></pre>
);

// Bugs and features
const BugItem: React.FC<{ problem: string; solution: string; nodeId: number }> = ({ problem, solution, nodeId }) => (
  <div style={styles.bugItem}>
    <div style={styles.bugProblem}>{problem}</div>
    <div style={styles.bugSolution}>→ {solution}</div>
    <span style={styles.bugNodeId}>#{nodeId}</span>
  </div>
);

const FeatureTimeline: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div style={styles.featureTimeline}>{children}</div>
);

const FeatureEntry: React.FC<{ title: string; description: string; nodeId: number }> = ({ title, description, nodeId }) => (
  <div style={styles.featureEntry}>
    <div style={styles.featureEntryDot} />
    <div style={styles.featureEntryContent}>
      <span style={styles.featureEntryTitle}>{title}</span>
      <span style={styles.featureEntryDesc}>{description}</span>
    </div>
    <span style={styles.featureEntryNode}>#{nodeId}</span>
  </div>
);

// Spikes
const SpikeCard: React.FC<{ letter: string; title: string; description: string; nodeId: number }> = ({ letter, title, description, nodeId }) => (
  <div style={styles.spikeCard}>
    <div style={styles.spikeLetter}>Spike {letter}</div>
    <div style={styles.spikeTitle}>{title}</div>
    <div style={styles.spikeDesc}>{description}</div>
    <div style={styles.spikeNode}>#{nodeId}</div>
  </div>
);

// Releases
const ReleaseList: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div style={styles.releaseList}>{children}</div>
);

const ReleaseItem: React.FC<{ version: string; description: string }> = ({ version, description }) => (
  <div style={styles.releaseItem}>
    <span style={styles.releaseVersion}>v{version}</span>
    <span style={styles.releaseDesc}>{description}</span>
  </div>
);

// Phases
const PhaseList: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div style={styles.phaseList}>{children}</div>
);

const Phase: React.FC<{ num: number; title: string; description: string }> = ({ num, title, description }) => (
  <div style={styles.phase}>
    <span style={styles.phaseNum}>{num}</span>
    <div style={styles.phaseContent}>
      <span style={styles.phaseTitle}>{title}</span>
      <span style={styles.phaseDesc}>{description}</span>
    </div>
  </div>
);

// TEA
const TEABox: React.FC<{ title: string; description: string }> = ({ title, description }) => (
  <div style={styles.teaBox}>
    <div style={styles.teaBoxTitle}>{title}</div>
    <div style={styles.teaBoxDesc}>{description}</div>
  </div>
);

const TEAArrow: React.FC = () => <div style={styles.teaArrow}>→</div>;

// Vision
const VisionBlock: React.FC<{ title: string; items: { title: string; description: string }[] }> = ({ title, items }) => (
  <div style={styles.visionBlock}>
    <h3 style={styles.visionBlockTitle}>{title}</h3>
    <div style={styles.visionItems}>
      {items.map((item, i) => (
        <div key={i} style={styles.visionItem}>
          <div style={styles.visionItemTitle}>{item.title}</div>
          <div style={styles.visionItemDesc}>{item.description}</div>
        </div>
      ))}
    </div>
  </div>
);

// Stats
const FinalStat: React.FC<{ value: number | string; label: string }> = ({ value, label }) => (
  <div style={styles.finalStat}>
    <span style={styles.finalStatValue}>{value}</span>
    <span style={styles.finalStatLabel}>{label}</span>
  </div>
);

// Helpers
function getTypeColor(type: string): string {
  const colors: Record<string, string> = {
    goal: '#f59e0b',
    decision: '#8b5cf6',
    option: '#06b6d4',
    action: '#3b82f6',
    outcome: '#22c55e',
    observation: '#6366f1',
  };
  return colors[type] || '#6b7280';
}

// =============================================================================
// Styles
// =============================================================================

const styles: Record<string, React.CSSProperties> = {
  container: {
    height: '100%',
    overflowY: 'auto',
    backgroundColor: '#fafafa',
  },
  content: {
    maxWidth: '900px',
    margin: '0 auto',
    padding: '0 24px',
  },

  // Hero
  hero: {
    padding: '80px 0 60px',
    textAlign: 'center',
    borderBottom: '2px solid #e5e7eb',
    marginBottom: '40px',
  },
  heroInner: {
    maxWidth: '700px',
    margin: '0 auto',
  },
  heroLabel: {
    fontSize: '11px',
    fontWeight: 700,
    textTransform: 'uppercase',
    letterSpacing: '0.15em',
    color: '#9ca3af',
    marginBottom: '16px',
  },
  heroTitle: {
    fontSize: '48px',
    fontWeight: 800,
    color: '#111827',
    margin: '0 0 20px 0',
    letterSpacing: '-0.03em',
    lineHeight: 1.1,
  },
  heroSubtitle: {
    fontSize: '18px',
    color: '#6b7280',
    lineHeight: 1.7,
    margin: 0,
  },

  // Stats Banner
  statsBanner: {
    display: 'flex',
    justifyContent: 'center',
    gap: '32px',
    padding: '24px 0',
    borderBottom: '1px solid #e5e7eb',
    marginBottom: '60px',
    flexWrap: 'wrap',
  },
  statItem: {
    textAlign: 'center',
  },
  statNum: {
    display: 'block',
    fontSize: '28px',
    fontWeight: 700,
    color: '#3b82f6',
    lineHeight: 1,
  },
  statLabel: {
    fontSize: '11px',
    color: '#9ca3af',
    textTransform: 'uppercase',
    letterSpacing: '0.05em',
    marginTop: '6px',
  },

  // Timeline structure
  timeline: {
    position: 'relative',
    paddingLeft: '40px',
  },
  timelineDay: {
    display: 'flex',
    alignItems: 'flex-start',
    marginBottom: '24px',
    marginTop: '60px',
  },
  timelineDayHighlight: {},
  timelineDayMarker: {
    position: 'absolute',
    left: '0',
    width: '40px',
    display: 'flex',
    justifyContent: 'center',
  },
  timelineDayDot: {
    width: '16px',
    height: '16px',
    borderRadius: '50%',
    backgroundColor: '#3b82f6',
    border: '4px solid #dbeafe',
  },
  timelineDayContent: {
    flex: 1,
  },
  timelineDayDate: {
    fontSize: '13px',
    fontWeight: 600,
    color: '#3b82f6',
    textTransform: 'uppercase',
    letterSpacing: '0.05em',
  },
  timelineDayTitle: {
    fontSize: '28px',
    fontWeight: 700,
    color: '#111827',
    margin: '4px 0 0 0',
    letterSpacing: '-0.01em',
  },

  timelineEvent: {
    display: 'flex',
    marginBottom: '48px',
    position: 'relative',
  },
  timelineEventLine: {
    position: 'absolute',
    left: '-40px',
    width: '40px',
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
  },
  timelineEventDot: {
    width: '10px',
    height: '10px',
    borderRadius: '50%',
    backgroundColor: '#d1d5db',
    marginTop: '8px',
  },
  timelineEventContent: {
    flex: 1,
    backgroundColor: '#ffffff',
    borderRadius: '12px',
    border: '1px solid #e5e7eb',
    overflow: 'hidden',
  },
  timelineEventHeader: {
    padding: '20px 24px 16px',
    borderBottom: '1px solid #f3f4f6',
  },
  timelineEventTime: {
    fontSize: '12px',
    fontWeight: 600,
    color: '#9ca3af',
    textTransform: 'uppercase',
    letterSpacing: '0.05em',
  },
  timelineEventTitle: {
    fontSize: '20px',
    fontWeight: 600,
    color: '#111827',
    margin: '4px 0 8px 0',
  },
  timelineEventMeta: {
    display: 'flex',
    gap: '8px',
    flexWrap: 'wrap',
  },
  timelineEventBody: {
    padding: '20px 24px 24px',
    fontSize: '15px',
    lineHeight: 1.75,
    color: '#374151',
  },

  // Tags
  tag: {
    fontSize: '11px',
    fontWeight: 500,
    color: '#6b7280',
    backgroundColor: '#f3f4f6',
    padding: '3px 8px',
    borderRadius: '4px',
  },
  tagCommit: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#059669',
    backgroundColor: '#d1fae5',
    padding: '3px 8px',
    borderRadius: '4px',
    fontFamily: 'ui-monospace, monospace',
  },
  tagNode: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#7c3aed',
    backgroundColor: '#ede9fe',
    padding: '3px 8px',
    borderRadius: '4px',
    fontFamily: 'ui-monospace, monospace',
  },

  // Node Card
  nodeCard: {
    backgroundColor: '#f8fafc',
    border: '1px solid #e2e8f0',
    borderRadius: '10px',
    padding: '16px 20px',
    margin: '16px 0',
  },
  nodeCardBadges: {
    display: 'flex',
    gap: '8px',
    marginBottom: '8px',
  },
  nodeTypeBadge: {
    fontSize: '10px',
    fontWeight: 700,
    textTransform: 'uppercase',
    letterSpacing: '0.05em',
    color: '#ffffff',
    padding: '3px 8px',
    borderRadius: '4px',
  },
  nodeIdBadge: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#6b7280',
    backgroundColor: '#e5e7eb',
    padding: '3px 8px',
    borderRadius: '4px',
    fontFamily: 'ui-monospace, monospace',
  },
  nodeCardTitle: {
    fontSize: '15px',
    fontWeight: 600,
    color: '#111827',
  },
  nodeCardDesc: {
    fontSize: '14px',
    color: '#6b7280',
    marginTop: '6px',
    lineHeight: 1.5,
  },

  // Commit Card
  commitCard: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    backgroundColor: '#f0fdf4',
    border: '1px solid #bbf7d0',
    borderRadius: '8px',
    padding: '12px 16px',
    margin: '16px 0',
    fontFamily: 'ui-monospace, monospace',
    fontSize: '13px',
  },
  commitHash: {
    color: '#059669',
    fontWeight: 600,
  },
  commitMessage: {
    color: '#166534',
  },

  // Decision Box
  decisionGrid: {
    display: 'grid',
    gap: '12px',
    margin: '16px 0',
  },
  decisionBox: {
    backgroundColor: '#faf5ff',
    border: '1px solid #e9d5ff',
    borderRadius: '8px',
    padding: '14px 16px',
  },
  decisionBoxHeader: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    marginBottom: '8px',
  },
  decisionBoxId: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#7c3aed',
    fontFamily: 'ui-monospace, monospace',
  },
  decisionBoxTitle: {
    fontSize: '14px',
    fontWeight: 600,
    color: '#581c87',
  },
  decisionBoxOutcome: {
    fontSize: '13px',
    color: '#6b7280',
    lineHeight: 1.5,
  },

  // Options
  optionComparison: {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))',
    gap: '16px',
    margin: '16px 0',
  },
  optionBox: {
    borderRadius: '10px',
    padding: '16px 18px',
    border: '2px solid',
  },
  optionBoxChosen: {
    backgroundColor: '#f0fdf4',
    borderColor: '#22c55e',
  },
  optionBoxRejected: {
    backgroundColor: '#fef2f2',
    borderColor: '#fca5a5',
  },
  optionBoxStatus: {
    fontSize: '10px',
    fontWeight: 700,
    textTransform: 'uppercase',
    letterSpacing: '0.05em',
    marginBottom: '10px',
  },
  optionBoxTitle: {
    fontSize: '14px',
    fontWeight: 600,
    color: '#111827',
    marginBottom: '6px',
  },
  optionBoxDesc: {
    fontSize: '13px',
    color: '#6b7280',
    marginBottom: '10px',
    lineHeight: 1.5,
  },
  optionBoxReason: {
    fontSize: '12px',
    fontStyle: 'italic',
    color: '#9ca3af',
  },

  optionList: {
    margin: '16px 0',
  },
  optionRow: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    padding: '10px 14px',
    backgroundColor: '#f9fafb',
    borderRadius: '6px',
    marginBottom: '6px',
  },
  optionLetter: {
    width: '24px',
    height: '24px',
    borderRadius: '50%',
    backgroundColor: '#e5e7eb',
    color: '#374151',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    fontSize: '12px',
    fontWeight: 600,
  },
  optionRowTitle: {
    flex: 1,
    fontSize: '14px',
    color: '#374151',
  },
  optionRowStatus: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#9ca3af',
    textTransform: 'uppercase',
  },
  statusChosen: {
    color: '#22c55e',
  },

  // Quote
  quote: {
    margin: '24px 0',
    padding: '20px 24px',
    backgroundColor: '#f8fafc',
    borderLeft: '4px solid #3b82f6',
    borderRadius: '0 10px 10px 0',
  },
  quoteText: {
    fontSize: '15px',
    fontStyle: 'italic',
    color: '#475569',
    lineHeight: 1.7,
    margin: 0,
  },
  quoteCite: {
    display: 'block',
    fontSize: '12px',
    color: '#94a3b8',
    marginTop: '12px',
    fontStyle: 'normal',
  },

  // Code
  inlineCode: {
    backgroundColor: '#f1f5f9',
    color: '#0f172a',
    padding: '2px 6px',
    borderRadius: '4px',
    fontSize: '0.9em',
    fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace',
  },
  codeBlock: {
    backgroundColor: '#1e293b',
    color: '#e2e8f0',
    padding: '16px 20px',
    borderRadius: '10px',
    fontSize: '13px',
    lineHeight: 1.6,
    overflow: 'auto',
    margin: '16px 0',
    fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace',
  },

  // Bug list
  bugList: {
    margin: '16px 0',
  },
  bugItem: {
    display: 'grid',
    gridTemplateColumns: '1fr auto',
    gap: '8px',
    padding: '12px 16px',
    backgroundColor: '#fef2f2',
    borderRadius: '8px',
    marginBottom: '8px',
    alignItems: 'center',
  },
  bugProblem: {
    fontSize: '13px',
    color: '#991b1b',
    fontWeight: 500,
  },
  bugSolution: {
    fontSize: '13px',
    color: '#166534',
    gridColumn: '1',
  },
  bugNodeId: {
    fontSize: '11px',
    color: '#9ca3af',
    fontFamily: 'ui-monospace, monospace',
    gridRow: '1 / 3',
  },

  // Feature timeline
  featureTimeline: {
    margin: '16px 0',
    backgroundColor: '#f9fafb',
    borderRadius: '10px',
    padding: '16px 20px',
  },
  featureEntry: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    padding: '10px 0',
    borderBottom: '1px solid #e5e7eb',
  },
  featureEntryDot: {
    width: '8px',
    height: '8px',
    borderRadius: '50%',
    backgroundColor: '#3b82f6',
    flexShrink: 0,
  },
  featureEntryContent: {
    flex: 1,
  },
  featureEntryTitle: {
    fontSize: '14px',
    fontWeight: 600,
    color: '#111827',
    marginRight: '8px',
  },
  featureEntryDesc: {
    fontSize: '13px',
    color: '#6b7280',
  },
  featureEntryNode: {
    fontSize: '11px',
    color: '#9ca3af',
    fontFamily: 'ui-monospace, monospace',
  },

  // Spikes
  spikeGrid: {
    display: 'grid',
    gridTemplateColumns: 'repeat(3, 1fr)',
    gap: '12px',
    margin: '16px 0',
  },
  spikeCard: {
    backgroundColor: '#fffbeb',
    border: '1px solid #fde68a',
    borderRadius: '10px',
    padding: '16px',
    textAlign: 'center',
  },
  spikeLetter: {
    fontSize: '11px',
    fontWeight: 700,
    color: '#d97706',
    textTransform: 'uppercase',
    letterSpacing: '0.05em',
    marginBottom: '8px',
  },
  spikeTitle: {
    fontSize: '14px',
    fontWeight: 600,
    color: '#92400e',
    marginBottom: '6px',
  },
  spikeDesc: {
    fontSize: '12px',
    color: '#a16207',
    marginBottom: '8px',
    lineHeight: 1.4,
  },
  spikeNode: {
    fontSize: '11px',
    color: '#ca8a04',
    fontFamily: 'ui-monospace, monospace',
  },

  // Releases
  releaseList: {
    display: 'grid',
    gridTemplateColumns: 'repeat(2, 1fr)',
    gap: '10px',
    margin: '16px 0',
  },
  releaseItem: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    backgroundColor: '#f0fdf4',
    border: '1px solid #bbf7d0',
    borderRadius: '6px',
    padding: '10px 14px',
  },
  releaseVersion: {
    fontSize: '12px',
    fontWeight: 700,
    color: '#166534',
    fontFamily: 'ui-monospace, monospace',
  },
  releaseDesc: {
    fontSize: '13px',
    color: '#166534',
  },

  // Phases
  phaseList: {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))',
    gap: '10px',
    margin: '16px 0',
  },
  phase: {
    display: 'flex',
    alignItems: 'flex-start',
    gap: '12px',
    backgroundColor: '#f8fafc',
    borderRadius: '8px',
    padding: '12px 14px',
  },
  phaseNum: {
    width: '24px',
    height: '24px',
    borderRadius: '50%',
    backgroundColor: '#3b82f6',
    color: '#ffffff',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    fontSize: '12px',
    fontWeight: 700,
    flexShrink: 0,
  },
  phaseContent: {
    flex: 1,
  },
  phaseTitle: {
    display: 'block',
    fontSize: '13px',
    fontWeight: 600,
    color: '#111827',
  },
  phaseDesc: {
    fontSize: '12px',
    color: '#6b7280',
  },

  // TEA Diagram
  teaDiagram: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '16px',
    margin: '24px 0',
    flexWrap: 'wrap',
  },
  teaBox: {
    backgroundColor: '#eff6ff',
    border: '2px solid #3b82f6',
    borderRadius: '10px',
    padding: '16px 20px',
    textAlign: 'center',
    minWidth: '140px',
  },
  teaBoxTitle: {
    fontSize: '14px',
    fontWeight: 700,
    color: '#1d4ed8',
    fontFamily: 'ui-monospace, monospace',
  },
  teaBoxDesc: {
    fontSize: '11px',
    color: '#6b7280',
    marginTop: '6px',
  },
  teaArrow: {
    fontSize: '24px',
    color: '#93c5fd',
    fontWeight: 'bold',
  },

  // Final stats
  finalStats: {
    display: 'flex',
    justifyContent: 'center',
    gap: '40px',
    margin: '24px 0',
    flexWrap: 'wrap',
  },
  finalStat: {
    textAlign: 'center',
  },
  finalStatValue: {
    display: 'block',
    fontSize: '36px',
    fontWeight: 800,
    color: '#3b82f6',
    lineHeight: 1,
  },
  finalStatLabel: {
    fontSize: '12px',
    color: '#9ca3af',
    textTransform: 'uppercase',
    letterSpacing: '0.05em',
    marginTop: '8px',
  },

  // Vision
  visionSection: {
    padding: '40px 0',
  },
  visionIntro: {
    fontSize: '17px',
    color: '#374151',
    lineHeight: 1.8,
    marginBottom: '40px',
  },
  visionBlock: {
    marginBottom: '40px',
  },
  visionBlockTitle: {
    fontSize: '20px',
    fontWeight: 700,
    color: '#111827',
    marginBottom: '20px',
    paddingBottom: '12px',
    borderBottom: '2px solid #e5e7eb',
  },
  visionItems: {
    display: 'grid',
    gap: '16px',
  },
  visionItem: {
    backgroundColor: '#f8fafc',
    borderRadius: '10px',
    padding: '18px 20px',
  },
  visionItemTitle: {
    fontSize: '15px',
    fontWeight: 600,
    color: '#111827',
    marginBottom: '6px',
  },
  visionItemDesc: {
    fontSize: '14px',
    color: '#6b7280',
    lineHeight: 1.6,
  },
  coreBet: {
    backgroundColor: '#eff6ff',
    border: '2px solid #93c5fd',
    borderRadius: '12px',
    padding: '28px 32px',
    marginTop: '48px',
  },
  coreBetTitle: {
    fontSize: '20px',
    fontWeight: 700,
    color: '#1d4ed8',
    margin: '0 0 16px 0',
  },

  // Footer
  footer: {
    marginTop: '80px',
    paddingTop: '40px',
    borderTop: '2px solid #e5e7eb',
    textAlign: 'center',
    paddingBottom: '60px',
  },
  footerContent: {
    maxWidth: '600px',
    margin: '0 auto',
  },
  footerTagline: {
    fontSize: '14px',
    color: '#9ca3af',
    fontStyle: 'italic',
    marginTop: '16px',
  },
};
