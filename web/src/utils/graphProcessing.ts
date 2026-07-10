/**
 * Graph Processing - pure functions for building narratives and trees
 * from decision graph data.
 *
 * Extracted from App.tsx so they can be unit tested in isolation.
 */

import {
  DecisionNode,
  DecisionEdge,
  GraphData,
  getBranch,
} from '../types/graph';

// =============================================================================
// Types
// =============================================================================

export interface TreeNode {
  node: DecisionNode;
  children: TreeNode[];
  depth: number;
}

export interface Narrative {
  id: string;
  name: string;
  root: DecisionNode;
  nodes: DecisionNode[];
  edges: DecisionEdge[];
  tree: TreeNode;
  nodeCount: number;
  dateRange: { start: Date; end: Date };
  branches: string[];
}

export type NarrativeMode = 'significant' | 'goals' | 'branches' | 'hubs' | 'custom';

// Threshold for "significant" narratives - trees with this many nodes or more
export const SIGNIFICANT_TREE_SIZE = 10;

export interface AdjacencyLists {
  outgoing: Map<number, Array<{ to: number; edge: DecisionEdge }>>;
  incoming: Map<number, Array<{ from: number; edge: DecisionEdge }>>;
}

// =============================================================================
// Graph Processing
// =============================================================================

export function buildAdjacencyLists(edges: DecisionEdge[]): AdjacencyLists {
  const outgoing = new Map<number, Array<{ to: number; edge: DecisionEdge }>>();
  const incoming = new Map<number, Array<{ from: number; edge: DecisionEdge }>>();

  for (const edge of edges) {
    if (!outgoing.has(edge.from_node_id)) {
      outgoing.set(edge.from_node_id, []);
    }
    outgoing.get(edge.from_node_id)!.push({ to: edge.to_node_id, edge });

    if (!incoming.has(edge.to_node_id)) {
      incoming.set(edge.to_node_id, []);
    }
    incoming.get(edge.to_node_id)!.push({ from: edge.from_node_id, edge });
  }

  return { outgoing, incoming };
}

export function buildTree(
  rootId: number,
  outgoing: Map<number, Array<{ to: number; edge: DecisionEdge }>>,
  nodeMap: Map<number, DecisionNode>,
  visited: Set<number>,
  depth: number = 0
): TreeNode | null {
  if (visited.has(rootId)) return null;
  const node = nodeMap.get(rootId);
  if (!node) return null;

  visited.add(rootId);

  const children: TreeNode[] = [];
  const edges = outgoing.get(rootId) || [];

  for (const { to } of edges) {
    const childTree = buildTree(to, outgoing, nodeMap, visited, depth + 1);
    if (childTree) {
      children.push(childTree);
    }
  }

  // Sort children by created_at
  children.sort((a, b) =>
    new Date(a.node.created_at).getTime() - new Date(b.node.created_at).getTime()
  );

  return { node, children, depth };
}

export function collectTreeNodes(tree: TreeNode): DecisionNode[] {
  const nodes: DecisionNode[] = [tree.node];
  for (const child of tree.children) {
    nodes.push(...collectTreeNodes(child));
  }
  return nodes;
}

/**
 * Calculate tree size (total descendants) for a root node using BFS
 */
export function calculateTreeSize(
  rootId: number,
  outgoing: Map<number, Array<{ to: number; edge: DecisionEdge }>>,
): number {
  const visited = new Set<number>();
  const queue = [rootId];
  let head = 0;

  while (head < queue.length) {
    const nodeId = queue[head++];
    if (visited.has(nodeId)) continue;
    visited.add(nodeId);

    const children = outgoing.get(nodeId) || [];
    for (const { to } of children) {
      if (!visited.has(to)) {
        queue.push(to);
      }
    }
  }

  return visited.size;
}

