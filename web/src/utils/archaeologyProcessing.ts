/**
 * Archaeology Processing Utilities
 *
 * Algorithms for building narratives from decision graphs.
 * Pure functions - no side effects, no state mutation.
 */

import type {
  DecisionNode,
  DecisionEdge,
  GraphData,
} from '../types/graph';
import { getCommit } from '../types/graph';
import type {
  Narrative,
  Pivot,
  GithubLink,
  NarrativeContext,
  PivotContext,
  GithubLinkContext,
  ArchaeologyFilters,
} from '../types/archaeology';
import {
  getGithubPr,
  getGithubIssue,
  getGithubRepo,
  getNarrativeName,
  buildCommitUrl,
  buildPrUrl,
  buildIssueUrl,
} from '../types/archaeology';
import { buildAdjacencyLists, type AdjacencyLists } from './graphProcessing';

// =============================================================================
// Constants
// =============================================================================

/** Default repository for GitHub links */
const DEFAULT_REPO = 'notactuallytreyanastasio/deciduous';

// =============================================================================
// Pivot Detection
// =============================================================================

/**
 * Find pivot points in the graph
 *
 * A pivot is detected when:
 * 1. A revisit node exists
 * 2. It has incoming edges from observation nodes (triggering observations)
 * 3. It has outgoing edges to decision/action nodes (new approach)
 *
 * Superseded nodes are found by tracing backwards from the observations.
 */
export function findPivots(graphData: GraphData): Pivot[] {
  const { nodes, edges } = graphData;
  const { outgoing, incoming } = buildAdjacencyLists(nodes, edges);
  const nodeMap = new Map(nodes.map(n => [n.id, n]));

  const pivots: Pivot[] = [];

  // Find all revisit nodes
  const revisitNodes = nodes.filter(n => n.node_type === 'revisit');

  for (const revisitNode of revisitNodes) {
    // Find triggering observations (incoming edges from observations)
    const incomingEdges = incoming.get(revisitNode.id) || [];
    const triggeringObservations = incomingEdges
      .map(({ from }) => nodeMap.get(from))
      .filter((n): n is DecisionNode => n !== undefined && n.node_type === 'observation');

    // Find new approach nodes (outgoing edges to decisions/actions)
    const outgoingEdges = outgoing.get(revisitNode.id) || [];
    const newApproachNodes = outgoingEdges
      .map(({ to }) => nodeMap.get(to))
      .filter((n): n is DecisionNode =>
        n !== undefined &&
        (n.node_type === 'decision' || n.node_type === 'action' || n.node_type === 'goal')
      );

    // Find superseded nodes by tracing back from observations
    const supersededNodes: DecisionNode[] = [];
    const visited = new Set<number>();

    for (const obs of triggeringObservations) {
      const obsIncoming = incoming.get(obs.id) || [];
      for (const { from } of obsIncoming) {
        if (visited.has(from)) continue;
        visited.add(from);

        const node = nodeMap.get(from);
        if (node && node.node_type !== 'observation' && node.id !== revisitNode.id) {
          supersededNodes.push(node);
        }
      }
    }

    // Only create pivot if we have meaningful connections
    if (triggeringObservations.length > 0 || newApproachNodes.length > 0) {
      pivots.push({
        revisitNode,
        triggeringObservations,
        supersededNodes,
        newApproachNodes,
      });
    }
  }

  return pivots;
}

// =============================================================================
// GitHub Link Aggregation
// =============================================================================

/**
 * Get the default repository from graph config
 */
function getDefaultRepo(graphData: GraphData): string {
  return graphData.config?.github?.commit_repo || DEFAULT_REPO;
}

/**
 * Aggregate GitHub links from a set of nodes
 */
