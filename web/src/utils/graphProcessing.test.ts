import { describe, it, expect } from 'vitest';
import {
  buildAdjacencyLists,
  buildTree,
  collectTreeNodes,
  calculateTreeSize,
  buildNarratives,
} from './graphProcessing';
import { DecisionNode, DecisionEdge, GraphData, NodeType, EdgeType } from '../types/graph';

// =============================================================================
// Test fixtures
// =============================================================================

let nextEdgeId = 1;

function makeNode(
  id: number,
  node_type: NodeType,
  overrides: Partial<DecisionNode> & { branch?: string } = {}
): DecisionNode {
  const { branch, ...rest } = overrides;
  const created = rest.created_at ?? `2025-01-01T00:00:${String(id).padStart(2, '0')}Z`;
  return {
    id,
    change_id: `change-${id}`,
    node_type,
    title: rest.title ?? `Node ${id}`,
    description: null,
    status: 'active',
    created_at: created,
    updated_at: created,
    metadata_json: branch ? JSON.stringify({ branch }) : null,
    ...rest,
  } as DecisionNode;
}

function makeEdge(from: number, to: number, edge_type: EdgeType = 'leads_to'): DecisionEdge {
  return {
    id: nextEdgeId++,
    from_change_id: `change-${from}`,
    to_change_id: `change-${to}`,
    from_node_id: from,
    to_node_id: to,
    edge_type,
    rationale: null,
    created_at: '2025-01-01T00:00:00Z',
  } as DecisionEdge;
}

function makeGraph(nodes: DecisionNode[], edges: DecisionEdge[]): GraphData {
  return { nodes, edges };
}

// =============================================================================
// buildAdjacencyLists
// =============================================================================

describe('buildAdjacencyLists', () => {
  it('returns empty maps for no edges', () => {
    const { outgoing, incoming } = buildAdjacencyLists([]);
    expect(outgoing.size).toBe(0);
    expect(incoming.size).toBe(0);
  });

  it('indexes edges by from and to node ids', () => {
    const edges = [makeEdge(1, 2), makeEdge(1, 3), makeEdge(2, 3)];
    const { outgoing, incoming } = buildAdjacencyLists(edges);

    expect(outgoing.get(1)!.map(e => e.to)).toEqual([2, 3]);
    expect(outgoing.get(2)!.map(e => e.to)).toEqual([3]);
    expect(outgoing.has(3)).toBe(false);

    expect(incoming.get(3)!.map(e => e.from)).toEqual([1, 2]);
    expect(incoming.get(2)!.map(e => e.from)).toEqual([1]);
    expect(incoming.has(1)).toBe(false);
  });
});

// =============================================================================
// buildTree
// =============================================================================

