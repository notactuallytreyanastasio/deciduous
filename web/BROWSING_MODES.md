# Decision Graph Browsing Modes

The web viewer provides three browsing modes for exploring decision graphs, each optimized for different use cases.

## Overview

| Mode | Purpose | Best For |
|------|---------|----------|
| **Structural** | Browse by goal trees | Understanding how decisions relate to goals |
| **Branch** | Browse by git branch | Tracking work across feature branches |
| **Narratives** | Browse evolution over time | Understanding how thinking evolved |

## Structural Mode (`/structural`)

Browse the decision graph by goal trees - connected subgraphs rooted at goal nodes.

### Features

- **Goal Dropdown**: Select a goal to view its connected subgraph
- **Hierarchical Tree**: Expand/collapse nodes to explore relationships
- **Edge Type Labels**: See how nodes connect (leads_to, chosen, rejected, etc.)
- **REVISIT Badges**: Highlight pivot nodes with expandable context

### Data Types

```typescript
interface StructuralGroup {
  id: string;                    // Unique identifier
  rootNode: DecisionNode;        // The root goal
  nodes: DecisionNode[];         // All connected nodes
  edges: DecisionEdge[];         // Internal edges
  isAutoDetected: boolean;       // true for goal-rooted groups
  label?: string;                // Display label
}

interface TreeNode {
  node: DecisionNode;
  children: TreeNode[];
  edgeFromParent?: DecisionEdge;
  depth: number;
  isExpanded: boolean;
  isRevisit: boolean;
}
```

### Algorithm

1. Find all goal nodes
2. For each goal, BFS to find all connected nodes (both directions)
3. Build tree structure following edge direction
4. Sort children by creation time

## Branch Mode (`/branch`)

Browse decisions grouped by git branch metadata.

### Features

- **Branch Dropdown**: Select a branch to view its nodes
- **Narrative Summary**: Goals, outcomes, pivots, and open work
- **Cross-Branch Indicators**: See connections to other branches
- **View Toggle**: Switch between tree and timeline views
- **Timeline View**: Chronological progression with cross-branch highlights

### Data Types

```typescript
interface BranchGroup {
  branch: string;                // Branch name
  nodes: DecisionNode[];         // Nodes on this branch
  edges: DecisionEdge[];         // Internal edges
  crossBranchEdges: DecisionEdge[]; // Edges to/from other branches
  commits: GitCommit[];          // Linked commits
  dateRange: { start: Date; end: Date };
  nodeCount: number;
}

interface BranchNarrativeSummary {
  headline: string;
  goals: string[];
  keyDecisions: string[];
  outcomes: string[];
  pivots: string[];
  openWork: string[];            // Actions without outcomes
  crossBranchDeps: {
    branch: string;
    direction: 'incoming' | 'outgoing' | 'both';
    summary: string;
  }[];
}

interface BranchTimelineItem {
  node: DecisionNode;
  timestamp: Date;
  isCrossBranch: boolean;
  crossBranchInfo?: {
    branch: string;
    direction: 'incoming' | 'outgoing';
  };
}
```

### Algorithm

1. Group nodes by branch metadata
2. Find edges within branch and crossing to other branches
3. Generate narrative summary from node types
4. Build timeline sorted by creation time

## Narratives Mode (`/narratives`)

Browse the evolution of decisions over time, with pivots (REVISIT nodes) as focal points.

### Features

- **Summary Section**: Headline, achievements, pivots, open questions
- **Chapter Organization**: Items grouped into logical phases
- **Pivot Cards**: Expanded view of what was superseded, why, and replacements
- **Chronological Timeline**: Within each chapter

### Data Types

```typescript
interface NarrativeChapter {
  id: string;
  title: string;
  summary: string;               // e.g., "3 decisions, 2 actions, 1 pivot"
  items: NarrativeItem[];
  startTime: Date;
  endTime: Date;
  pivot?: PivotEvent;
  outcome?: DecisionNode;
  decisions: DecisionNode[];
}

interface NarrativeSummary {
  headline: string;
  goalsAchieved: string[];
  keyPivots: string[];
  openQuestions: string[];
}

interface PivotEvent {
  revisitNode: DecisionNode;
  supersededNodes: DecisionNode[];  // What was abandoned
  reasonNodes: DecisionNode[];      // Observations that triggered it
  replacementNodes: DecisionNode[]; // New approach
  timestamp: Date;
}

interface NarrativeItem {
  type: 'goal' | 'decision' | 'action' | 'outcome' | 'observation' | 'pivot';
  timestamp: Date;
  node: DecisionNode;
  pivotEvent?: PivotEvent;
  relatedNodes?: DecisionNode[];
  linkedCommit?: GitCommit;
}
```

### Chapter Detection Algorithm

Chapters are delimited by:
1. REVISIT/pivot nodes (end current chapter)
2. New goal nodes
3. Significant time gaps (> 4 hours)

## Search Filtering

All modes support search filtering:

```typescript
function filterBySearch(nodes: DecisionNode[], query: string): DecisionNode[];
```

- Searches title and description (case-insensitive)
- In tree views, shows matching nodes AND their ancestors
- In timeline views, shows only matching nodes
- Empty results show appropriate empty state

## File Structure

```
web/src/
├── views/
│   ├── StructuralView.tsx      # Goal tree browsing
│   ├── BranchView.tsx          # Git branch browsing
│   └── NarrativesView.tsx      # Timeline/evolution browsing
├── components/
│   ├── ModeSelector.tsx        # Top navigation bar
│   ├── HierarchicalTree.tsx    # Reusable tree component
│   ├── PivotCard.tsx           # REVISIT node visualization
│   └── DetailPanel.tsx         # Node detail sidebar
├── utils/
│   ├── structuralProcessing.ts # Goal tree extraction
│   ├── branchProcessing.ts     # Branch grouping
│   └── narrativeProcessing.ts  # Timeline/chapter building
└── types/
    └── graph.ts                # All type definitions
```

## URL Routes

- `/structural` - Structural mode (default)
- `/structural/:groupId` - Specific goal group
- `/structural/:groupId/:nodeId` - Specific node selected
- `/branch` - Branch mode
- `/branch/:branch` - Specific branch
- `/branch/:branch/:nodeId` - Specific node on branch
- `/narratives` - Narratives mode
- `/narratives/:groupId` - Specific group narrative
- `/narratives/:groupId/:nodeId` - Specific node in narrative