export function aggregateGithubLinks(
  nodes: DecisionNode[],
  graphData: GraphData
): GithubLink[] {
  const links: GithubLink[] = [];
  const defaultRepo = getDefaultRepo(graphData);
  const seen = new Set<string>(); // Dedupe by "type:identifier:repo"

  for (const node of nodes) {
    const nodeRepo = getGithubRepo(node) || defaultRepo;

    // Check for commit link
    const commit = getCommit(node);
    if (commit) {
      const key = `commit:${commit}:${nodeRepo}`;
      if (!seen.has(key)) {
        seen.add(key);
        links.push({
          type: 'commit',
          identifier: commit,
          repo: nodeRepo,
          url: buildCommitUrl(commit, nodeRepo),
          nodeId: node.id,
        });
      }
    }

    // Check for PR link
    const pr = getGithubPr(node);
    if (pr !== null) {
      const key = `pr:${pr}:${nodeRepo}`;
      if (!seen.has(key)) {
        seen.add(key);
        links.push({
          type: 'pr',
          identifier: String(pr),
          repo: nodeRepo,
          url: buildPrUrl(pr, nodeRepo),
          nodeId: node.id,
        });
      }
    }

    // Check for issue link
    const issue = getGithubIssue(node);
    if (issue !== null) {
      const key = `issue:${issue}:${nodeRepo}`;
      if (!seen.has(key)) {
        seen.add(key);
        links.push({
          type: 'issue',
          identifier: String(issue),
          repo: nodeRepo,
          url: buildIssueUrl(issue, nodeRepo),
          nodeId: node.id,
        });
      }
    }
  }

  return links;
}

// =============================================================================
// Narrative Building
// =============================================================================

/**
 * Find narrative root nodes
 *
 * Roots are:
 * 1. Goal nodes (primary story starters)
 * 2. Revisit nodes with no incoming edges (orphan pivots)
 */
function findNarrativeRoots(
  nodes: DecisionNode[],
  adjacency: AdjacencyLists
): DecisionNode[] {
  const { incoming } = adjacency;

  return nodes.filter(n => {
    // Goals are always roots
    if (n.node_type === 'goal') return true;

    // Revisits with no incoming edges are orphan pivots (start of a new narrative)
    if (n.node_type === 'revisit') {
      const incomingCount = incoming.get(n.id)?.length ?? 0;
      return incomingCount === 0;
    }

    return false;
  }).sort((a, b) => {
    // Goals first
    if (a.node_type === 'goal' && b.node_type !== 'goal') return -1;
    if (b.node_type === 'goal' && a.node_type !== 'goal') return 1;
    // Then by creation time (oldest first for narratives)
    return new Date(a.created_at).getTime() - new Date(b.created_at).getTime();
  });
}

/**
 * Build narratives from graph data
 *
 * Algorithm:
 * 1. Find narrative roots (goals + orphan revisits)
 * 2. BFS from each root to collect connected nodes
 * 3. For each narrative, detect pivots and extract observations
 * 4. Aggregate GitHub links
 * 5. Generate name from metadata or root title
 */
