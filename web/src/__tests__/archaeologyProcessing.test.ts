/**
 * Tests for Archaeology Processing Utilities
 */

import { describe, it, expect } from 'vitest';
import type { DecisionNode, DecisionEdge, GraphData } from '../types/graph';
import type { Narrative } from '../types/archaeology';
import {
  findPivots,
  buildNarratives,
  aggregateGithubLinks,
  formatNarrativeContext,
  filterNarratives,
  calculateArchaeologyStats,
  traceWhyChain,
} from '../utils/archaeologyProcessing';
import { DEFAULT_ARCHAEOLOGY_FILTERS } from '../types/archaeology';

// =============================================================================
// Test Helpers
// =============================================================================

function makeNode(
  id: number,
  type: DecisionNode['node_type'],
  title: string,
  metadata: Record<string, unknown> = {}
): DecisionNode {
  return {
    id,
    change_id: `change-${id}`,
    node_type: type,
    title,
    description: null,
    status: 'active',
    created_at: new Date(2024, 0, id).toISOString(), // Use id as day for ordering
    updated_at: new Date(2024, 0, id).toISOString(),
    metadata_json: Object.keys(metadata).length > 0 ? JSON.stringify(metadata) : null,
  };
}

function makeEdge(
  id: number,
  from: number,
  to: number,
  type: DecisionEdge['edge_type'] = 'leads_to'
): DecisionEdge {
  return {
    id,
    from_node_id: from,
    to_node_id: to,
    from_change_id: `change-${from}`,
    to_change_id: `change-${to}`,
    edge_type: type,
    weight: null,
    rationale: null,
    created_at: new Date().toISOString(),
  };
}

function makeGraphData(nodes: DecisionNode[], edges: DecisionEdge[]): GraphData {
  return { nodes, edges };
}

// =============================================================================
// findPivots Tests
// =============================================================================

describe('findPivots', () => {
  it('returns empty array when no revisits exist', () => {
    const graphData = makeGraphData(
      [
        makeNode(1, 'goal', 'Goal 1'),
        makeNode(2, 'decision', 'Decision 1'),
      ],
      [makeEdge(1, 1, 2)]
    );

    const pivots = findPivots(graphData);
    expect(pivots).toHaveLength(0);
  });

  it('detects observation -> revisit -> decision pattern', () => {
    const graphData = makeGraphData(
      [
        makeNode(1, 'goal', 'Original Goal'),
        makeNode(2, 'decision', 'Old Decision'),
        makeNode(3, 'observation', 'Found a problem'),
        makeNode(4, 'revisit', 'Reconsidering approach'),
        makeNode(5, 'decision', 'New Decision'),
      ],
      [
        makeEdge(1, 1, 2),
        makeEdge(2, 2, 3), // decision -> observation
        makeEdge(3, 3, 4), // observation -> revisit
        makeEdge(4, 4, 5), // revisit -> new decision
      ]
    );

    const pivots = findPivots(graphData);
    expect(pivots).toHaveLength(1);

    const pivot = pivots[0];
    expect(pivot.revisitNode.id).toBe(4);
    expect(pivot.triggeringObservations).toHaveLength(1);
    expect(pivot.triggeringObservations[0].id).toBe(3);
    expect(pivot.newApproachNodes).toHaveLength(1);
    expect(pivot.newApproachNodes[0].id).toBe(5);
    expect(pivot.supersededNodes).toHaveLength(1);
    expect(pivot.supersededNodes[0].id).toBe(2);
  });

  it('handles multiple observations feeding one revisit', () => {
    const graphData = makeGraphData(
      [
        makeNode(1, 'decision', 'Old Decision'),
        makeNode(2, 'observation', 'Problem 1'),
        makeNode(3, 'observation', 'Problem 2'),
        makeNode(4, 'revisit', 'Reconsidering'),
        makeNode(5, 'action', 'New Action'),
      ],
      [
        makeEdge(1, 1, 2),
        makeEdge(2, 1, 3),
        makeEdge(3, 2, 4), // obs1 -> revisit
        makeEdge(4, 3, 4), // obs2 -> revisit
        makeEdge(5, 4, 5),
      ]
    );

    const pivots = findPivots(graphData);
    expect(pivots).toHaveLength(1);
    expect(pivots[0].triggeringObservations).toHaveLength(2);
  });

  it('handles revisit with no incoming observations', () => {
    const graphData = makeGraphData(
      [
        makeNode(1, 'revisit', 'Orphan Revisit'),
        makeNode(2, 'decision', 'New Decision'),
      ],
      [makeEdge(1, 1, 2)]
    );

    const pivots = findPivots(graphData);
    expect(pivots).toHaveLength(1);
    expect(pivots[0].triggeringObservations).toHaveLength(0);
    expect(pivots[0].newApproachNodes).toHaveLength(1);
  });
});