describe('buildTree', () => {
  it('builds a simple goal chain', () => {
    // goal -> action -> outcome
    const nodes = [makeNode(1, 'goal'), makeNode(2, 'action'), makeNode(3, 'outcome')];
    const edges = [makeEdge(1, 2), makeEdge(2, 3)];
    const { outgoing } = buildAdjacencyLists(edges);
    const nodeMap = new Map(nodes.map(n => [n.id, n]));

    const tree = buildTree(1, outgoing, nodeMap, new Set());
    expect(tree).not.toBeNull();
    expect(tree!.node.id).toBe(1);
    expect(tree!.depth).toBe(0);
    expect(tree!.children).toHaveLength(1);
    expect(tree!.children[0].node.id).toBe(2);
    expect(tree!.children[0].depth).toBe(1);
    expect(tree!.children[0].children[0].node.id).toBe(3);
    expect(tree!.children[0].children[0].depth).toBe(2);
  });

  it('is safe against cycles', () => {
    // 1 -> 2 -> 3 -> 1 (cycle)
    const nodes = [makeNode(1, 'goal'), makeNode(2, 'action'), makeNode(3, 'outcome')];
    const edges = [makeEdge(1, 2), makeEdge(2, 3), makeEdge(3, 1)];
    const { outgoing } = buildAdjacencyLists(edges);
    const nodeMap = new Map(nodes.map(n => [n.id, n]));

    const tree = buildTree(1, outgoing, nodeMap, new Set());
    expect(tree).not.toBeNull();
    expect(collectTreeNodes(tree!).map(n => n.id).sort()).toEqual([1, 2, 3]);
  });

  it('visits multi-parent nodes only once', () => {
    // 1 -> 2, 1 -> 3, 2 -> 4, 3 -> 4 (diamond)
    const nodes = [
      makeNode(1, 'goal'),
      makeNode(2, 'option'),
      makeNode(3, 'option'),
      makeNode(4, 'decision'),
    ];
    const edges = [makeEdge(1, 2), makeEdge(1, 3), makeEdge(2, 4), makeEdge(3, 4)];
    const { outgoing } = buildAdjacencyLists(edges);
    const nodeMap = new Map(nodes.map(n => [n.id, n]));

    const tree = buildTree(1, outgoing, nodeMap, new Set());
    const collected = collectTreeNodes(tree!).map(n => n.id);
    expect(collected).toHaveLength(4);
    expect([...collected].sort()).toEqual([1, 2, 3, 4]);
  });

  it('returns null for unknown or already-visited roots', () => {
    const nodes = [makeNode(1, 'goal')];
    const nodeMap = new Map(nodes.map(n => [n.id, n]));
    expect(buildTree(99, new Map(), nodeMap, new Set())).toBeNull();
    expect(buildTree(1, new Map(), nodeMap, new Set([1]))).toBeNull();
  });

  it('sorts children by created_at ascending', () => {
    const nodes = [
      makeNode(1, 'goal'),
      makeNode(2, 'action', { created_at: '2025-01-02T00:00:00Z' }),
      makeNode(3, 'action', { created_at: '2025-01-01T00:00:00Z' }),
    ];
    const edges = [makeEdge(1, 2), makeEdge(1, 3)];
    const { outgoing } = buildAdjacencyLists(edges);
    const nodeMap = new Map(nodes.map(n => [n.id, n]));

    const tree = buildTree(1, outgoing, nodeMap, new Set());
    expect(tree!.children.map(c => c.node.id)).toEqual([3, 2]);
  });
});

// =============================================================================
// calculateTreeSize
// =============================================================================

describe('calculateTreeSize', () => {
  it('counts a single node with no edges', () => {
    expect(calculateTreeSize(1, new Map())).toBe(1);
  });

  it('counts all reachable descendants in a chain', () => {
    const edges = [makeEdge(1, 2), makeEdge(2, 3), makeEdge(3, 4)];
    const { outgoing } = buildAdjacencyLists(edges);
    expect(calculateTreeSize(1, outgoing)).toBe(4);
    expect(calculateTreeSize(2, outgoing)).toBe(3);
    expect(calculateTreeSize(4, outgoing)).toBe(1);
  });

  it('counts diamond (multi-parent) nodes once', () => {
    const edges = [makeEdge(1, 2), makeEdge(1, 3), makeEdge(2, 4), makeEdge(3, 4)];
    const { outgoing } = buildAdjacencyLists(edges);
    expect(calculateTreeSize(1, outgoing)).toBe(4);
  });

  it('terminates and counts correctly in the presence of cycles', () => {
    const edges = [makeEdge(1, 2), makeEdge(2, 1), makeEdge(2, 3)];
    const { outgoing } = buildAdjacencyLists(edges);
    expect(calculateTreeSize(1, outgoing)).toBe(3);
  });
});

// =============================================================================
// buildNarratives
// =============================================================================