export function buildNarratives(graphData: GraphData): Narrative[] {
  const { nodes, edges } = graphData;
  const adjacency = buildAdjacencyLists(nodes, edges);
  const { outgoing, incoming } = adjacency;
  const nodeMap = new Map(nodes.map(n => [n.id, n]));

  const narratives: Narrative[] = [];
  const visited = new Set<number>();

  // Find all pivots upfront (we'll associate them with narratives)
  const allPivots = findPivots(graphData);
  const pivotByRevisitId = new Map(allPivots.map(p => [p.revisitNode.id, p]));

  // Find narrative roots
  const roots = findNarrativeRoots(nodes, adjacency);

  for (const root of roots) {
    if (visited.has(root.id)) continue;

    // BFS to collect all connected nodes
    const narrativeNodes: DecisionNode[] = [];
    const narrativeEdges: DecisionEdge[] = [];
    const narrativeEdgeIds = new Set<number>();
    const queue = [root.id];

    while (queue.length > 0) {
      const nodeId = queue.shift()!;
      if (visited.has(nodeId)) continue;
      visited.add(nodeId);

      const node = nodeMap.get(nodeId);
      if (node) {
        narrativeNodes.push(node);
      }

      // Follow outgoing edges
      const outEdges = outgoing.get(nodeId) || [];
      for (const { to, edge } of outEdges) {
        if (!narrativeEdgeIds.has(edge.id)) {
          narrativeEdgeIds.add(edge.id);
          narrativeEdges.push(edge);
        }
        if (!visited.has(to)) {
          queue.push(to);
        }
      }

      // Follow incoming edges (for complete connected component)
      const inEdges = incoming.get(nodeId) || [];
      for (const { from, edge } of inEdges) {
        if (!narrativeEdgeIds.has(edge.id)) {
          narrativeEdgeIds.add(edge.id);
          narrativeEdges.push(edge);
        }
        if (!visited.has(from)) {
          queue.push(from);
        }
      }
    }

    if (narrativeNodes.length === 0) continue;

    // Sort nodes by creation time
    narrativeNodes.sort((a, b) =>
      new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
    );

    // Find pivots within this narrative
    const narrativePivots = narrativeNodes
      .filter(n => n.node_type === 'revisit')
      .map(n => pivotByRevisitId.get(n.id))
      .filter((p): p is Pivot => p !== undefined);

    // Extract observations
    const observations = narrativeNodes.filter(n => n.node_type === 'observation');

    // Calculate time range
    const timestamps = narrativeNodes.map(n => new Date(n.created_at).getTime());
    const startTime = new Date(Math.min(...timestamps));
    const endTime = new Date(Math.max(...timestamps));

    // Aggregate GitHub links
    const githubLinks = aggregateGithubLinks(narrativeNodes, graphData);

    // Generate name
    const narrativeName = getNarrativeName(root) || root.title;

    narratives.push({
      id: root.change_id,
      name: narrativeName,
      root,
      nodes: narrativeNodes,
      edges: narrativeEdges,
      pivots: narrativePivots,
      observations,
      timeRange: { start: startTime, end: endTime },
      githubLinks,
    });
  }

  // Catch any orphaned nodes not connected to roots
  const orphanedNodes = nodes.filter(n => !visited.has(n.id));
  if (orphanedNodes.length > 0) {
    // Group orphans into a single "Miscellaneous" narrative
    orphanedNodes.sort((a, b) =>
      new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
    );

    const timestamps = orphanedNodes.map(n => new Date(n.created_at).getTime());
    const observations = orphanedNodes.filter(n => n.node_type === 'observation');
    const githubLinks = aggregateGithubLinks(orphanedNodes, graphData);

    narratives.push({
      id: 'orphaned',
      name: 'Unconnected Nodes',
      root: orphanedNodes[0],
      nodes: orphanedNodes,
      edges: [],
      pivots: [],
      observations,
      timeRange: {
        start: new Date(Math.min(...timestamps)),
        end: new Date(Math.max(...timestamps)),
      },
      githubLinks,
    });
  }

  // Sort narratives by start time (newest first)
  narratives.sort((a, b) => b.timeRange.start.getTime() - a.timeRange.start.getTime());

  return narratives;
}

// =============================================================================
// Narrative Context Formatting (for API)
// =============================================================================

/**
 * Format a pivot for API context
 */
function formatPivotContext(pivot: Pivot): PivotContext {
  return {
    revisit_id: pivot.revisitNode.id,
    observation_ids: pivot.triggeringObservations.map(n => n.id),
    superseded_ids: pivot.supersededNodes.map(n => n.id),
    new_approach_ids: pivot.newApproachNodes.map(n => n.id),
  };
}

/**
 * Format a GitHub link for API context
 */
function formatGithubLinkContext(link: GithubLink): GithubLinkContext {
  return {
    type: link.type,
    identifier: link.identifier,
    repo: link.repo,
  };
}

/**
 * Format a narrative for API context
 *
 * This creates a serializable structure that can be sent to the backend
 * for Claude prompt building.
 */
export function formatNarrativeContext(narrative: Narrative): NarrativeContext {
  return {
    name: narrative.name,
    root_id: narrative.root.id,
    node_ids: narrative.nodes.map(n => n.id),
    pivots: narrative.pivots.map(formatPivotContext),
    github_links: narrative.githubLinks.map(formatGithubLinkContext),
  };
}

// =============================================================================
// Filtering
// =============================================================================

/**
 * Apply filters to narratives
 */