// =============================================================================
// buildNarratives Tests
// =============================================================================

describe('buildNarratives', () => {
  it('creates narrative for each goal', () => {
    const graphData = makeGraphData(
      [
        makeNode(1, 'goal', 'Goal 1'),
        makeNode(2, 'decision', 'Decision 1'),
        makeNode(3, 'goal', 'Goal 2'),
        makeNode(4, 'action', 'Action 2'),
      ],
      [
        makeEdge(1, 1, 2),
        makeEdge(2, 3, 4),
      ]
    );

    const narratives = buildNarratives(graphData);
    expect(narratives).toHaveLength(2);

    // Both narratives should have a goal as root
    expect(narratives.every(n => n.root.node_type === 'goal')).toBe(true);
  });

  it('includes all connected nodes in narrative', () => {
    const graphData = makeGraphData(
      [
        makeNode(1, 'goal', 'Main Goal'),
        makeNode(2, 'decision', 'Decision 1'),
        makeNode(3, 'option', 'Option A'),
        makeNode(4, 'option', 'Option B'),
        makeNode(5, 'action', 'Implementation'),
      ],
      [
        makeEdge(1, 1, 2),
        makeEdge(2, 2, 3),
        makeEdge(3, 2, 4),
        makeEdge(4, 3, 5),
      ]
    );

    const narratives = buildNarratives(graphData);
    expect(narratives).toHaveLength(1);
    expect(narratives[0].nodes).toHaveLength(5);
  });

  it('handles orphan revisit nodes as narrative roots', () => {
    const graphData = makeGraphData(
      [
        makeNode(1, 'revisit', 'Pivot Point'),
        makeNode(2, 'decision', 'New Direction'),
      ],
      [makeEdge(1, 1, 2)]
    );

    const narratives = buildNarratives(graphData);
    expect(narratives).toHaveLength(1);
    expect(narratives[0].root.node_type).toBe('revisit');
  });

  it('calculates correct time range', () => {
    const graphData = makeGraphData(
      [
        makeNode(1, 'goal', 'Goal'), // Jan 1
        makeNode(10, 'decision', 'Decision'), // Jan 10
        makeNode(20, 'outcome', 'Outcome'), // Jan 20
      ],
      [
        makeEdge(1, 1, 10),
        makeEdge(2, 10, 20),
      ]
    );

    const narratives = buildNarratives(graphData);
    expect(narratives).toHaveLength(1);

    const { start, end } = narratives[0].timeRange;
    expect(start.getDate()).toBe(1);
    expect(end.getDate()).toBe(20);
  });

  it('groups orphaned nodes into miscellaneous narrative', () => {
    const graphData = makeGraphData(
      [
        makeNode(1, 'goal', 'Connected Goal'),
        makeNode(2, 'decision', 'Connected Decision'),
        makeNode(3, 'observation', 'Orphan 1'), // No edges
        makeNode(4, 'action', 'Orphan 2'), // No edges
      ],
      [makeEdge(1, 1, 2)]
    );

    const narratives = buildNarratives(graphData);
    expect(narratives).toHaveLength(2);

    const orphanNarrative = narratives.find(n => n.id === 'orphaned');
    expect(orphanNarrative).toBeDefined();
    expect(orphanNarrative?.nodes).toHaveLength(2);
  });

  it('uses narrative_name from metadata when available', () => {
    const graphData = makeGraphData(
      [makeNode(1, 'goal', 'Generic Title', { narrative_name: 'Authentication Story' })],
      []
    );

    const narratives = buildNarratives(graphData);
    expect(narratives[0].name).toBe('Authentication Story');
  });

  it('correctly identifies pivots within narrative', () => {
    const graphData = makeGraphData(
      [
        makeNode(1, 'goal', 'Main Goal'),
        makeNode(2, 'observation', 'Found issue'),
        makeNode(3, 'revisit', 'Reconsidering'),
        makeNode(4, 'decision', 'New approach'),
      ],
      [
        makeEdge(1, 1, 2),
        makeEdge(2, 2, 3),
        makeEdge(3, 3, 4),
      ]
    );

    const narratives = buildNarratives(graphData);
    expect(narratives).toHaveLength(1);
    expect(narratives[0].pivots).toHaveLength(1);
    expect(narratives[0].observations).toHaveLength(1);
  });
});

