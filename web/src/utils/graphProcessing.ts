/**
 * Graph Processing Utilities
 *
 * Core algorithms for chain building, session grouping, and path tracing.
 * These preserve the exact logic from the vanilla JS implementations.
 */

import type {
  DecisionNode,
  DecisionEdge,
  GraphData,
  Chain,
  Session,
  GitCommit,
  TimelineItem,
} from '../types/graph';
import { getCommit } from '../types/graph';

// =============================================================================
// Constants
// =============================================================================

/** Session gap threshold in milliseconds (4 hours) */
export const SESSION_GAP_MS = 4 * 60 * 60 * 1000;

/** Maximum nodes per chain (0 = unlimited, which is the default) */
export const MAX_CHAIN_NODES = 0;

// =============================================================================
// Adjacency Lists
// =============================================================================

export interface AdjacencyLists {
  outgoing: Map<number, Array<{ to: number; edge: DecisionEdge }>>;
  incoming: Map<number, Array<{ from: number; edge: DecisionEdge }>>;
}

/**
 * Build adjacency lists for the graph
 * Matches: docs/demo/index.html buildChainsAndSessions (lines 380-387)
 */
export function buildAdjacencyLists(nodes: DecisionNode[], edges: DecisionEdge[]): AdjacencyLists {
  const outgoing = new Map<number, Array<{ to: number; edge: DecisionEdge }>>();
  const incoming = new Map<number, Array<{ from: number; edge: DecisionEdge }>>();

  // Initialize empty arrays for all nodes
  nodes.forEach(n => {
    outgoing.set(n.id, []);
    incoming.set(n.id, []);
  });

  // Populate adjacency lists
  edges.forEach(e => {
    outgoing.get(e.from_node_id)?.push({ to: e.to_node_id, edge: e });
    incoming.get(e.to_node_id)?.push({ from: e.from_node_id, edge: e });
  });

  return { outgoing, incoming };
}

// =============================================================================
// Chain Building
// =============================================================================

/**
 * Build chains from graph data using BFS from root nodes
 * Matches: docs/demo/index.html buildChainsAndSessions (lines 389-451)
 *
 * Priority: goals first, then decisions with no incoming, then any unvisited
 */
export function buildChains(graphData: GraphData): Chain[] {
  const { nodes, edges } = graphData;
  const { outgoing, incoming } = buildAdjacencyLists(nodes, edges);

  const visited = new Set<number>();
  const chains: Chain[] = [];

  // Find root nodes: goals first, then nodes with no incoming edges
  const roots = nodes
    .filter(n => n.node_type === 'goal' || (incoming.get(n.id)?.length ?? 0) === 0)
    .sort((a, b) => {
      // Goals first
      if (a.node_type === 'goal' && b.node_type !== 'goal') return -1;
      if (b.node_type === 'goal' && a.node_type !== 'goal') return 1;
      // Then by creation time
      return new Date(a.created_at).getTime() - new Date(b.created_at).getTime();
    });

  /**
   * Build a single chain starting from rootId using BFS
   * Traverses ALL connected nodes without any limit
   */
  function buildChain(rootId: number): Chain | null {
    if (visited.has(rootId)) return null;

    const chain: Chain = {
      root: null as unknown as DecisionNode,
      nodes: [],
      edges: [],
    };
    const queue = [rootId];

    // BFS traversal - no node limit, traverse entire connected component
    while (queue.length > 0) {
      const nodeId = queue.shift()!;
      if (visited.has(nodeId)) continue;
      visited.add(nodeId);

      const node = nodes.find(n => n.id === nodeId);
      if (node) {
        chain.nodes.push(node);
        if (!chain.root) chain.root = node;
      }

      // Add outgoing edges and nodes to queue (follow all connections)
      outgoing.get(nodeId)?.forEach(({ to, edge }) => {
        if (!visited.has(to)) {
          queue.push(to);
          chain.edges.push(edge);
        }
      });

      // Also follow incoming edges to ensure we get the full connected component
      incoming.get(nodeId)?.forEach(({ from, edge }) => {
        if (!visited.has(from)) {
          queue.push(from);
          // Only add edge if not already in chain
          if (!chain.edges.some(e => e.id === edge.id)) {
            chain.edges.push(edge);
          }
        }
      });
    }

    // Sort nodes by creation time
    chain.nodes.sort((a, b) =>
      new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
    );

    return chain.nodes.length > 0 ? chain : null;
  }

  // Build chains from roots
  roots.forEach(root => {
    const chain = buildChain(root.id);
    if (chain) chains.push(chain);
  });

  // Catch any orphaned nodes
  nodes.forEach(n => {
    if (!visited.has(n.id)) {
      const chain = buildChain(n.id);
      if (chain) chains.push(chain);
    }
  });

  // Sort chains by first node time (newest first)
  chains.sort((a, b) =>
    new Date(b.nodes[0].created_at).getTime() - new Date(a.nodes[0].created_at).getTime()
  );

  return chains;
}

// =============================================================================
// Session Grouping
// =============================================================================

/**
 * Build sessions by grouping nodes by time proximity
 * Matches: docs/demo/index.html buildChainsAndSessions (lines 453-485)
 *
 * Session gap: 4 hours between nodes
 */