export function buildNarratives(
  graphData: GraphData,
  mode: NarrativeMode
): Narrative[] {
  const { nodes, edges } = graphData;
  const { outgoing } = buildAdjacencyLists(edges);
  const nodeMap = new Map(nodes.map(n => [n.id, n]));

  if (mode === 'branches') {
    // Group by branch
    const branchGroups = new Map<string, DecisionNode[]>();
    for (const node of nodes) {
      const branch = getBranch(node) || 'unknown';
      if (!branchGroups.has(branch)) {
        branchGroups.set(branch, []);
      }
      branchGroups.get(branch)!.push(node);
    }

    return Array.from(branchGroups.entries()).map(([branch, branchNodes]) => {
      const sortedNodes = branchNodes.sort((a, b) =>
        new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
      );
      const root = sortedNodes[0];
      const nodeIds = new Set(branchNodes.map(n => n.id));
      const branchEdges = edges.filter(e =>
        nodeIds.has(e.from_node_id) && nodeIds.has(e.to_node_id)
      );

      // Build tree from first node using a branch-local adjacency list so the
      // tree only contains this branch's nodes (consistent with nodes/edges/nodeCount)
      const { outgoing: branchOutgoing } = buildAdjacencyLists(branchEdges);
      const visited = new Set<number>();
      const tree = buildTree(root.id, branchOutgoing, nodeMap, visited, 0) || {
        node: root,
        children: [],
        depth: 0,
      };

      return {
        id: `branch-${branch}`,
        name: branch,
        root,
        nodes: branchNodes,
        edges: branchEdges,
        tree,
        nodeCount: branchNodes.length,
        dateRange: {
          start: new Date(sortedNodes[0].created_at),
          end: new Date(sortedNodes[sortedNodes.length - 1].created_at),
        },
        branches: [branch],
      };
    }).sort((a, b) => b.dateRange.end.getTime() - a.dateRange.end.getTime()); // Sort by most recent activity
  }

  if (mode === 'hubs') {
    // Find nodes with high out-degree (3+ outgoing edges)
    const hubNodes = nodes.filter(n => {
      const outEdges = outgoing.get(n.id) || [];
      return outEdges.length >= 3;
    });

    // Sort by out-degree descending
    hubNodes.sort((a, b) => {
      const aOut = (outgoing.get(a.id) || []).length;
      const bOut = (outgoing.get(b.id) || []).length;
      return bOut - aOut;
    });

    const visited = new Set<number>();
    const narratives: Narrative[] = [];

    for (const hub of hubNodes) {
      if (visited.has(hub.id)) continue;

      const tree = buildTree(hub.id, outgoing, nodeMap, visited, 0);
      if (!tree) continue;

      const narrativeNodes = collectTreeNodes(tree);
      if (narrativeNodes.length < 3) continue; // Skip tiny hubs

      const nodeIds = new Set(narrativeNodes.map(n => n.id));
      const narrativeEdges = edges.filter(e =>
        nodeIds.has(e.from_node_id) && nodeIds.has(e.to_node_id)
      );

      const sortedNodes = narrativeNodes.sort((a, b) =>
        new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
      );

      const branches = new Set<string>();
      narrativeNodes.forEach(n => {
        const b = getBranch(n);
        if (b) branches.add(b);
      });

      narratives.push({
        id: `hub-${hub.id}`,
        name: hub.title,
        root: hub,
        nodes: narrativeNodes,
        edges: narrativeEdges,
        tree,
        nodeCount: narrativeNodes.length,
        dateRange: {
          start: new Date(sortedNodes[0].created_at),
          end: new Date(sortedNodes[sortedNodes.length - 1].created_at),
        },
        branches: Array.from(branches),
      });
    }

    return narratives.sort((a, b) => b.dateRange.end.getTime() - a.dateRange.end.getTime());
  }

  // For 'significant' and 'goals' modes, start with all goals
  const goals = nodes.filter(n => n.node_type === 'goal');

  // Calculate tree sizes for all goals
  const goalTreeSizes = new Map<number, number>();
  for (const goal of goals) {
    goalTreeSizes.set(goal.id, calculateTreeSize(goal.id, outgoing));
  }

  // Sort goals by most recently created descending
  goals.sort((a, b) =>
    new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
  );

  // For 'significant' mode, only include goals with significant tree sizes
  const roots = mode === 'significant'
    ? goals.filter(g => (goalTreeSizes.get(g.id) || 0) >= SIGNIFICANT_TREE_SIZE)
    : goals;

  const visited = new Set<number>();
  const narratives: Narrative[] = [];

  for (const root of roots) {
    if (visited.has(root.id)) continue;

    const tree = buildTree(root.id, outgoing, nodeMap, visited, 0);
    if (!tree) continue;

    const narrativeNodes = collectTreeNodes(tree);
    const nodeIds = new Set(narrativeNodes.map(n => n.id));
    const narrativeEdges = edges.filter(e =>
      nodeIds.has(e.from_node_id) && nodeIds.has(e.to_node_id)
    );

    const sortedNodes = narrativeNodes.sort((a, b) =>
      new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
    );

    const branches = new Set<string>();
    narrativeNodes.forEach(n => {
      const b = getBranch(n);
      if (b) branches.add(b);
    });

    narratives.push({
      id: `narrative-${root.id}`,
      name: root.title,
      root,
      nodes: narrativeNodes,
      edges: narrativeEdges,
      tree,
      nodeCount: narrativeNodes.length,
      dateRange: {
        start: new Date(sortedNodes[0].created_at),
        end: new Date(sortedNodes[sortedNodes.length - 1].created_at),
      },
      branches: Array.from(branches),
    });
  }

  // Sort by most recent activity (newest first)
  return narratives.sort((a, b) => b.dateRange.end.getTime() - a.dateRange.end.getTime());
}