// =============================================================================
// aggregateGithubLinks Tests
// =============================================================================

describe('aggregateGithubLinks', () => {
  it('extracts commit links from metadata', () => {
    const nodes = [
      makeNode(1, 'action', 'Commit action', { commit: 'abc123def456' }),
    ];
    const graphData = makeGraphData(nodes, []);

    const links = aggregateGithubLinks(nodes, graphData);
    expect(links).toHaveLength(1);
    expect(links[0].type).toBe('commit');
    expect(links[0].identifier).toBe('abc123def456');
  });

  it('extracts PR links from metadata', () => {
    const nodes = [
      makeNode(1, 'action', 'PR action', { github_pr: 42 }),
    ];
    const graphData = makeGraphData(nodes, []);

    const links = aggregateGithubLinks(nodes, graphData);
    expect(links).toHaveLength(1);
    expect(links[0].type).toBe('pr');
    expect(links[0].identifier).toBe('42');
    expect(links[0].url).toContain('/pull/42');
  });

  it('extracts issue links from metadata', () => {
    const nodes = [
      makeNode(1, 'goal', 'Issue goal', { github_issue: 123 }),
    ];
    const graphData = makeGraphData(nodes, []);

    const links = aggregateGithubLinks(nodes, graphData);
    expect(links).toHaveLength(1);
    expect(links[0].type).toBe('issue');
    expect(links[0].identifier).toBe('123');
  });

  it('uses config default repo when node has no repo', () => {
    const nodes = [
      makeNode(1, 'action', 'Action', { commit: 'abc123' }),
    ];
    const graphData: GraphData = {
      nodes,
      edges: [],
      config: { github: { commit_repo: 'myorg/myrepo' } },
    };

    const links = aggregateGithubLinks(nodes, graphData);
    expect(links[0].repo).toBe('myorg/myrepo');
    expect(links[0].url).toContain('myorg/myrepo');
  });

  it('uses node-level github_repo override', () => {
    const nodes = [
      makeNode(1, 'action', 'Action', { commit: 'abc123', github_repo: 'other/repo' }),
    ];
    const graphData: GraphData = {
      nodes,
      edges: [],
      config: { github: { commit_repo: 'default/repo' } },
    };

    const links = aggregateGithubLinks(nodes, graphData);
    expect(links[0].repo).toBe('other/repo');
  });

  it('deduplicates links by type, identifier, and repo', () => {
    const nodes = [
      makeNode(1, 'action', 'Action 1', { commit: 'abc123' }),
      makeNode(2, 'action', 'Action 2', { commit: 'abc123' }), // Same commit
    ];
    const graphData = makeGraphData(nodes, []);

    const links = aggregateGithubLinks(nodes, graphData);
    expect(links).toHaveLength(1);
  });

  it('handles nodes with multiple link types', () => {
    const nodes = [
      makeNode(1, 'action', 'Full action', {
        commit: 'abc123',
        github_pr: 42,
        github_issue: 10,
      }),
    ];
    const graphData = makeGraphData(nodes, []);

    const links = aggregateGithubLinks(nodes, graphData);
    expect(links).toHaveLength(3);
    expect(links.map(l => l.type).sort()).toEqual(['commit', 'issue', 'pr']);
  });
});

