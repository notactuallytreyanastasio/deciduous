# Graph Explorer & Enhanced Search - Planning Document

## Overview

This document outlines the plan to evolve the DAG viewer from a simple visualization tool into a powerful **graph explorer** with intelligent search capabilities. The goal is to make large decision graphs navigable and searchable at any zoom level.

## Problem Statement

The current search implementation has limitations when working with large graphs:

1. **Invisible matches**: When zoomed out, highlighted nodes are too small to see
2. **No way to navigate to off-screen matches**: Results may be outside the viewport
3. **No focus mode**: Can't drill into a subset of the graph
4. **No lazy loading**: All nodes load at once, causing performance issues on large graphs
5. **Limited context**: Searching finds nodes but doesn't help understand their connections

## Proposed Features

### 1. Floating Search Results Panel

When search matches exist, display a floating panel on the side that:
- Lists all matching nodes with their titles and types
- Shows which matches are **visible** vs **hidden** (off-screen or too small)
- Clicking a result: opens the detail modal for that node
- Supports keyboard navigation (arrow keys, enter to select)

### 2. Magnifier/Callout Lines

For nodes that are visible but too small to read:
- Draw a "callout line" from the tiny node to a floating label
- The label shows: node ID, title (truncated), and type badge
- Lines are semi-transparent to not obscure the graph
- Only show callouts for search matches (not all nodes)
- Callouts auto-hide when zooming in past a threshold

### 3. Focus Mode

When user selects specific nodes or chains:
- Hide all unrelated nodes (fade or remove)
- Recenter the view on the focused subgraph
- Show only edges between focused nodes
- Provide a clear "Exit Focus Mode" button
- Preserve focus through zoom/pan operations

Implementation options:
- **a) Node-based focus**: Click "Focus" on any node in the modal to focus on it + its connected nodes
- **b) Search-based focus**: "Focus on results" button that isolates all current search matches
- **c) Chain-based focus**: Focus on entire chain(s) from the sidebar dropdown

### 4. Lazy Loading / Progressive Reveal

For graphs with 100+ nodes:
- Initially show only "root" nodes (goals, high-connection nodes)
- Click to expand: reveal direct children
- Double-click: reveal full subgraph (all descendants)
- Collapse button: hide children again
- Show "+N nodes" badge on collapsed parents

### 5. Visibility Detection

Technical feature to support the above:
- Track which nodes are "visible" in the current viewport
- Account for zoom level (nodes < 10px = "too small")
- Categorize nodes into: `visible` | `too-small` | `off-screen`
- Update on zoom/pan events (debounced for performance)

---

## User Stories

1. **As a user searching for "auth"**, I want to see a list of all matching nodes even if they're zoomed out, so I can click to view any of them.

2. **As a user viewing a large graph**, I want tiny search matches to have callout labels pointing to them, so I can find them without zooming in.

3. **As a user investigating a bug**, I want to focus on just the nodes related to a specific goal chain, hiding everything else to reduce noise.

4. **As a user opening a 500-node graph**, I want it to load quickly by only showing top-level goals initially, letting me drill down as needed.

---

## Technical Approach

### Current Architecture

```
DagView.tsx
├── SearchBar.tsx (dropdown results, highlighting)
├── D3 + Dagre (layout + rendering)
├── Detail Modal (node info on click)
└── Controls sidebar (chain selection, legend)
```

### Proposed Architecture

```
DagView.tsx
├── SearchBar.tsx (enhanced)
│   └── SearchResultsPanel.tsx (floating side panel)
├── D3 + Dagre
│   ├── CalloutLines.tsx (magnifier lines for tiny nodes)
│   ├── VisibilityTracker (useNodeVisibility hook)
│   └── LazyNodeRenderer (progressive loading)
├── FocusMode.tsx (isolation logic)
├── Detail Modal
└── Controls sidebar
```

### New Hooks

```typescript
// Track which nodes are visible at current zoom/pan
useNodeVisibility(svgRef, nodes, zoom): Map<number, 'visible' | 'too-small' | 'off-screen'>

// Manage focus mode state
useFocusMode(nodes, edges): {
  focusedNodes: Set<number>,
  enterFocus: (nodeIds: number[]) => void,
  exitFocus: () => void,
  isFocused: boolean
}

// Lazy loading state
useLazyGraph(nodes, edges): {
  expandedNodes: Set<number>,
  visibleNodes: DecisionNode[],
  expand: (nodeId: number) => void,
  collapse: (nodeId: number) => void,
  expandAll: () => void,
  collapseToRoots: () => void
}
```

### Data Flow

```
Search Query
    ↓
SearchBar finds matches
    ↓
Update highlightedNodeIds (existing)
    ↓
useNodeVisibility categorizes matches
    ↓
├── Visible → highlight in graph (existing)
├── Too-small → render CalloutLines
└── Off-screen → show in SearchResultsPanel with "Navigate" button
```

---

## Implementation Approaches

We will prototype two different approaches to validate the UX:

### Approach A: Side Panel First

Priority order:
1. Floating SearchResultsPanel that categorizes visible/hidden matches
2. "Navigate to" button that pans/zooms to bring node into view
3. Focus mode triggered from panel ("Focus on these results")
4. Callout lines added later for polish

**Pros:**
- Panel is simpler to implement
- Clear UX: results are always accessible
- Works at any zoom level

**Cons:**
- Takes up screen real estate
- Two places to look (panel + graph)

### Approach B: Callouts First

Priority order:
1. Callout lines for too-small nodes
2. Visibility tracking hook
3. Mini-map or indicator showing off-screen match locations
4. Focus mode from node modal

**Pros:**
- Keeps attention on the graph
- More visually integrated
- No extra UI elements

**Cons:**
- Callout lines can get cluttered with many matches
- Off-screen nodes still need some panel/indicator

---

## File Changes Summary

| File | Changes |
|------|---------|
| `web/src/views/DagView.tsx` | Add focus mode, lazy loading, visibility tracking |
| `web/src/components/SearchBar.tsx` | Extend to support results panel mode |
| `web/src/components/SearchResultsPanel.tsx` | **NEW** - Floating side panel for search results |
| `web/src/components/CalloutLines.tsx` | **NEW** - SVG callout lines for tiny nodes |
| `web/src/components/MiniMap.tsx` | **NEW** (optional) - Miniature overview with match indicators |
| `web/src/hooks/useNodeVisibility.ts` | **NEW** - Track visible/hidden nodes |
| `web/src/hooks/useFocusMode.ts` | **NEW** - Focus mode state management |
| `web/src/hooks/useLazyGraph.ts` | **NEW** - Lazy loading logic |

---

## Success Criteria

1. **Search finds invisible nodes**: All matches are accessible regardless of zoom level
2. **Callouts are readable**: Tiny nodes have clear labels that don't overlap
3. **Focus mode isolates**: Can reduce a 100-node graph to just relevant nodes
4. **Performance**: 500-node graphs load in <1s, interactions feel instant
5. **Intuitive**: New users can discover features without documentation

---

## Open Questions

1. Should callout lines be always-on for search matches, or toggle-able?
2. For focus mode, should we animate the transition or instant-switch?
3. Should lazy loading be opt-in or default for large graphs?
4. How should we handle overlapping callout lines?
5. Should the results panel be resizable/collapsible?

---

## Next Steps

1. Create two implementation branches from this planning document
2. **Branch A**: Implement Side Panel First approach
3. **Branch B**: Implement Callouts First approach
4. Open PRs for both with detailed writeups
5. Compare UX and choose winner (or merge best of both)