export function buildSessions(nodes: DecisionNode[], chains: Chain[]): Session[] {
  const sortedNodes = [...nodes].sort((a, b) =>
    new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
  );

  const sessions: Session[] = [];
  let currentSession: Session | null = null;

  sortedNodes.forEach(node => {
    const nodeTime = new Date(node.created_at).getTime();

    if (!currentSession || nodeTime - currentSession.endTime > SESSION_GAP_MS) {
      // Start new session
      currentSession = {
        startTime: nodeTime,
        endTime: nodeTime,
        nodes: [node],
        chains: [],
      };
      sessions.push(currentSession);
    } else {
      // Extend current session
      currentSession.endTime = nodeTime;
      currentSession.nodes.push(node);
    }
  });

  // Associate chains with sessions
  chains.forEach(chain => {
    const chainStart = new Date(chain.nodes[0].created_at).getTime();
    const session = sessions.find(
      s => chainStart >= s.startTime && chainStart <= s.endTime + SESSION_GAP_MS
    );
    if (session) session.chains.push(chain);
  });

  // Reverse to show newest first
  sessions.reverse();

  return sessions;
}

// =============================================================================
// Path Tracing
// =============================================================================

/**
 * Trace path from a node back to its root
 * Matches: docs/spelunk-graph.html tracePath (lines 608-627)
 */
export function tracePath(nodeId: number, graphData: GraphData): DecisionNode[] {
  const { nodes, edges } = graphData;
  const path: DecisionNode[] = [];
  const visited = new Set<number>();
  let currentId: number | null = nodeId;

  while (currentId !== null && !visited.has(currentId)) {
    visited.add(currentId);
    const current = nodes.find(n => n.id === currentId);
    if (current) {
      path.unshift(current);
    }

    // Find incoming edge to trace backwards
    const inEdge = edges.find(e => e.to_node_id === currentId);
    currentId = inEdge ? inEdge.from_node_id : null;
  }

  return path;
}

/**
 * Get all descendants of a node
 * Matches: docs/demo/visual-graph.html getDescendants concept
 */
export function getDescendants(nodeId: number, graphData: GraphData): Set<number> {
  const { nodes, edges } = graphData;
  const { outgoing } = buildAdjacencyLists(nodes, edges);

  const descendants = new Set<number>();
  const queue = [nodeId];

  while (queue.length > 0) {
    const current = queue.shift()!;
    if (descendants.has(current)) continue;
    descendants.add(current);

    outgoing.get(current)?.forEach(({ to }) => {
      if (!descendants.has(to)) {
        queue.push(to);
      }
    });
  }

  return descendants;
}

// =============================================================================
// Timeline Merging (for Timeline View)
// =============================================================================

/**
 * Merge decision nodes with git commits into a unified timeline
 * Matches: docs/spelunk-timeline.html mergeTimelines logic
 */
export function buildMergedTimeline(
  nodes: DecisionNode[],
  commits: GitCommit[]
): TimelineItem[] {
  const items: TimelineItem[] = [];

  // Create a map of commit hashes to commits for linking
  const commitMap = new Map(commits.map(c => [c.hash, c]));
  const shortCommitMap = new Map(commits.map(c => [c.short_hash, c]));

  // Add nodes as timeline items
  nodes.forEach(node => {
    const commit = getCommit(node);
    const linkedCommits: GitCommit[] = [];

    if (commit) {
      // Try full hash first, then short hash
      const linkedCommit = commitMap.get(commit) || shortCommitMap.get(commit.slice(0, 7));
      if (linkedCommit) linkedCommits.push(linkedCommit);
    }

    items.push({
      type: 'node',
      timestamp: new Date(node.created_at),
      node,
      linkedCommits,
    });
  });

  // Add commits as timeline items
  commits.forEach(commit => {
    // Find nodes linked to this commit
    const linkedNodes = nodes.filter(n => {
      const nodeCommit = getCommit(n);
      return nodeCommit === commit.hash || nodeCommit?.slice(0, 7) === commit.short_hash;
    });

    items.push({
      type: 'commit',
      timestamp: new Date(commit.date),
      commit,
      linkedNodes,
    });
  });

  // Sort by timestamp (newest first)
  items.sort((a, b) => b.timestamp.getTime() - a.timestamp.getTime());

  return items;
}

// =============================================================================
// Filtering
// =============================================================================

export type NodeFilter = 'all' | DecisionNode['node_type'];

/**
 * Filter nodes by type
 */
export function filterNodesByType(nodes: DecisionNode[], filter: NodeFilter): DecisionNode[] {
  if (filter === 'all') return nodes;
  return nodes.filter(n => n.node_type === filter);
}

/**
 * Search nodes by title and description
 */
export function searchNodes(nodes: DecisionNode[], term: string): DecisionNode[] {
  if (!term) return nodes;
  const lowerTerm = term.toLowerCase();
  return nodes.filter(n =>
    n.title.toLowerCase().includes(lowerTerm) ||
    (n.description?.toLowerCase().includes(lowerTerm) ?? false)
  );
}

// =============================================================================
// Statistics
// =============================================================================

export interface GraphStats {
  nodeCount: number;
  edgeCount: number;
  chainCount: number;
  sessionCount: number;
  linkedCommitCount: number;
  nodesByType: Record<string, number>;
}

/**
 * Calculate graph statistics
 */
export function calculateStats(
  graphData: GraphData,
  chains: Chain[],
  sessions: Session[]
): GraphStats {
  const nodesByType: Record<string, number> = {};
  let linkedCommitCount = 0;

  graphData.nodes.forEach(n => {
    nodesByType[n.node_type] = (nodesByType[n.node_type] || 0) + 1;
    if (getCommit(n)) linkedCommitCount++;
  });

  return {
    nodeCount: graphData.nodes.length,
    edgeCount: graphData.edges.length,
    chainCount: chains.length,
    sessionCount: sessions.length,
    linkedCommitCount,
    nodesByType,
  };
}