// =============================================================================
// formatNarrativeContext Tests
// =============================================================================

describe('formatNarrativeContext', () => {
  it('generates valid context structure', () => {
    const narrative: Narrative = {
      id: 'test-id',
      name: 'Test Narrative',
      root: makeNode(1, 'goal', 'Root'),
      nodes: [
        makeNode(1, 'goal', 'Root'),
        makeNode(2, 'decision', 'Decision'),
      ],
      edges: [],
      pivots: [],
      observations: [],
      timeRange: { start: new Date(), end: new Date() },
      githubLinks: [],
    };

    const context = formatNarrativeContext(narrative);

    expect(context.name).toBe('Test Narrative');
    expect(context.root_id).toBe(1);
    expect(context.node_ids).toEqual([1, 2]);
    expect(context.pivots).toEqual([]);
    expect(context.github_links).toEqual([]);
  });

  it('includes pivot information in context', () => {
    const revisitNode = makeNode(3, 'revisit', 'Revisit');
    const observationNode = makeNode(2, 'observation', 'Observation');

    const narrative: Narrative = {
      id: 'test-id',
      name: 'Test',
      root: makeNode(1, 'goal', 'Root'),
      nodes: [makeNode(1, 'goal', 'Root'), observationNode, revisitNode],
      edges: [],
      pivots: [{
        revisitNode,
        triggeringObservations: [observationNode],
        supersededNodes: [],
        newApproachNodes: [],
      }],
      observations: [observationNode],
      timeRange: { start: new Date(), end: new Date() },
      githubLinks: [],
    };

    const context = formatNarrativeContext(narrative);

    expect(context.pivots).toHaveLength(1);
    expect(context.pivots[0].revisit_id).toBe(3);
    expect(context.pivots[0].observation_ids).toEqual([2]);
  });
});

// =============================================================================
// filterNarratives Tests
// =============================================================================

describe('filterNarratives', () => {
  const makeNarrative = (
    id: string,
    name: string,
    pivotCount: number,
    linkCount: number
  ): Narrative => ({
    id,
    name,
    root: makeNode(1, 'goal', name),
    nodes: [makeNode(1, 'goal', name)],
    edges: [],
    pivots: Array(pivotCount).fill({
      revisitNode: makeNode(2, 'revisit', 'Revisit'),
      triggeringObservations: [],
      supersededNodes: [],
      newApproachNodes: [],
    }),
    observations: [],
    timeRange: {
      start: new Date(2024, 0, 1),
      end: new Date(2024, 0, 15),
    },
    githubLinks: Array(linkCount).fill({
      type: 'commit' as const,
      identifier: 'abc',
      repo: 'test/repo',
      url: 'https://github.com/test/repo/commit/abc',
      nodeId: 1,
    }),
  });

  it('filters by pivots only', () => {
    const narratives = [
      makeNarrative('1', 'Has Pivots', 2, 0),
      makeNarrative('2', 'No Pivots', 0, 0),
    ];

    const filtered = filterNarratives(narratives, {
      ...DEFAULT_ARCHAEOLOGY_FILTERS,
      pivotsOnly: true,
    });

    expect(filtered).toHaveLength(1);
    expect(filtered[0].name).toBe('Has Pivots');
  });

  it('filters by has GitHub links', () => {
    const narratives = [
      makeNarrative('1', 'Has Links', 0, 3),
      makeNarrative('2', 'No Links', 0, 0),
    ];

    const filtered = filterNarratives(narratives, {
      ...DEFAULT_ARCHAEOLOGY_FILTERS,
      hasGithubLinks: true,
    });

    expect(filtered).toHaveLength(1);
    expect(filtered[0].name).toBe('Has Links');
  });

  it('filters by search query in name', () => {
    const narratives = [
      makeNarrative('1', 'Authentication Flow', 0, 0),
      makeNarrative('2', 'Database Schema', 0, 0),
    ];

    const filtered = filterNarratives(narratives, {
      ...DEFAULT_ARCHAEOLOGY_FILTERS,
      searchQuery: 'auth',
    });

    expect(filtered).toHaveLength(1);
    expect(filtered[0].name).toBe('Authentication Flow');
  });

  it('applies multiple filters together', () => {
    const narratives = [
      makeNarrative('1', 'Auth with Pivots', 2, 3),
      makeNarrative('2', 'Auth no Pivots', 0, 3),
      makeNarrative('3', 'DB with Pivots', 2, 0),
    ];

    const filtered = filterNarratives(narratives, {
      ...DEFAULT_ARCHAEOLOGY_FILTERS,
      pivotsOnly: true,
      hasGithubLinks: true,
      searchQuery: 'auth',
    });

    expect(filtered).toHaveLength(1);
    expect(filtered[0].name).toBe('Auth with Pivots');
  });
});