describe('buildNarratives', () => {
  it('returns an empty list for an empty graph', () => {
    expect(buildNarratives(makeGraph([], []), 'goals')).toEqual([]);
    expect(buildNarratives(makeGraph([], []), 'branches')).toEqual([]);
    expect(buildNarratives(makeGraph([], []), 'hubs')).toEqual([]);
    expect(buildNarratives(makeGraph([], []), 'significant')).toEqual([]);
  });

  it('builds one narrative per goal in goals mode', () => {
    const nodes = [
      makeNode(1, 'goal'),
      makeNode(2, 'action'),
      makeNode(3, 'outcome'),
      makeNode(4, 'goal'),
    ];
    const edges = [makeEdge(1, 2), makeEdge(2, 3)];
    const narratives = buildNarratives(makeGraph(nodes, edges), 'goals');

    expect(narratives).toHaveLength(2);
    const main = narratives.find(n => n.root.id === 1)!;
    expect(main.nodeCount).toBe(3);
    expect(main.nodes.map(n => n.id).sort()).toEqual([1, 2, 3]);
    expect(main.edges).toHaveLength(2);
    expect(collectTreeNodes(main.tree)).toHaveLength(3);

    const solo = narratives.find(n => n.root.id === 4)!;
    expect(solo.nodeCount).toBe(1);
  });

  it('only includes goals with >= SIGNIFICANT_TREE_SIZE nodes in significant mode', () => {
    // Goal 1 with 10 descendants (11 total), goal 100 alone
    const nodes: DecisionNode[] = [makeNode(1, 'goal'), makeNode(100, 'goal')];
    const edges: DecisionEdge[] = [];
    for (let i = 2; i <= 11; i++) {
      nodes.push(makeNode(i, 'action'));
      edges.push(makeEdge(i - 1, i));
    }

    const narratives = buildNarratives(makeGraph(nodes, edges), 'significant');
    expect(narratives).toHaveLength(1);
    expect(narratives[0].root.id).toBe(1);
    expect(narratives[0].nodeCount).toBe(11);
  });

  it('groups nodes by branch in branches mode', () => {
    const nodes = [
      makeNode(1, 'goal', { branch: 'main' }),
      makeNode(2, 'action', { branch: 'main' }),
      makeNode(3, 'goal', { branch: 'feature' }),
      makeNode(4, 'action' /* no branch -> 'unknown' */),
    ];
    const edges = [makeEdge(1, 2)];
    const narratives = buildNarratives(makeGraph(nodes, edges), 'branches');

    expect(narratives).toHaveLength(3);
    const names = narratives.map(n => n.name).sort();
    expect(names).toEqual(['feature', 'main', 'unknown']);

    const main = narratives.find(n => n.name === 'main')!;
    expect(main.nodeCount).toBe(2);
    expect(main.edges).toHaveLength(1);
  });

  it('keeps branch trees consistent with branch nodes/edges (no cross-branch leakage)', () => {
    // Branch A: 1 -> 2. Node 2 also links to node 3 which is on branch B.
    // The branch A tree must NOT contain node 3.
    const nodes = [
      makeNode(1, 'goal', { branch: 'a' }),
      makeNode(2, 'action', { branch: 'a' }),
      makeNode(3, 'outcome', { branch: 'b' }),
    ];
    const edges = [makeEdge(1, 2), makeEdge(2, 3)];
    const narratives = buildNarratives(makeGraph(nodes, edges), 'branches');

    const branchA = narratives.find(n => n.name === 'a')!;
    expect(branchA.nodeCount).toBe(2);
    expect(branchA.edges).toHaveLength(1); // only 1 -> 2; 2 -> 3 crosses branches

    const treeIds = collectTreeNodes(branchA.tree).map(n => n.id);
    expect(treeIds).not.toContain(3);
    expect(treeIds.sort()).toEqual([1, 2]);

    // Tree node count matches narrative nodeCount (the regression this guards)
    expect(treeIds).toHaveLength(branchA.nodeCount);

    const branchB = narratives.find(n => n.name === 'b')!;
    expect(branchB.nodeCount).toBe(1);
    expect(collectTreeNodes(branchB.tree).map(n => n.id)).toEqual([3]);
  });

  it('builds hub narratives for nodes with 3+ outgoing edges', () => {
    const nodes = [
      makeNode(1, 'decision'),
      makeNode(2, 'action'),
      makeNode(3, 'action'),
      makeNode(4, 'action'),
      makeNode(5, 'goal'), // not a hub
    ];
    const edges = [makeEdge(1, 2), makeEdge(1, 3), makeEdge(1, 4)];
    const narratives = buildNarratives(makeGraph(nodes, edges), 'hubs');

    expect(narratives).toHaveLength(1);
    expect(narratives[0].root.id).toBe(1);
    expect(narratives[0].nodeCount).toBe(4);
  });

  it('sorts narratives by most recent activity (newest first)', () => {
    const nodes = [
      makeNode(1, 'goal', { created_at: '2025-01-01T00:00:00Z' }),
      makeNode(2, 'goal', { created_at: '2025-06-01T00:00:00Z' }),
    ];
    const narratives = buildNarratives(makeGraph(nodes, []), 'goals');
    expect(narratives.map(n => n.root.id)).toEqual([2, 1]);
  });
});