export function filterNarratives(
  narratives: Narrative[],
  filters: ArchaeologyFilters
): Narrative[] {
  let filtered = narratives;

  // Filter by pivots only
  if (filters.pivotsOnly) {
    filtered = filtered.filter(n => n.pivots.length > 0);
  }

  // Filter by has GitHub links
  if (filters.hasGithubLinks) {
    filtered = filtered.filter(n => n.githubLinks.length > 0);
  }

  // Filter by search query
  if (filters.searchQuery) {
    const query = filters.searchQuery.toLowerCase();
    filtered = filtered.filter(n =>
      n.name.toLowerCase().includes(query) ||
      n.nodes.some(node =>
        node.title.toLowerCase().includes(query) ||
        node.description?.toLowerCase().includes(query)
      )
    );
  }

  // Filter by date range
  if (filters.dateRange.start) {
    const startMs = filters.dateRange.start.getTime();
    filtered = filtered.filter(n => n.timeRange.end.getTime() >= startMs);
  }
  if (filters.dateRange.end) {
    const endMs = filters.dateRange.end.getTime();
    filtered = filtered.filter(n => n.timeRange.start.getTime() <= endMs);
  }

  return filtered;
}

// =============================================================================
// Statistics
// =============================================================================

export interface ArchaeologyStats {
  narrativeCount: number;
  totalNodes: number;
  totalPivots: number;
  totalGithubLinks: number;
  nodesByType: Record<string, number>;
}

/**
 * Calculate archaeology statistics
 */
export function calculateArchaeologyStats(narratives: Narrative[]): ArchaeologyStats {
  const nodesByType: Record<string, number> = {};
  let totalNodes = 0;
  let totalPivots = 0;
  let totalGithubLinks = 0;

  for (const narrative of narratives) {
    totalNodes += narrative.nodes.length;
    totalPivots += narrative.pivots.length;
    totalGithubLinks += narrative.githubLinks.length;

    for (const node of narrative.nodes) {
      nodesByType[node.node_type] = (nodesByType[node.node_type] || 0) + 1;
    }
  }

  return {
    narrativeCount: narratives.length,
    totalNodes,
    totalPivots,
    totalGithubLinks,
    nodesByType,
  };
}

// =============================================================================
// Claude Prompt Generation
// =============================================================================

/**
 * Generate a comprehensive Claude prompt from a narrative
 *
 * Includes all node details formatted for Claude to understand the
 * decision graph context.
 */