// =============================================================================
// calculateArchaeologyStats Tests
// =============================================================================

describe('calculateArchaeologyStats', () => {
  it('calculates correct statistics', () => {
    const narratives: Narrative[] = [
      {
        id: '1',
        name: 'Narrative 1',
        root: makeNode(1, 'goal', 'Goal'),
        nodes: [
          makeNode(1, 'goal', 'Goal'),
          makeNode(2, 'decision', 'Decision'),
          makeNode(3, 'action', 'Action'),
        ],
        edges: [],
        pivots: [{
          revisitNode: makeNode(4, 'revisit', 'Revisit'),
          triggeringObservations: [],
          supersededNodes: [],
          newApproachNodes: [],
        }],
        observations: [],
        timeRange: { start: new Date(), end: new Date() },
        githubLinks: [
          { type: 'commit', identifier: 'abc', repo: 'r', url: 'u', nodeId: 1 },
          { type: 'pr', identifier: '42', repo: 'r', url: 'u', nodeId: 2 },
        ],
      },
    ];

    const stats = calculateArchaeologyStats(narratives);

    expect(stats.narrativeCount).toBe(1);
    expect(stats.totalNodes).toBe(3);
    expect(stats.totalPivots).toBe(1);
    expect(stats.totalGithubLinks).toBe(2);
    expect(stats.nodesByType.goal).toBe(1);
    expect(stats.nodesByType.decision).toBe(1);
    expect(stats.nodesByType.action).toBe(1);
  });
});

// =============================================================================
// traceWhyChain Tests
// =============================================================================

describe('traceWhyChain', () => {
  it('traces observations back to root', () => {
    const graphData = makeGraphData(
      [
        makeNode(1, 'goal', 'Goal'),
        makeNode(2, 'observation', 'Observation 1'),
        makeNode(3, 'decision', 'Decision'),
        makeNode(4, 'observation', 'Observation 2'),
        makeNode(5, 'action', 'Action'),
      ],
      [
        makeEdge(1, 1, 2),
        makeEdge(2, 2, 3),
        makeEdge(3, 3, 4),
        makeEdge(4, 4, 5),
      ]
    );

    const observations = traceWhyChain(5, graphData);

    expect(observations).toHaveLength(2);
    // Should be sorted by creation time (oldest first)
    expect(observations[0].id).toBe(2);
    expect(observations[1].id).toBe(4);
  });

  it('returns empty array when no observations found', () => {
    const graphData = makeGraphData(
      [
        makeNode(1, 'goal', 'Goal'),
        makeNode(2, 'decision', 'Decision'),
      ],
      [makeEdge(1, 1, 2)]
    );

    const observations = traceWhyChain(2, graphData);
    expect(observations).toHaveLength(0);
  });

  it('handles cycles gracefully', () => {
    const graphData = makeGraphData(
      [
        makeNode(1, 'decision', 'Decision'),
        makeNode(2, 'observation', 'Observation'),
        makeNode(3, 'action', 'Action'),
      ],
      [
        makeEdge(1, 1, 2),
        makeEdge(2, 2, 3),
        makeEdge(3, 3, 1), // Cycle back
      ]
    );

    // Should not infinite loop
    const observations = traceWhyChain(3, graphData);
    expect(observations).toHaveLength(1);
    expect(observations[0].id).toBe(2);
  });
});
