import { useState, useEffect, useMemo, useCallback, useRef, MouseEvent as ReactMouseEvent } from 'react';
import * as d3 from 'd3';
import {
  DecisionNode,
  DecisionEdge,
  GraphData,
  NodeType,
  EdgeType,
  parseMetadata,
  getConfidence,
  getBranch,
  getCommit,
  getPrompt,
  getFiles,
  getIncomingEdges,
  getOutgoingEdges,
} from './types/graph';

// =============================================================================
// Theme - Dark purple like beads
// =============================================================================

const THEME = {
  bg: '#1a1625',
  bgLight: '#252036',
  bgHover: '#2d2747',
  bgSelected: '#3d3560',
  border: '#3d3560',
  text: '#e0dce8',
  textMuted: '#8b85a0',
  textDim: '#6b6580',

  // Type colors
  goal: '#f59e0b',
  decision: '#8b5cf6',
  action: '#3b82f6',
  outcome: '#10b981',
  observation: '#06b6d4',
  option: '#ec4899',
  revisit: '#ef4444',

  // Status colors
  active: '#10b981',
  completed: '#6b7280',
  pending: '#8b5cf6',

  // Confidence
  confHigh: '#10b981',
  confMed: '#f59e0b',
  confLow: '#ef4444',
};

const TYPE_ABBREV: Record<NodeType, string> = {
  goal: 'GOAL',
  decision: 'DCSN',
  action: 'ACTN',
  outcome: 'OUTC',
  observation: 'OBSV',
  option: 'OPTN',
  revisit: 'RVST',
};

// =============================================================================
// Types
// =============================================================================

interface TreeNode {
  node: DecisionNode;
  children: TreeNode[];
  depth: number;
}

interface Narrative {
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

type NarrativeMode = 'significant' | 'goals' | 'branches' | 'hubs' | 'custom';

// Threshold for "significant" narratives - trees with this many nodes or more
const SIGNIFICANT_TREE_SIZE = 10;

interface AdjacencyLists {
  outgoing: Map<number, Array<{ to: number; edge: DecisionEdge }>>;
  incoming: Map<number, Array<{ from: number; edge: DecisionEdge }>>;
}

// =============================================================================
// Graph Processing
// =============================================================================

function buildAdjacencyLists(edges: DecisionEdge[]): AdjacencyLists {
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

function buildTree(
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

function collectTreeNodes(tree: TreeNode): DecisionNode[] {
  const nodes: DecisionNode[] = [tree.node];
  for (const child of tree.children) {
    nodes.push(...collectTreeNodes(child));
  }
  return nodes;
}

/**
 * Calculate tree size (total descendants) for a root node using BFS
 */
function calculateTreeSize(
  rootId: number,
  outgoing: Map<number, Array<{ to: number; edge: DecisionEdge }>>,
): number {
  const visited = new Set<number>();
  const queue = [rootId];

  while (queue.length > 0) {
    const nodeId = queue.shift()!;
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

function buildNarratives(
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

      // Build tree from first node
      const visited = new Set<number>();
      const tree = buildTree(root.id, outgoing, nodeMap, visited, 0) || {
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
    }).sort((a, b) => b.nodeCount - a.nodeCount); // Sort by size for branches
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

    return narratives.sort((a, b) => b.nodeCount - a.nodeCount);
  }

  // For 'significant' and 'goals' modes, start with all goals
  const goals = nodes.filter(n => n.node_type === 'goal');

  // Calculate tree sizes for all goals
  const goalTreeSizes = new Map<number, number>();
  for (const goal of goals) {
    goalTreeSizes.set(goal.id, calculateTreeSize(goal.id, outgoing));
  }

  // Sort goals by tree size descending
  goals.sort((a, b) => {
    const sizeA = goalTreeSizes.get(a.id) || 0;
    const sizeB = goalTreeSizes.get(b.id) || 0;
    return sizeB - sizeA;
  });

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

  // Sort by tree size (largest narratives first)
  return narratives.sort((a, b) => b.nodeCount - a.nodeCount);
}

// =============================================================================
// Data Loading
// =============================================================================

async function loadGraphData(): Promise<GraphData | null> {
  const paths = ['/api/graph', './graph-data.json', '/graph-data.json'];

  for (const path of paths) {
    try {
      const resp = await fetch(path);
      if (resp.ok) {
        const data = await resp.json();
        if (data.ok && data.data) {
          return data.data as GraphData;
        }
        if (data.nodes) {
          return data as GraphData;
        }
      }
    } catch {
      continue;
    }
  }
  return null;
}

// =============================================================================
// Copy Button Component
// =============================================================================

function CopyButton({ text, label }: { text: string; label?: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(async (e: React.MouseEvent) => {
    e.stopPropagation();
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }, [text]);

  return (
    <button
      onClick={handleCopy}
      title={`Copy ${label || 'to clipboard'}`}
      style={{
        background: 'none',
        border: 'none',
        cursor: 'pointer',
        padding: '2px 4px',
        fontSize: '12px',
        color: copied ? THEME.confHigh : THEME.textMuted,
        opacity: copied ? 1 : 0.7,
      }}
    >
      {copied ? '✓' : '📋'}
    </button>
  );
}

// =============================================================================
// GitHub Link Component
// =============================================================================

function GitHubLink({ type, value, repo }: { type: 'commit' | 'pr' | 'issue'; value: string; repo?: string }) {
  const repoPath = repo || 'notactuallytreyanastasio/deciduous';
  let url = '';
  let display = '';

  if (type === 'commit') {
    url = `https://github.com/${repoPath}/commit/${value}`;
    display = value.slice(0, 7);
  } else if (type === 'pr') {
    url = `https://github.com/${repoPath}/pull/${value}`;
    display = `#${value}`;
  } else if (type === 'issue') {
    url = `https://github.com/${repoPath}/issues/${value}`;
    display = `#${value}`;
  }

  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: '4px' }}>
      <a
        href={url}
        target="_blank"
        rel="noopener noreferrer"
        style={{ color: THEME.action, textDecoration: 'none' }}
        onClick={(e) => e.stopPropagation()}
      >
        {display} ↗
      </a>
      <CopyButton text={value} label={type} />
    </span>
  );
}

// =============================================================================
// Tree Row Component
// =============================================================================

interface TreeRowProps {
  treeNode: TreeNode;
  isSelected: boolean;
  expandedNodes: Set<number>;
  onSelect: (id: number) => void;
  onToggle: (id: number) => void;
  edges: DecisionEdge[];
}

function TreeRow({ treeNode, isSelected, expandedNodes, onSelect, onToggle, edges }: TreeRowProps) {
  const { node, children, depth } = treeNode;
  const hasChildren = children.length > 0;
  const isExpanded = expandedNodes.has(node.id);

  const confidence = getConfidence(node);
  const typeColor = THEME[node.node_type as keyof typeof THEME] || THEME.text;
  const confColor = confidence !== null
    ? confidence >= 70 ? THEME.confHigh : confidence >= 40 ? THEME.confMed : THEME.confLow
    : THEME.textDim;

  const incoming = getIncomingEdges(node.id, edges);
  const outgoing = getOutgoingEdges(node.id, edges);

  return (
    <>
      <div
        onClick={() => onSelect(node.id)}
        style={{
          display: 'grid',
          gridTemplateColumns: '24px 60px 50px 50px 1fr',
          gap: '8px',
          padding: '6px 12px',
          paddingLeft: `${12 + depth * 20}px`,
          cursor: 'pointer',
          backgroundColor: isSelected ? THEME.bgSelected : 'transparent',
          borderLeft: isSelected ? `3px solid ${typeColor}` : '3px solid transparent',
          fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
          fontSize: '13px',
          transition: 'background-color 0.1s',
        }}
        onMouseEnter={(e) => {
          if (!isSelected) e.currentTarget.style.backgroundColor = THEME.bgHover;
        }}
        onMouseLeave={(e) => {
          if (!isSelected) e.currentTarget.style.backgroundColor = 'transparent';
        }}
      >
        {/* Expand/Collapse */}
        <span
          onClick={(e) => {
            e.stopPropagation();
            if (hasChildren) onToggle(node.id);
          }}
          style={{
            color: THEME.textDim,
            cursor: hasChildren ? 'pointer' : 'default',
            userSelect: 'none',
          }}
        >
          {hasChildren ? (isExpanded ? '▼' : '▶') : '·'}
        </span>

        {/* TYPE */}
        <span style={{ color: typeColor, fontWeight: 600 }}>
          {TYPE_ABBREV[node.node_type]}
        </span>

        {/* CONF */}
        <span style={{ color: confColor }}>
          {confidence !== null ? `${confidence}%` : '--'}
        </span>

        {/* ID */}
        <span style={{ color: THEME.textMuted }}>
          #{node.id}
        </span>

        {/* TITLE */}
        <span style={{
          color: THEME.text,
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
        }}>
          <span style={{ color: THEME.textDim, marginRight: '4px', fontSize: '11px' }}>
            {incoming.length > 0 && `←${incoming.length}`}
            {outgoing.length > 0 && `→${outgoing.length}`}
          </span>
          {node.title}
        </span>
      </div>

      {/* Render children if expanded */}
      {isExpanded && children.map(child => (
        <TreeRow
          key={child.node.id}
          treeNode={child}
          isSelected={isSelected}
          expandedNodes={expandedNodes}
          onSelect={onSelect}
          onToggle={onToggle}
          edges={edges}
        />
      ))}
    </>
  );
}

// =============================================================================
// Narrative Card Component
// =============================================================================

interface NarrativeCardProps {
  narrative: Narrative;
  onClick: () => void;
}

function NarrativeCard({ narrative, onClick }: NarrativeCardProps) {
  const typeColor = THEME[narrative.root.node_type as keyof typeof THEME] || THEME.text;

  return (
    <div
      onClick={onClick}
      style={{
        padding: '16px',
        margin: '8px 12px',
        backgroundColor: THEME.bgLight,
        borderRadius: '8px',
        cursor: 'pointer',
        borderLeft: `4px solid ${typeColor}`,
        transition: 'background-color 0.1s',
      }}
      onMouseEnter={(e) => e.currentTarget.style.backgroundColor = THEME.bgHover}
      onMouseLeave={(e) => e.currentTarget.style.backgroundColor = THEME.bgLight}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '8px' }}>
        <span style={{ color: typeColor, fontWeight: 600, fontSize: '11px' }}>
          {TYPE_ABBREV[narrative.root.node_type]}
        </span>
        <span style={{ color: THEME.textMuted, fontSize: '12px' }}>
          #{narrative.root.id}
        </span>
      </div>