export function generateClaudePrompt(
  narrative: Narrative,
  userQuestion: string
): string {
  const lines: string[] = [];

  lines.push('Considering the following graph of decisions:\n');
  lines.push('---\n');

  // Narrative header
  lines.push(`## Narrative: ${narrative.name}`);
  lines.push(`Root: ${narrative.root.node_type} #${narrative.root.id}`);
  lines.push(`Time Range: ${narrative.timeRange.start.toLocaleDateString()} - ${narrative.timeRange.end.toLocaleDateString()}`);
  lines.push(`Total Nodes: ${narrative.nodes.length}`);
  if (narrative.pivots.length > 0) {
    lines.push(`Pivots: ${narrative.pivots.length}`);
  }
  lines.push('');

  // Node details table
  lines.push('### Nodes\n');
  lines.push('| ID | Type | Title | Description | Confidence | Branch | Commit |');
  lines.push('|----|------|-------|-------------|------------|--------|--------|');

  for (const node of narrative.nodes) {
    const meta = node.metadata_json ? JSON.parse(node.metadata_json) : {};
    const confidence = meta.confidence ? `${meta.confidence}%` : '-';
    const branch = meta.branch || '-';
    const commit = meta.commit ? meta.commit.slice(0, 7) : '-';
    const desc = node.description ? node.description.replace(/\|/g, '\\|').replace(/\n/g, ' ').slice(0, 50) : '-';
    const title = node.title.replace(/\|/g, '\\|').replace(/\n/g, ' ');

    lines.push(`| ${node.id} | ${node.node_type} | ${title} | ${desc} | ${confidence} | ${branch} | ${commit} |`);
  }
  lines.push('');

  // Detailed node information
  lines.push('### Node Details\n');
  for (const node of narrative.nodes) {
    const meta = node.metadata_json ? JSON.parse(node.metadata_json) : {};

    lines.push(`#### [${node.node_type.toUpperCase()}] #${node.id}: ${node.title}`);
    if (node.description) {
      lines.push(`**Description:** ${node.description}`);
    }
    if (meta.confidence) {
      lines.push(`**Confidence:** ${meta.confidence}%`);
    }
    if (meta.branch) {
      lines.push(`**Branch:** ${meta.branch}`);
    }
    if (meta.commit) {
      lines.push(`**Commit:** ${meta.commit}`);
    }
    if (meta.files && meta.files.length > 0) {
      lines.push(`**Files:** ${meta.files.join(', ')}`);
    }
    if (meta.prompt) {
      lines.push(`**Original Prompt:** ${meta.prompt}`);
    }
    lines.push(`**Created:** ${new Date(node.created_at).toLocaleString()}`);
    lines.push('');
  }

  // Edge relationships
  if (narrative.edges.length > 0) {
    lines.push('### Relationships\n');
    lines.push('| From | To | Type | Rationale |');
    lines.push('|------|----|----- |-----------|');

    for (const edge of narrative.edges) {
      const rationale = edge.rationale ? edge.rationale.replace(/\|/g, '\\|').replace(/\n/g, ' ') : '-';
      lines.push(`| #${edge.from_node_id} | #${edge.to_node_id} | ${edge.edge_type} | ${rationale} |`);
    }
    lines.push('');
  }

  // Pivots (if any)
  if (narrative.pivots.length > 0) {
    lines.push('### Pivots (Direction Changes)\n');
    for (const pivot of narrative.pivots) {
      lines.push(`**Revisit #${pivot.revisitNode.id}:** ${pivot.revisitNode.title}`);
      if (pivot.triggeringObservations.length > 0) {
        lines.push(`- Triggered by: ${pivot.triggeringObservations.map(n => `#${n.id} (${n.title})`).join(', ')}`);
      }
      if (pivot.supersededNodes.length > 0) {
        lines.push(`- Superseded: ${pivot.supersededNodes.map(n => `#${n.id} (${n.title})`).join(', ')}`);
      }
      if (pivot.newApproachNodes.length > 0) {
        lines.push(`- New approach: ${pivot.newApproachNodes.map(n => `#${n.id} (${n.title})`).join(', ')}`);
      }
      lines.push('');
    }
  }

  // GitHub links
  if (narrative.githubLinks.length > 0) {
    lines.push('### GitHub Links\n');
    for (const link of narrative.githubLinks) {
      lines.push(`- ${link.type}: ${link.identifier} (${link.repo}) - ${link.url}`);
    }
    lines.push('');
  }

  lines.push('---\n');

  // Instructions for Claude
  lines.push('Consider this user\'s question and try to answer it as if you were "in the room" during this development process.\n');
  lines.push('Remember, you can look at more commits and PRs etc to do this, but use this graph as a base for understanding the decision-making process.\n');

  // User question
  lines.push('---\n');
  lines.push('**User Question:**\n');
  lines.push(userQuestion || '(Ask your question about this code...)');

  return lines.join('\n');
}

// =============================================================================
// Trace "Why" Chain
// =============================================================================

/**
 * Trace the "why" chain for a node
 *
 * Returns observations that explain why this node exists,
 * by tracing backwards through the graph.
 */
export function traceWhyChain(
  nodeId: number,
  graphData: GraphData
): DecisionNode[] {
  const { nodes, edges } = graphData;
  const { incoming } = buildAdjacencyLists(nodes, edges);
  const nodeMap = new Map(nodes.map(n => [n.id, n]));

  const observations: DecisionNode[] = [];
  const visited = new Set<number>();
  const queue = [nodeId];

  while (queue.length > 0) {
    const currentId = queue.shift()!;
    if (visited.has(currentId)) continue;
    visited.add(currentId);

    const current = nodeMap.get(currentId);
    if (current && current.node_type === 'observation') {
      observations.push(current);
    }

    // Trace backwards
    const inEdges = incoming.get(currentId) || [];
    for (const { from } of inEdges) {
      if (!visited.has(from)) {
        queue.push(from);
      }
    }
  }

  // Sort by creation time (oldest first - the original reasoning)
  observations.sort((a, b) =>
    new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
  );

  return observations;
}