      <h3 style={{
        color: THEME.text,
        margin: 0,
        fontSize: '15px',
        fontWeight: 500,
        marginBottom: '8px',
      }}>
        {narrative.name}
      </h3>

      <div style={{
        display: 'flex',
        gap: '16px',
        fontSize: '12px',
        color: THEME.textMuted,
      }}>
        <span>{narrative.nodeCount} nodes</span>
        <span>{narrative.branches.join(', ')}</span>
        <span>{narrative.dateRange.start.toLocaleDateString()}</span>
      </div>
    </div>
  );
}

// =============================================================================
// Detail Panel Component
// =============================================================================

interface DetailPanelProps {
  node: DecisionNode | null;
  narrative: Narrative | null;
  edges: DecisionEdge[];
  nodes: DecisionNode[];
  graphData: GraphData | null;
  onSelectNode: (id: number) => void;
}

function DetailPanel({ node, narrative, edges, nodes, graphData, onSelectNode }: DetailPanelProps) {
  if (!node && !narrative) {
    return (
      <div style={{
        padding: '20px',
        color: THEME.textMuted,
        fontStyle: 'italic',
      }}>
        Select a narrative or node to view details
      </div>
    );
  }

  // Show narrative summary if no specific node selected
  if (narrative && !node) {
    return (
      <div style={{
        padding: '20px',
        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
        fontSize: '13px',
        overflow: 'auto',
        height: '100%',
      }}>
        <div style={{
          color: THEME[narrative.root.node_type as keyof typeof THEME],
          fontSize: '11px',
          fontWeight: 600,
          letterSpacing: '1px',
          marginBottom: '4px',
        }}>
          NARRATIVE
        </div>
        <h2 style={{ color: THEME.text, margin: 0, fontSize: '18px', marginBottom: '16px' }}>
          {narrative.name}
        </h2>

        <div style={{
          padding: '12px',
          backgroundColor: THEME.bgLight,
          borderRadius: '6px',
          marginBottom: '16px',
        }}>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '8px' }}>
            <div>
              <div style={{ color: THEME.textMuted, fontSize: '11px' }}>Nodes</div>
              <div style={{ color: THEME.text, fontSize: '18px' }}>{narrative.nodeCount}</div>
            </div>
            <div>
              <div style={{ color: THEME.textMuted, fontSize: '11px' }}>Branches</div>
              <div style={{ color: THEME.decision }}>{narrative.branches.join(', ') || 'none'}</div>
            </div>
            <div>
              <div style={{ color: THEME.textMuted, fontSize: '11px' }}>Started</div>
              <div style={{ color: THEME.text }}>{narrative.dateRange.start.toLocaleString()}</div>
            </div>
            <div>
              <div style={{ color: THEME.textMuted, fontSize: '11px' }}>Last Activity</div>
              <div style={{ color: THEME.text }}>{narrative.dateRange.end.toLocaleString()}</div>
            </div>
          </div>
        </div>

        {/* Node type breakdown */}
        <div style={{ marginBottom: '16px' }}>
          <div style={{ color: THEME.textMuted, fontSize: '11px', marginBottom: '8px' }}>
            COMPOSITION
          </div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px' }}>
            {(['goal', 'decision', 'action', 'outcome', 'observation', 'option', 'revisit'] as NodeType[]).map(type => {
              const count = narrative.nodes.filter(n => n.node_type === type).length;
              if (count === 0) return null;
              return (
                <span key={type} style={{
                  padding: '4px 8px',
                  backgroundColor: THEME.bg,
                  borderRadius: '4px',
                  fontSize: '12px',
                }}>
                  <span style={{ color: THEME[type] }}>{TYPE_ABBREV[type]}</span>
                  <span style={{ color: THEME.textMuted, marginLeft: '4px' }}>{count}</span>
                </span>
              );
            })}
          </div>
        </div>
      </div>
    );
  }

  if (!node) return null;

  const metadata = parseMetadata(node.metadata_json);
  const confidence = getConfidence(node);
  const branch = getBranch(node);
  const commit = getCommit(node);
  const prompt = getPrompt(node);
  const files = getFiles(node);

  const incoming = getIncomingEdges(node.id, edges);
  const outgoing = getOutgoingEdges(node.id, edges);

  const typeColor = THEME[node.node_type as keyof typeof THEME] || THEME.text;
  const nodeById = (id: number) => nodes.find(n => n.id === id);

  const repo = graphData?.config?.github?.commit_repo || 'notactuallytreyanastasio/deciduous';

  return (
    <div style={{
      padding: '20px',
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
      fontSize: '13px',
      overflow: 'auto',
      height: '100%',
    }}>
      {/* Header */}
      <div style={{ marginBottom: '20px' }}>
        <div style={{
          color: typeColor,
          fontSize: '11px',
          fontWeight: 600,
          letterSpacing: '1px',
          marginBottom: '4px',
        }}>
          {node.node_type.toUpperCase()}
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <h2 style={{ color: THEME.text, margin: 0, fontSize: '18px', fontWeight: 500 }}>
            {node.title}
          </h2>
          <CopyButton text={node.title} label="title" />
        </div>
      </div>

      {/* Metadata Grid */}
      <div style={{
        display: 'grid',
        gridTemplateColumns: 'auto 1fr',
        gap: '8px 16px',
        marginBottom: '20px',
        padding: '12px',
        backgroundColor: THEME.bgLight,
        borderRadius: '6px',
      }}>
        <span style={{ color: THEME.textMuted }}>ID</span>
        <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
          <span style={{ color: THEME.text }}>#{node.id}</span>
          <CopyButton text={String(node.id)} label="ID" />
        </span>

        <span style={{ color: THEME.textMuted }}>Change ID</span>
        <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
          <span style={{ color: THEME.textDim, fontSize: '11px' }}>{node.change_id.slice(0, 8)}</span>
          <CopyButton text={node.change_id} label="change ID" />
        </span>

        <span style={{ color: THEME.textMuted }}>Status</span>
        <span style={{ color: THEME[node.status as keyof typeof THEME] || THEME.text }}>
          {node.status}
        </span>

        {confidence !== null && <>
          <span style={{ color: THEME.textMuted }}>Confidence</span>
          <span style={{
            color: confidence >= 70 ? THEME.confHigh : confidence >= 40 ? THEME.confMed : THEME.confLow
          }}>
            {confidence}%
          </span>
        </>}

        {branch && <>
          <span style={{ color: THEME.textMuted }}>Branch</span>
          <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
            <span style={{ color: THEME.decision }}>{branch}</span>
            <CopyButton text={branch} label="branch" />
          </span>
        </>}

        {commit && <>
          <span style={{ color: THEME.textMuted }}>Commit</span>
          <GitHubLink type="commit" value={commit} repo={repo} />
        </>}

        {metadata?.github_pr && <>
          <span style={{ color: THEME.textMuted }}>PR</span>
          <GitHubLink type="pr" value={String(metadata.github_pr)} repo={repo} />
        </>}

        {metadata?.github_issue && <>
          <span style={{ color: THEME.textMuted }}>Issue</span>
          <GitHubLink type="issue" value={String(metadata.github_issue)} repo={repo} />
        </>}

        <span style={{ color: THEME.textMuted }}>Created</span>
        <span style={{ color: THEME.text }}>
          {new Date(node.created_at).toLocaleString()}
        </span>

        <span style={{ color: THEME.textMuted }}>Updated</span>
        <span style={{ color: THEME.text }}>
          {new Date(node.updated_at).toLocaleString()}
        </span>
      </div>

      {/* Files */}
      {files && files.length > 0 && (
        <div style={{ marginBottom: '20px' }}>
          <div style={{
            color: THEME.textMuted,
            fontSize: '11px',
            marginBottom: '8px',
            letterSpacing: '0.5px',
          }}>
            FILES
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
            {files.map((file, i) => (
              <div key={i} style={{
                display: 'flex',
                alignItems: 'center',
                gap: '4px',
                padding: '4px 8px',
                backgroundColor: THEME.bgLight,
                borderRadius: '4px',
              }}>
                <span style={{ color: THEME.text, fontSize: '12px' }}>{file}</span>
                <CopyButton text={file} label="file path" />
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Description */}
      {node.description && (
        <div style={{ marginBottom: '20px' }}>
          <div style={{
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
            color: THEME.textMuted,
            fontSize: '11px',
            marginBottom: '8px',
            letterSpacing: '0.5px',
          }}>
            DESCRIPTION
            <CopyButton text={node.description} label="description" />
          </div>
          <div style={{
            color: THEME.text,
            whiteSpace: 'pre-wrap',
            lineHeight: 1.5,
          }}>
            {node.description}
          </div>
        </div>
      )}

      {/* Prompt */}
      {prompt && (
        <div style={{ marginBottom: '20px' }}>
          <div style={{
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
            color: THEME.textMuted,
            fontSize: '11px',
            marginBottom: '8px',
            letterSpacing: '0.5px',
          }}>
            USER PROMPT
            <CopyButton text={prompt} label="prompt" />
          </div>
          <div style={{
            color: THEME.text,
            whiteSpace: 'pre-wrap',
            lineHeight: 1.5,
            padding: '12px',
            backgroundColor: THEME.bgLight,
            borderRadius: '6px',
            borderLeft: `3px solid ${THEME.goal}`,
            maxHeight: '300px',
            overflow: 'auto',
          }}>
            {prompt}
          </div>
        </div>
      )}

      {/* Connections */}
      {(incoming.length > 0 || outgoing.length > 0) && (
        <div style={{ marginBottom: '20px' }}>
          <div style={{
            color: THEME.textMuted,
            fontSize: '11px',
            marginBottom: '8px',
            letterSpacing: '0.5px',
          }}>
            CONNECTIONS
          </div>

          {incoming.length > 0 && (
            <div style={{ marginBottom: '12px' }}>
              <div style={{ color: THEME.textDim, fontSize: '11px', marginBottom: '4px' }}>
                Incoming ({incoming.length})
              </div>
              {incoming.map(edge => {
                const fromNode = nodeById(edge.from_node_id);
                if (!fromNode) return null;
                return (
                  <div
                    key={edge.id}
                    onClick={() => onSelectNode(edge.from_node_id)}
                    style={{
                      padding: '6px 8px',
                      cursor: 'pointer',
                      borderRadius: '4px',
                      marginBottom: '4px',
                      backgroundColor: THEME.bgLight,
                    }}
                  >
                    <span style={{ color: THEME[fromNode.node_type as keyof typeof THEME] }}>
                      {TYPE_ABBREV[fromNode.node_type]}
                    </span>
                    <span style={{ color: THEME.textMuted, margin: '0 8px' }}>#{fromNode.id}</span>
                    <span style={{ color: THEME.text }}>{fromNode.title}</span>
                    {edge.rationale && (
                      <div style={{ color: THEME.textDim, fontSize: '11px', marginTop: '2px' }}>
                        "{edge.rationale}"
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}

          {outgoing.length > 0 && (
            <div>
              <div style={{ color: THEME.textDim, fontSize: '11px', marginBottom: '4px' }}>
                Outgoing ({outgoing.length})
              </div>
              {outgoing.map(edge => {
                const toNode = nodeById(edge.to_node_id);
                if (!toNode) return null;
                return (
                  <div
                    key={edge.id}
                    onClick={() => onSelectNode(edge.to_node_id)}
                    style={{
                      padding: '6px 8px',
                      cursor: 'pointer',
                      borderRadius: '4px',
                      marginBottom: '4px',
                      backgroundColor: THEME.bgLight,
                    }}
                  >
                    <span style={{ color: THEME[toNode.node_type as keyof typeof THEME] }}>
                      {TYPE_ABBREV[toNode.node_type]}
                    </span>
                    <span style={{ color: THEME.textMuted, margin: '0 8px' }}>#{toNode.id}</span>
                    <span style={{ color: THEME.text }}>{toNode.title}</span>
                    {edge.rationale && (
                      <div style={{ color: THEME.textDim, fontSize: '11px', marginTop: '2px' }}>
                        "{edge.rationale}"
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}

      {/* Raw Metadata (for debugging/completeness) */}
      {metadata && Object.keys(metadata).length > 0 && (
        <div style={{ marginBottom: '20px' }}>
          <div style={{
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
            color: THEME.textMuted,
            fontSize: '11px',
            marginBottom: '8px',
            letterSpacing: '0.5px',
          }}>
            RAW METADATA
            <CopyButton text={JSON.stringify(metadata, null, 2)} label="metadata JSON" />
          </div>
          <pre style={{
            color: THEME.textDim,
            fontSize: '11px',
            padding: '8px',
            backgroundColor: THEME.bgLight,
            borderRadius: '4px',
            overflow: 'auto',
            maxHeight: '150px',
          }}>
            {JSON.stringify(metadata, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

// =============================================================================
// D3 Flow Graph Component (Mermaid-style DAG layout)
// =============================================================================

interface LayoutNode {
  id: number;
  node: DecisionNode;
  x: number;
  y: number;
  depth: number;
}

// Edge type to dash pattern mapping
const EDGE_DASH_PATTERNS: Record<EdgeType, string> = {
  leads_to: '',           // solid
  requires: '5,5',        // dashed
  chosen: '2,2',          // dotted
  rejected: '8,4,2,4',    // dash-dot
  blocks: '10,5',         // long dash
  enables: '3,3',         // short dash
};

interface D3GraphProps {
  narrative: Narrative;
  selectedNodeId: number | null;
  onSelectNode: (id: number) => void;
}

function D3Graph({ narrative, selectedNodeId, onSelectNode }: D3GraphProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!svgRef.current || !containerRef.current || narrative.nodes.length === 0) return;

    const svg = d3.select(svgRef.current);
    const container = containerRef.current;
    const width = container.clientWidth;
    const height = container.clientHeight;

    // Clear previous content
    svg.selectAll('*').remove();

    // Create container group for zoom/pan
    const g = svg.append('g');

    // Setup zoom
    const zoom = d3.zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.1, 4])
      .on('zoom', (event) => {
        g.attr('transform', event.transform);
      });

    svg.call(zoom);

    // Build adjacency for layout
    const outgoing = new Map<number, number[]>();
    const incoming = new Map<number, number[]>();
    const nodeSet = new Set(narrative.nodes.map(n => n.id));

    for (const edge of narrative.edges) {
      if (!nodeSet.has(edge.from_node_id) || !nodeSet.has(edge.to_node_id)) continue;
      if (!outgoing.has(edge.from_node_id)) outgoing.set(edge.from_node_id, []);
      outgoing.get(edge.from_node_id)!.push(edge.to_node_id);
      if (!incoming.has(edge.to_node_id)) incoming.set(edge.to_node_id, []);
      incoming.get(edge.to_node_id)!.push(edge.from_node_id);
    }

    // Find roots (nodes with no incoming edges)
    const roots = narrative.nodes.filter(n => !incoming.has(n.id) || incoming.get(n.id)!.length === 0);

    // Assign depths via BFS from roots
    const depths = new Map<number, number>();
    const queue: Array<{ id: number; depth: number }> = roots.map(r => ({ id: r.id, depth: 0 }));

    while (queue.length > 0) {
      const { id, depth } = queue.shift()!;
      if (depths.has(id)) continue;
      depths.set(id, depth);

      const children = outgoing.get(id) || [];
      for (const childId of children) {
        if (!depths.has(childId)) {
          queue.push({ id: childId, depth: depth + 1 });
        }
      }
    }

    // Handle any nodes not reached (cycles or disconnected)
    for (const node of narrative.nodes) {
      if (!depths.has(node.id)) {
        depths.set(node.id, 0);
      }
    }

    // Group nodes by depth
    const byDepth = new Map<number, DecisionNode[]>();
    for (const node of narrative.nodes) {
      const d = depths.get(node.id) || 0;
      if (!byDepth.has(d)) byDepth.set(d, []);
      byDepth.get(d)!.push(node);
    }

    // Layout parameters
    const nodeWidth = 140;
    const nodeHeight = 36;
    const horizontalGap = 60;
    const verticalGap = 50;

    // Calculate positions (left to right flow)
    const layoutNodes: LayoutNode[] = [];
    const nodePositions = new Map<number, { x: number; y: number }>();
    const maxDepth = Math.max(...Array.from(depths.values()));

    for (let depth = 0; depth <= maxDepth; depth++) {
      const nodesAtDepth = byDepth.get(depth) || [];
      const totalHeight = nodesAtDepth.length * nodeHeight + (nodesAtDepth.length - 1) * verticalGap;
      const startY = -totalHeight / 2;

      nodesAtDepth.forEach((node, i) => {
        const x = depth * (nodeWidth + horizontalGap);
        const y = startY + i * (nodeHeight + verticalGap);
        layoutNodes.push({ id: node.id, node, x, y, depth });
        nodePositions.set(node.id, { x, y });
      });
    }

    // Define arrowhead marker and gold glow filter
    const defs = svg.append('defs');
    defs.append('marker')
      .attr('id', 'flow-arrowhead')
      .attr('viewBox', '0 -5 10 10')
      .attr('refX', 8)
      .attr('refY', 0)
      .attr('markerWidth', 6)
      .attr('markerHeight', 6)
      .attr('orient', 'auto')
      .append('path')
      .attr('d', 'M0,-4L8,0L0,4')
      .attr('fill', THEME.textDim);

    // Gold glow filter for selected node
    const glowFilter = defs.append('filter')
      .attr('id', 'gold-glow')
      .attr('x', '-50%')
      .attr('y', '-50%')
      .attr('width', '200%')
      .attr('height', '200%');
    glowFilter.append('feGaussianBlur')
      .attr('stdDeviation', '4')
      .attr('result', 'coloredBlur');
    glowFilter.append('feFlood')
      .attr('flood-color', '#fbbf24')
      .attr('flood-opacity', '0.8')
      .attr('result', 'glowColor');
    glowFilter.append('feComposite')
      .attr('in', 'glowColor')
      .attr('in2', 'coloredBlur')
      .attr('operator', 'in')
      .attr('result', 'softGlow');
    const glowMerge = glowFilter.append('feMerge');
    glowMerge.append('feMergeNode').attr('in', 'softGlow');
    glowMerge.append('feMergeNode').attr('in', 'softGlow');
    glowMerge.append('feMergeNode').attr('in', 'SourceGraphic');

    // Animated shimmer gradient
    const shimmerGradient = defs.append('linearGradient')
      .attr('id', 'gold-shimmer')
      .attr('x1', '0%')
      .attr('y1', '0%')
      .attr('x2', '100%')
      .attr('y2', '100%');
    shimmerGradient.append('stop')
      .attr('offset', '0%')
      .attr('stop-color', '#fbbf24')
      .attr('stop-opacity', '0.3');
    shimmerGradient.append('stop')
      .attr('offset', '50%')
      .attr('stop-color', '#fcd34d')
      .attr('stop-opacity', '0.6');
    shimmerGradient.append('stop')
      .attr('offset', '100%')
      .attr('stop-color', '#fbbf24')
      .attr('stop-opacity', '0.3');

    // Animate the shimmer
    shimmerGradient.append('animate')
      .attr('attributeName', 'x1')
      .attr('values', '-100%;100%')
      .attr('dur', '2s')
      .attr('repeatCount', 'indefinite');
    shimmerGradient.append('animate')
      .attr('attributeName', 'x2')
      .attr('values', '0%;200%')
      .attr('dur', '2s')
      .attr('repeatCount', 'indefinite');

    // Draw edges as curved paths
    const edgeGroup = g.append('g');

    for (const edge of narrative.edges) {
      const from = nodePositions.get(edge.from_node_id);
      const to = nodePositions.get(edge.to_node_id);
      if (!from || !to) continue;

      const startX = from.x + nodeWidth;
      const startY = from.y + nodeHeight / 2;
      const endX = to.x;
      const endY = to.y + nodeHeight / 2;

      // Bezier curve for smooth flow
      const midX = (startX + endX) / 2;
      const path = `M ${startX} ${startY} C ${midX} ${startY}, ${midX} ${endY}, ${endX} ${endY}`;

      edgeGroup.append('path')
        .attr('d', path)
        .attr('fill', 'none')
        .attr('stroke', THEME.textDim)
        .attr('stroke-width', 1.5)
        .attr('stroke-dasharray', EDGE_DASH_PATTERNS[edge.edge_type] || '')
        .attr('marker-end', 'url(#flow-arrowhead)');
    }

    // Draw nodes as rounded rectangles
    const nodeGroup = g.append('g');

    for (const ln of layoutNodes) {
      const typeColor = THEME[ln.node.node_type as keyof typeof THEME] || THEME.text;

      const nodeG = nodeGroup.append('g')
        .attr('transform', `translate(${ln.x}, ${ln.y})`)
        .attr('cursor', 'pointer')
        .attr('data-node-id', ln.id)
        .attr('data-type-color', typeColor)
        .on('click', (event) => {
          event.stopPropagation();
          onSelectNode(ln.id);
        });

      const isSelected = ln.id === selectedNodeId;

      // Gold shimmer overlay for selected node (behind the main rect)
      if (isSelected) {
        nodeG.append('rect')
          .attr('width', nodeWidth + 8)
          .attr('height', nodeHeight + 8)
          .attr('x', -4)
          .attr('y', -4)
          .attr('rx', 10)
          .attr('ry', 10)
          .attr('fill', 'url(#gold-shimmer)')
          .attr('filter', 'url(#gold-glow)');
      }

      // Background rect
      nodeG.append('rect')
        .attr('width', nodeWidth)
        .attr('height', nodeHeight)
        .attr('rx', 6)
        .attr('ry', 6)
        .attr('fill', THEME.bgLight)
        .attr('stroke', isSelected ? '#fbbf24' : THEME.border)
        .attr('stroke-width', isSelected ? 3 : 1);

      // Type indicator (left bar)
      nodeG.append('rect')
        .attr('width', 4)
        .attr('height', nodeHeight)
        .attr('rx', 2)
        .attr('fill', typeColor);

      // Type label
      nodeG.append('text')
        .attr('x', 10)
        .attr('y', 12)
        .attr('font-size', '9px')
        .attr('font-weight', '600')
        .attr('fill', typeColor)
        .text(TYPE_ABBREV[ln.node.node_type]);

      // Node ID
      nodeG.append('text')
        .attr('x', nodeWidth - 8)
        .attr('y', 12)
        .attr('font-size', '9px')
        .attr('fill', THEME.textMuted)
        .attr('text-anchor', 'end')
        .text(`#${ln.id}`);

      // Title (truncated)
      const title = ln.node.title.length > 18
        ? ln.node.title.slice(0, 16) + '...'
        : ln.node.title;
      nodeG.append('text')
        .attr('x', 10)
        .attr('y', 28)
        .attr('font-size', '11px')
        .attr('fill', THEME.text)
        .text(title);

      // Tooltip
      nodeG.append('title')
        .text(`${TYPE_ABBREV[ln.node.node_type]} #${ln.id}: ${ln.node.title}`);
    }

    // Fit to view, centering on selected node if present
    setTimeout(() => {
      const bounds = g.node()?.getBBox();
      if (bounds && bounds.width > 0 && bounds.height > 0) {
        let targetX = bounds.x + bounds.width / 2;
        let targetY = bounds.y + bounds.height / 2;
        let targetScale = Math.min(
          (width - 80) / bounds.width,
          (height - 80) / bounds.height,
          1.5
        );

        // If there's a selected node, zoom in on it
        if (selectedNodeId) {
          const selectedLayout = layoutNodes.find(ln => ln.id === selectedNodeId);
          if (selectedLayout) {
            targetX = selectedLayout.x + nodeWidth / 2;
            targetY = selectedLayout.y + nodeHeight / 2;
            targetScale = 1.2; // Zoom in more on selected node
          }
        }

        const translateX = width / 2 - targetX * targetScale;
        const translateY = height / 2 - targetY * targetScale;

        svg.transition().duration(400).call(
          zoom.transform,
          d3.zoomIdentity.translate(translateX, translateY).scale(targetScale)
        );
      }
    }, 100);

  }, [narrative, onSelectNode, selectedNodeId]);

  // Update selection styling and shimmer without re-rendering entire graph
  useEffect(() => {
    if (!svgRef.current) return;
    const svg = d3.select(svgRef.current);

    // Update all node groups
    svg.selectAll<SVGGElement, unknown>('g[data-node-id]').each(function() {
      const nodeG = d3.select(this);
      const nodeId = parseInt(this.getAttribute('data-node-id') || '0');
      const isSelected = nodeId === selectedNodeId;

      // Remove old shimmer
      nodeG.selectAll('.shimmer-rect').remove();

      // Add shimmer for selected node
      if (isSelected) {
        nodeG.insert('rect', ':first-child')
          .attr('class', 'shimmer-rect')
          .attr('width', 148)
          .attr('height', 44)
          .attr('x', -4)
          .attr('y', -4)
          .attr('rx', 10)
          .attr('ry', 10)
          .attr('fill', 'url(#gold-shimmer)')
          .attr('filter', 'url(#gold-glow)');
      }

      // Update stroke on main rect (the one with width 140)
      nodeG.selectAll<SVGRectElement, unknown>('rect')
        .filter(function() { return this.getAttribute('width') === '140'; })
        .attr('stroke', isSelected ? '#fbbf24' : THEME.border)
        .attr('stroke-width', isSelected ? 3 : 1);
    });
  }, [selectedNodeId]);

  return (
    <div
      ref={containerRef}
      style={{
        width: '100%',
        height: '100%',
        backgroundColor: THEME.bg,
        position: 'relative',
      }}
    >
      <svg
        ref={svgRef}
        style={{ width: '100%', height: '100%' }}
      />
      {/* Node Type Legend */}
      <div style={{
        position: 'absolute',
        bottom: '8px',
        left: '8px',
        fontSize: '10px',
        color: THEME.textMuted,
        backgroundColor: THEME.bgLight,
        padding: '6px 10px',
        borderRadius: '4px',
        display: 'flex',
        flexWrap: 'wrap',
        gap: '8px',
        maxWidth: '60%',
      }}>
        {(['goal', 'decision', 'action', 'outcome', 'observation', 'option', 'revisit'] as NodeType[]).map(type => (
          <span key={type} style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
            <span style={{
              width: '8px',
              height: '8px',
              borderRadius: '50%',
              backgroundColor: THEME[type],
            }} />
            <span>{TYPE_ABBREV[type]}</span>
          </span>
        ))}
      </div>

      {/* Edge Type Legend */}
      <div style={{
        position: 'absolute',
        bottom: '8px',
        right: '8px',
        fontSize: '10px',
        color: THEME.textMuted,
        backgroundColor: THEME.bgLight,
        padding: '6px 10px',
        borderRadius: '4px',
        display: 'flex',
        flexDirection: 'column',
        gap: '4px',
      }}>
        <span style={{ fontWeight: 600, marginBottom: '2px' }}>Edges</span>
        {([
          { type: 'leads_to', label: 'leads to', dash: '' },
          { type: 'requires', label: 'requires', dash: '5,5' },
          { type: 'chosen', label: 'chosen', dash: '2,2' },
          { type: 'rejected', label: 'rejected', dash: '8,4,2,4' },
          { type: 'blocks', label: 'blocks', dash: '10,5' },
          { type: 'enables', label: 'enables', dash: '3,3' },
        ]).map(({ type, label, dash }) => (
          <span key={type} style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
            <svg width="24" height="8">
              <line
                x1="0" y1="4" x2="24" y2="4"
                stroke={THEME.textMuted}
                strokeWidth="2"
                strokeDasharray={dash}
              />
            </svg>
            <span>{label}</span>
          </span>
        ))}
      </div>
    </div>
  );
}

// =============================================================================
// Resizable Divider Component
// =============================================================================

interface ResizerProps {
  direction: 'horizontal' | 'vertical';
  onResize: (delta: number) => void;
}

function Resizer({ direction, onResize }: ResizerProps) {
  const [isDragging, setIsDragging] = useState(false);
  const lastPosRef = useRef(0);

  const handleMouseDown = useCallback((e: ReactMouseEvent) => {
    e.preventDefault();
    setIsDragging(true);
    lastPosRef.current = direction === 'horizontal' ? e.clientX : e.clientY;

    const handleMouseMove = (moveEvent: globalThis.MouseEvent) => {
      const currentPos = direction === 'horizontal' ? moveEvent.clientX : moveEvent.clientY;
      const delta = currentPos - lastPosRef.current;
      lastPosRef.current = currentPos;
      onResize(delta);
    };

    const handleMouseUp = () => {
      setIsDragging(false);
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    document.body.style.cursor = direction === 'horizontal' ? 'col-resize' : 'row-resize';
    document.body.style.userSelect = 'none';
  }, [direction, onResize]);

  return (
    <div
      onMouseDown={handleMouseDown}
      style={{
        backgroundColor: isDragging ? THEME.decision : THEME.border,
        cursor: direction === 'horizontal' ? 'col-resize' : 'row-resize',
        flexShrink: 0,
        ...(direction === 'horizontal'
          ? { width: '4px', height: '100%' }
          : { height: '4px', width: '100%' }
        ),
        transition: isDragging ? 'none' : 'background-color 0.2s',
      }}
      onMouseEnter={(e) => {
        if (!isDragging) e.currentTarget.style.backgroundColor = THEME.decision;
      }}
      onMouseLeave={(e) => {
        if (!isDragging) e.currentTarget.style.backgroundColor = THEME.border;
      }}
    />
  );
}

// =============================================================================
// Search Results Component
// =============================================================================

interface SearchResultsProps {
  results: DecisionNode[];
  selectedNodeId: number | null;
  onSelectNode: (id: number) => void;
}

function SearchResults({ results, selectedNodeId, onSelectNode }: SearchResultsProps) {
  return (
    <div style={{ flex: 1, overflow: 'auto' }}>
      <div style={{
        padding: '8px 12px',
        borderBottom: `1px solid ${THEME.border}`,
        color: THEME.textMuted,
        fontSize: '11px',
        fontWeight: 600,
      }}>
        {results.length} RESULTS (newest first)
      </div>
      {results.map(node => {
        const typeColor = THEME[node.node_type as keyof typeof THEME] || THEME.text;
        const confidence = getConfidence(node);
        const isSelected = node.id === selectedNodeId;
        const date = new Date(node.created_at);

        return (
          <div
            key={node.id}
            onClick={() => onSelectNode(node.id)}
            style={{
              padding: '12px 16px',
              cursor: 'pointer',
              backgroundColor: isSelected ? THEME.bgSelected : 'transparent',
              borderLeft: isSelected ? `3px solid ${typeColor}` : '3px solid transparent',
              borderBottom: `1px solid ${THEME.border}`,
            }}
            onMouseEnter={(e) => {
              if (!isSelected) e.currentTarget.style.backgroundColor = THEME.bgHover;
            }}
            onMouseLeave={(e) => {
              if (!isSelected) e.currentTarget.style.backgroundColor = 'transparent';
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '4px' }}>
              <span style={{ color: typeColor, fontWeight: 600, fontSize: '11px' }}>
                {TYPE_ABBREV[node.node_type]}
              </span>
              {confidence !== null && (
                <span style={{
                  color: confidence >= 70 ? THEME.confHigh : confidence >= 40 ? THEME.confMed : THEME.confLow,
                  fontSize: '11px',
                }}>
                  {confidence}%
                </span>
              )}
              <span style={{ color: THEME.textMuted, fontSize: '11px' }}>
                #{node.id}
              </span>
              <span style={{ color: THEME.textDim, fontSize: '11px', marginLeft: 'auto' }}>
                {date.toLocaleDateString()} {date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
              </span>
            </div>
            <div style={{ color: THEME.text, fontSize: '13px' }}>
              {node.title}
            </div>
            {node.description && (
              <div style={{
                color: THEME.textMuted,
                fontSize: '12px',
                marginTop: '4px',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
              }}>
                {node.description.slice(0, 100)}{node.description.length > 100 ? '...' : ''}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

// =============================================================================
// Header Component
// =============================================================================

interface HeaderProps {
  mode: NarrativeMode;
  onModeChange: (mode: NarrativeMode) => void;
  focusedNarrative: Narrative | null;
  onBack: () => void;
  narrativeCount: number;
  nodeCount: number;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  searchResultCount: number;
  searchTypes: Set<NodeType>;
  onToggleSearchType: (type: NodeType) => void;
}

function Header({ mode, onModeChange, focusedNarrative, onBack, narrativeCount, nodeCount, searchQuery, onSearchChange, searchResultCount, searchTypes, onToggleSearchType }: HeaderProps) {
  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      gap: '16px',
      padding: '16px 20px',
      borderBottom: `1px solid ${THEME.border}`,
      backgroundColor: THEME.bgLight,
    }}>
      {/* Back button when focused */}
      {focusedNarrative && (
        <button
          onClick={onBack}
          style={{
            background: 'none',
            border: `1px solid ${THEME.border}`,
            borderRadius: '4px',
            padding: '4px 12px',
            color: THEME.text,
            cursor: 'pointer',
            fontSize: '12px',
          }}
        >
          ← Back
        </button>
      )}

      {/* Title */}
      <div style={{
        fontWeight: 600,
        color: THEME.text,
        fontSize: '14px',
      }}>
        {focusedNarrative ? focusedNarrative.name : 'deciduous'}
      </div>

      {/* Search bar and type filters */}
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: '10px' }}>
        {/* Search input */}
        <div style={{ position: 'relative' }}>
          <input
            type="text"
            placeholder="Search nodes..."
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            style={{
              width: '100%',
              padding: '20px 24px',
              paddingLeft: '56px',
              backgroundColor: THEME.bg,
              color: THEME.text,
              border: `1px solid ${THEME.border}`,
              borderRadius: '8px',
              fontSize: '24px',
              outline: 'none',
            }}
          />
          <span style={{
            position: 'absolute',
            left: '18px',
            top: '50%',
            transform: 'translateY(-50%)',
            color: THEME.textMuted,
            fontSize: '28px',
          }}>
            🔍
          </span>
          {searchQuery && (
            <span style={{
              position: 'absolute',
              right: '20px',
              top: '50%',
              transform: 'translateY(-50%)',
              color: THEME.textMuted,
              fontSize: '16px',
            }}>
              {searchResultCount} results
            </span>
          )}
        </div>

        {/* Type filter buttons */}
        <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
          {(['goal', 'decision', 'action', 'outcome', 'observation', 'option', 'revisit'] as NodeType[]).map(type => {
            const isActive = searchTypes.has(type);
            const typeColor = THEME[type];
            return (
              <button
                key={type}
                onClick={() => onToggleSearchType(type)}
                style={{
                  padding: '6px 12px',
                  backgroundColor: isActive ? typeColor : 'transparent',
                  color: isActive ? '#fff' : typeColor,
                  border: `2px solid ${typeColor}`,
                  borderRadius: '6px',
                  fontSize: '12px',
                  fontWeight: 600,
                  cursor: 'pointer',
                  transition: 'all 0.1s',
                  opacity: isActive ? 1 : 0.7,
                }}
              >
                {TYPE_ABBREV[type]}
              </button>
            );
          })}
        </div>
      </div>

      {/* Mode selector (only when not focused and not searching) */}
      {!focusedNarrative && !searchQuery && (
        <select
          value={mode}
          onChange={(e) => onModeChange(e.target.value as NarrativeMode)}
          style={{
            padding: '4px 8px',
            backgroundColor: THEME.bg,
            color: THEME.text,
            border: `1px solid ${THEME.border}`,
            borderRadius: '4px',
            fontSize: '12px',
          }}
        >
          <option value="significant">Major Narratives (10+ nodes)</option>
          <option value="goals">All Goals</option>
          <option value="branches">By Git Branch</option>
          <option value="hubs">By Key Decisions (3+ edges)</option>
        </select>
      )}

      {/* Count */}
      <div style={{
        marginLeft: 'auto',
        color: THEME.textMuted,
        fontSize: '12px',
      }}>
        {focusedNarrative
          ? `${focusedNarrative.nodeCount} nodes`
          : `${narrativeCount} narratives · ${nodeCount} nodes`
        }
      </div>
    </div>
  );
}

// =============================================================================
// Main App
// =============================================================================

export function App() {
  const [graphData, setGraphData] = useState<GraphData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // View state - default to 'significant' to show major narratives
  const [mode, setMode] = useState<NarrativeMode>('significant');
  const [focusedNarrativeId, setFocusedNarrativeId] = useState<string | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<number | null>(null);
  const [expandedNodes, setExpandedNodes] = useState<Set<number>>(new Set());
  const [searchQuery, setSearchQuery] = useState('');
  const [searchTypes, setSearchTypes] = useState<Set<NodeType>>(
    new Set(['goal', 'decision', 'action', 'outcome', 'observation', 'option', 'revisit'])
  );

  // Panel sizes in pixels - defaults for good initial layout
  const [leftPanelWidth, setLeftPanelWidth] = useState(Math.max(500, Math.floor(window.innerWidth * 0.4)));
  const [detailPanelHeight, setDetailPanelHeight] = useState(Math.max(400, Math.floor(window.innerHeight * 0.55)));

  const handleLeftPanelResize = useCallback((delta: number) => {
    setLeftPanelWidth(prev => Math.max(300, Math.min(prev + delta, window.innerWidth - 400)));
  }, []);

  const handleDetailPanelResize = useCallback((delta: number) => {
    setDetailPanelHeight(prev => Math.max(150, Math.min(prev + delta, window.innerHeight - 300)));
  }, []);

  const handleToggleSearchType = useCallback((type: NodeType) => {
    setSearchTypes(prev => {
      const next = new Set(prev);
      if (next.has(type)) {
        next.delete(type);
      } else {
        next.add(type);
      }
      return next;
    });
  }, []);

  // Load data
  useEffect(() => {
    loadGraphData()
      .then(data => {
        if (data) {
          setGraphData(data);
        } else {
          setError('No graph data found. Run `deciduous sync` to export.');
        }
      })
      .catch(e => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  // Build narratives
  const narratives = useMemo(() => {
    if (!graphData) return [];
    return buildNarratives(graphData, mode);
  }, [graphData, mode]);

  const focusedNarrative = narratives.find(n => n.id === focusedNarrativeId) || null;
  const selectedNode = graphData?.nodes.find(n => n.id === selectedNodeId) || null;

  // Build all goal-based narratives for search (regardless of current mode)
  const allNarratives = useMemo(() => {
    if (!graphData) return [];
    return buildNarratives(graphData, 'goals');
  }, [graphData]);

  // Find all narratives that contain the selected node (search across ALL narratives)
  const narrativesContainingNode = useMemo(() => {
    if (!selectedNodeId) return [];
    // First check current mode's narratives, then all narratives
    const inCurrentMode = narratives.filter(n => n.nodes.some(node => node.id === selectedNodeId));
    if (inCurrentMode.length > 0) return inCurrentMode;
    // Fall back to all narratives
    return allNarratives.filter(n => n.nodes.some(node => node.id === selectedNodeId));
  }, [selectedNodeId, narratives, allNarratives]);

  // For search results, use the first containing narrative for the graph
  // Key: this must depend on selectedNodeId to update when clicking different search results
  const activeNarrativeForGraph = useMemo(() => {
    if (focusedNarrative) return focusedNarrative;
    if (searchQuery && selectedNodeId && narrativesContainingNode.length > 0) {
      return narrativesContainingNode[0];
    }
    return null;
  }, [focusedNarrative, searchQuery, selectedNodeId, narrativesContainingNode]);

  // Search results - full text search over nodes (newest first), filtered by type
  const searchResults = useMemo(() => {
    if (!searchQuery || !graphData) return [];
    const q = searchQuery.toLowerCase();
    return graphData.nodes
      .filter(n => {
        // Filter by selected types
        if (!searchTypes.has(n.node_type)) return false;

        const title = n.title.toLowerCase();
        const desc = (n.description || '').toLowerCase();
        const meta = parseMetadata(n.metadata_json);
        const prompt = (meta?.prompt || '').toLowerCase();
        const branch = (meta?.branch || '').toLowerCase();
        return title.includes(q) || desc.includes(q) || prompt.includes(q) || branch.includes(q);
      })
      .sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime());
  }, [searchQuery, graphData, searchTypes]);

  // Expand all nodes in focused narrative by default
  useEffect(() => {
    if (focusedNarrative) {
      const allIds = new Set(focusedNarrative.nodes.map(n => n.id));
      setExpandedNodes(allIds);
    }
  }, [focusedNarrative]);

  const handleToggle = useCallback((id: number) => {
    setExpandedNodes(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const handleSelectNarrative = useCallback((id: string) => {
    setFocusedNarrativeId(id);
    setSelectedNodeId(null);
  }, []);

  const handleBack = useCallback(() => {
    setFocusedNarrativeId(null);
    setSelectedNodeId(null);
  }, []);

  if (loading) {
    return (
      <div style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        height: '100vh',
        backgroundColor: THEME.bg,
        color: THEME.text,
      }}>
        Loading...
      </div>
    );
  }

  if (error) {
    return (
      <div style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        height: '100vh',
        backgroundColor: THEME.bg,
        color: THEME.textMuted,
        flexDirection: 'column',
        gap: '8px',
      }}>
        <div style={{ color: THEME.revisit }}>Error</div>
        <div>{error}</div>
      </div>
    );
  }

  return (
    <div style={{
      display: 'flex',
      flexDirection: 'column',
      height: '100vh',
      backgroundColor: THEME.bg,
      color: THEME.text,
    }}>
      <Header
        mode={mode}
        onModeChange={setMode}
        focusedNarrative={focusedNarrative}
        onBack={handleBack}
        narrativeCount={narratives.length}
        nodeCount={graphData?.nodes.length || 0}
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        searchResultCount={searchResults.length}
        searchTypes={searchTypes}
        onToggleSearchType={handleToggleSearchType}
      />

      <div style={{
        display: 'flex',
        flex: 1,
        overflow: 'hidden',
      }}>
        {/* Left Panel: Search Results, Narrative List, or Tree */}
        <div style={{
          width: `${leftPanelWidth}px`,
          display: 'flex',
          flexDirection: 'column',
          overflow: 'auto',
          flexShrink: 0,
        }}>
          {searchQuery ? (
            // Search results
            <SearchResults
              results={searchResults}
              selectedNodeId={selectedNodeId}
              onSelectNode={setSelectedNodeId}
            />
          ) : !focusedNarrative ? (
            // Narrative list
            <div style={{ flex: 1, overflow: 'auto' }}>
              {narratives.map(narrative => (
                <NarrativeCard
                  key={narrative.id}
                  narrative={narrative}
                  onClick={() => handleSelectNarrative(narrative.id)}
                />
              ))}
            </div>
          ) : (
            // Focused narrative tree
            <div style={{ flex: 1, overflow: 'auto' }}>
              <div style={{
                display: 'grid',
                gridTemplateColumns: '24px 60px 50px 50px 1fr',
                gap: '8px',
                padding: '8px 12px',
                borderBottom: `1px solid ${THEME.border}`,
                color: THEME.textMuted,
                fontSize: '11px',
                fontWeight: 600,
                letterSpacing: '0.5px',
                fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
              }}>
                <span></span>
                <span>TYPE</span>
                <span>CONF</span>
                <span>ID</span>
                <span>TITLE</span>
              </div>
              <TreeRow
                treeNode={focusedNarrative.tree}
                isSelected={selectedNodeId === focusedNarrative.tree.node.id}
                expandedNodes={expandedNodes}
                onSelect={setSelectedNodeId}
                onToggle={handleToggle}
                edges={graphData?.edges || []}
              />
            </div>
          )}
        </div>

        {/* Horizontal Resizer */}
        <Resizer direction="horizontal" onResize={handleLeftPanelResize} />

        {/* Right Panel: Detail + Graph */}
        <div style={{
          flex: 1,
          overflow: 'hidden',
          minWidth: '300px',
          display: 'flex',
          flexDirection: 'column',
        }}>
          {/* Detail Panel (top) */}
          <div style={{
            height: activeNarrativeForGraph ? `${detailPanelHeight}px` : '100%',
            overflow: 'auto',
            flexShrink: 0,
          }}>
            <DetailPanel
              node={selectedNode}
              narrative={focusedNarrative}
              edges={graphData?.edges || []}
              nodes={graphData?.nodes || []}
              graphData={graphData}
              onSelectNode={setSelectedNodeId}
            />

            {/* Show narratives containing this node (when in search mode) */}
            {searchQuery && selectedNode && narrativesContainingNode.length > 0 && (
              <div style={{
                padding: '12px 20px',
                borderTop: `1px solid ${THEME.border}`,
              }}>
                <div style={{
                  color: THEME.textMuted,
                  fontSize: '11px',
                  fontWeight: 600,
                  letterSpacing: '0.5px',
                  marginBottom: '8px',
                }}>
                  PART OF {narrativesContainingNode.length} NARRATIVE{narrativesContainingNode.length > 1 ? 'S' : ''}
                </div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
                  {narrativesContainingNode.map(n => {
                    const typeColor = THEME[n.root.node_type as keyof typeof THEME] || THEME.text;
                    return (
                      <div
                        key={n.id}
                        onClick={() => {
                          setSearchQuery('');
                          setFocusedNarrativeId(n.id);
                        }}
                        style={{
                          padding: '8px 12px',
                          backgroundColor: THEME.bgLight,
                          borderRadius: '6px',
                          borderLeft: `3px solid ${typeColor}`,
                          cursor: 'pointer',
                        }}
                      >
                        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                          <span style={{ color: typeColor, fontSize: '10px', fontWeight: 600 }}>
                            {TYPE_ABBREV[n.root.node_type]}
                          </span>
                          <span style={{ color: THEME.text, fontSize: '13px' }}>
                            {n.name}
                          </span>
                          <span style={{ color: THEME.textMuted, fontSize: '11px', marginLeft: 'auto' }}>
                            {n.nodeCount} nodes →
                          </span>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            )}
          </div>

          {/* Vertical Resizer (between detail and graph) */}
          {activeNarrativeForGraph && (
            <Resizer direction="vertical" onResize={handleDetailPanelResize} />
          )}

          {/* D3 Graph (bottom, when narrative focused OR search result selected) */}
          {activeNarrativeForGraph && (
            <div style={{
              flex: 1,
              minHeight: '100px',
              position: 'relative',
            }}>
              <div style={{
                position: 'absolute',
                top: '8px',
                left: '8px',
                fontSize: '11px',
                color: THEME.textMuted,
                fontWeight: 600,
                letterSpacing: '0.5px',
                zIndex: 10,
              }}>
                GRAPH VIEW {searchQuery && `(${activeNarrativeForGraph.name.slice(0, 30)}${activeNarrativeForGraph.name.length > 30 ? '...' : ''})`}
              </div>
              <D3Graph
                narrative={activeNarrativeForGraph}
                selectedNodeId={selectedNodeId}
                onSelectNode={setSelectedNodeId}
              />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
