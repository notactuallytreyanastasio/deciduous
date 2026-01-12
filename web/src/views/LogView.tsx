/**
 * Log View - "Metro Map" visualization
 *
 * Clean, readable view of decision history. Major chains are metro lines.
 * Time flows left to right. Nodes are stations. Commits are milestones.
 * Focus on readability over visual novelty.
 */

import React, { useState, useMemo } from 'react';
import type { GraphData, DecisionNode, GitCommit, NodeType } from '../types/graph';
import { truncate } from '../types/graph';
import { DetailPanel } from '../components/DetailPanel';
import { getNodeColor } from '../utils/colors';

interface LogViewProps {
  graphData: GraphData;
  gitHistory?: GitCommit[];
}

interface ChainData {
  id: number;
  nodes: DecisionNode[];
  color: string;
  dominantType: NodeType;
  startTime: Date;
  endTime: Date;
}

// Build chains from connected components, filter to significant ones
function buildChains(graphData: GraphData): ChainData[] {
  const { nodes, edges } = graphData;
  if (nodes.length === 0) return [];

  // Build adjacency
  const adjacency = new Map<number, Set<number>>();
  for (const node of nodes) {
    adjacency.set(node.id, new Set());
  }
  for (const edge of edges) {
    adjacency.get(edge.from_node_id)?.add(edge.to_node_id);
    adjacency.get(edge.to_node_id)?.add(edge.from_node_id);
  }

  // Find connected components
  const visited = new Set<number>();
  const components: DecisionNode[][] = [];

  for (const node of nodes) {
    if (visited.has(node.id)) continue;

    const component: DecisionNode[] = [];
    const queue = [node.id];

    while (queue.length > 0) {
      const id = queue.shift()!;
      if (visited.has(id)) continue;
      visited.add(id);

      const n = nodes.find(x => x.id === id);
      if (n) component.push(n);

      for (const neighbor of adjacency.get(id) || []) {
        if (!visited.has(neighbor)) queue.push(neighbor);
      }
    }

    if (component.length > 0) components.push(component);
  }

  // Convert to ChainData, sort by size (largest first)
  const chains: ChainData[] = components
    .map((comp, idx) => {
      const sorted = [...comp].sort(
        (a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
      );

      // Find dominant type
      const typeCounts: Record<string, number> = {};
      comp.forEach(n => {
        typeCounts[n.node_type] = (typeCounts[n.node_type] || 0) + 1;
      });
      const dominantType = (Object.entries(typeCounts)
        .sort((a, b) => b[1] - a[1])[0]?.[0] || 'action') as NodeType;

      return {
        id: idx,
        nodes: sorted,
        color: getNodeColor(dominantType),
        dominantType,
        startTime: new Date(sorted[0].created_at),
        endTime: new Date(sorted[sorted.length - 1].created_at),
      };
    })
    .sort((a, b) => b.nodes.length - a.nodes.length);

  return chains;
}

// Group commits by date
function groupCommitsByDate(commits: GitCommit[]): Map<string, GitCommit[]> {
  const grouped = new Map<string, GitCommit[]>();
  for (const commit of commits) {
    const date = new Date(commit.date).toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
    });
    if (!grouped.has(date)) grouped.set(date, []);
    grouped.get(date)!.push(commit);
  }
  return grouped;
}

export const LogView: React.FC<LogViewProps> = ({
  graphData,
  gitHistory = [],
}) => {
  const [selectedNode, setSelectedNode] = useState<DecisionNode | null>(null);
  const [hoveredNode, setHoveredNode] = useState<number | null>(null);
  const [expandedChain, setExpandedChain] = useState<number | null>(null);

  const chains = useMemo(() => buildChains(graphData), [graphData]);
  const commitsByDate = useMemo(() => groupCommitsByDate(gitHistory), [gitHistory]);

  // Show top chains (significant ones) and aggregate the rest
  const TOP_CHAINS = 12;
  const topChains = chains.slice(0, TOP_CHAINS);
  const otherChains = chains.slice(TOP_CHAINS);
  const otherNodesCount = otherChains.reduce((sum, c) => sum + c.nodes.length, 0);

  // Get time range
  const allNodes = graphData.nodes;
  const sortedByTime = [...allNodes].sort(
    (a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
  );
  const minTime = sortedByTime.length > 0 ? new Date(sortedByTime[0].created_at) : new Date();
  const maxTime = sortedByTime.length > 0 ? new Date(sortedByTime[sortedByTime.length - 1].created_at) : new Date();

  // Format date range
  const formatDate = (d: Date) => d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });

  return (
    <div style={styles.container}>
      <div style={styles.mainContent}>
        {/* Header */}
        <div style={styles.header}>
          <div style={styles.headerTop}>
            <h2 style={styles.title}>Decision Log</h2>
            <span style={styles.dateRange}>
              {formatDate(minTime)} — {formatDate(maxTime)}
            </span>
          </div>
          <div style={styles.stats}>
            <span style={styles.stat}>{chains.length} chains</span>
            <span style={styles.statDivider}>·</span>
            <span style={styles.stat}>{graphData.nodes.length} decisions</span>
            <span style={styles.statDivider}>·</span>
            <span style={styles.stat}>{gitHistory.length} commits</span>
          </div>
          <div style={styles.legend}>
            {(['goal', 'decision', 'action', 'outcome', 'observation'] as NodeType[]).map(type => (
              <span key={type} style={styles.legendItem}>
                <span style={{ ...styles.legendDot, background: getNodeColor(type) }} />
                {type}
              </span>
            ))}
          </div>
        </div>

        {/* Chain list */}
        <div style={styles.chainList}>
          {topChains.map((chain, idx) => {
            const isExpanded = expandedChain === chain.id;
            const displayNodes = isExpanded ? chain.nodes : chain.nodes.slice(0, 8);
            const hasMore = chain.nodes.length > 8;

            return (
              <div key={chain.id} style={styles.chainRow}>
                {/* Chain header */}
                <div
                  style={{
                    ...styles.chainHeader,
                    borderLeftColor: chain.color,
                  }}
                  onClick={() => setExpandedChain(isExpanded ? null : chain.id)}
                >
                  <div style={styles.chainInfo}>
                    <span style={styles.chainNumber}>#{idx + 1}</span>
                    <span style={{ ...styles.chainType, color: chain.color }}>
                      {chain.dominantType}
                    </span>
                    <span style={styles.chainCount}>
                      {chain.nodes.length} node{chain.nodes.length !== 1 ? 's' : ''}
                    </span>
                    <span style={styles.chainTime}>
                      {formatDate(chain.startTime)}
                      {chain.startTime.toDateString() !== chain.endTime.toDateString() &&
                        ` → ${formatDate(chain.endTime)}`}
                    </span>
                  </div>
                  <span style={styles.expandIcon}>{isExpanded ? '▼' : '▶'}</span>
                </div>

                {/* Chain nodes - horizontal timeline */}
                <div style={styles.chainTimeline}>
                  <div
                    style={{
                      ...styles.timelineLine,
                      background: `linear-gradient(90deg, ${chain.color}44, ${chain.color}88, ${chain.color}44)`,
                    }}
                  />

                  <div style={styles.nodeRow}>
                    {displayNodes.map((node) => {
                      const isHovered = hoveredNode === node.id;
                      const isSelected = selectedNode?.id === node.id;

                      return (
                        <div
                          key={node.id}
                          style={{
                            ...styles.nodeCard,
                            borderColor: isSelected ? getNodeColor(node.node_type) : '#d0d7de',
                            backgroundColor: isHovered ? '#f6f8fa' : '#ffffff',
                            transform: isHovered ? 'translateY(-2px)' : 'none',
                            boxShadow: isHovered ? '0 4px 8px rgba(0,0,0,0.1)' : '0 1px 2px rgba(0,0,0,0.04)',
                          }}
                          onMouseEnter={() => setHoveredNode(node.id)}
                          onMouseLeave={() => setHoveredNode(null)}
                          onClick={(e) => {
                            e.stopPropagation();
                            setSelectedNode(node);
                          }}
                        >
                          <div style={styles.nodeHeader}>
                            <span
                              style={{
                                ...styles.nodeTypeBadge,
                                backgroundColor: getNodeColor(node.node_type) + '33',
                                color: getNodeColor(node.node_type),
                              }}
                            >
                              {node.node_type.slice(0, 3)}
                            </span>
                            <span style={styles.nodeId}>#{node.id}</span>
                          </div>
                          <div style={styles.nodeTitle}>
                            {truncate(node.title, 40)}
                          </div>
                          <div style={styles.nodeTime}>
                            {new Date(node.created_at).toLocaleTimeString('en-US', {
                              hour: 'numeric',
                              minute: '2-digit',
                            })}
                          </div>
                        </div>
                      );
                    })}

                    {/* Show more indicator */}
                    {hasMore && !isExpanded && (
                      <div
                        style={styles.moreIndicator}
                        onClick={(e) => {
                          e.stopPropagation();
                          setExpandedChain(chain.id);
                        }}
                      >
                        +{chain.nodes.length - 8} more
                      </div>
                    )}
                  </div>
                </div>
              </div>
            );
          })}

          {/* Other chains summary */}
          {otherChains.length > 0 && (
            <div style={styles.otherChains}>
              <span style={styles.otherLabel}>
                + {otherChains.length} smaller chains ({otherNodesCount} nodes)
              </span>
            </div>
          )}
        </div>

        {/* Commit timeline */}
        {gitHistory.length > 0 && (
          <div style={styles.commitSection}>
            <h3 style={styles.sectionTitle}>Recent Commits</h3>
            <div style={styles.commitList}>
              {Array.from(commitsByDate.entries()).slice(0, 5).map(([date, commits]) => (
                <div key={date} style={styles.commitGroup}>
                  <div style={styles.commitDate}>{date}</div>
                  <div style={styles.commitItems}>
                    {commits.slice(0, 3).map(commit => (
                      <div key={commit.hash} style={styles.commitItem}>
                        <span style={styles.commitHash}>{commit.hash.slice(0, 7)}</span>
                        <span style={styles.commitMsg}>{truncate(commit.message, 50)}</span>
                      </div>
                    ))}
                    {commits.length > 3 && (
                      <span style={styles.commitMore}>+{commits.length - 3} more</span>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Detail panel */}
      <div style={styles.detailPanel}>
        <DetailPanel
          node={selectedNode}
          graphData={graphData}
          onSelectNode={(id) => {
            const node = graphData.nodes.find(n => n.id === id);
            if (node) setSelectedNode(node);
          }}
          onClose={() => setSelectedNode(null)}
          gitHistory={gitHistory}
        />
      </div>
    </div>
  );
};

// =============================================================================
// Styles
// =============================================================================

const styles: Record<string, React.CSSProperties> = {
  container: {
    height: '100%',
    display: 'flex',
    backgroundColor: '#ffffff',
    color: '#24292f',
  },
  mainContent: {
    flex: 1,
    display: 'flex',
    flexDirection: 'column',
    overflow: 'hidden',
  },
  header: {
    padding: '24px 32px 20px',
    borderBottom: '1px solid #d0d7de',
    backgroundColor: '#f6f8fa',
  },
  headerTop: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'baseline',
    marginBottom: '8px',
  },
  title: {
    margin: 0,
    fontSize: '24px',
    fontWeight: 600,
    color: '#24292f',
  },
  dateRange: {
    fontSize: '14px',
    color: '#57606a',
  },
  stats: {
    display: 'flex',
    gap: '8px',
    marginBottom: '16px',
    fontSize: '14px',
    color: '#57606a',
  },
  stat: {},
  statDivider: {
    color: '#d0d7de',
  },
  legend: {
    display: 'flex',
    gap: '20px',
    flexWrap: 'wrap',
  },
  legendItem: {
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
    fontSize: '12px',
    color: '#57606a',
    textTransform: 'capitalize',
  },
  legendDot: {
    width: '10px',
    height: '10px',
    borderRadius: '50%',
  },

  // Chain list
  chainList: {
    flex: 1,
    overflow: 'auto',
    padding: '16px 32px',
  },
  chainRow: {
    marginBottom: '24px',
  },
  chainHeader: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: '12px 16px',
    backgroundColor: '#f6f8fa',
    borderRadius: '8px',
    borderLeft: '4px solid',
    cursor: 'pointer',
    transition: 'background-color 0.15s',
  },
  chainInfo: {
    display: 'flex',
    alignItems: 'center',
    gap: '16px',
  },
  chainNumber: {
    fontSize: '14px',
    fontWeight: 600,
    color: '#57606a',
    minWidth: '32px',
  },
  chainType: {
    fontSize: '13px',
    fontWeight: 600,
    textTransform: 'uppercase',
    letterSpacing: '0.5px',
  },
  chainCount: {
    fontSize: '13px',
    color: '#57606a',
  },
  chainTime: {
    fontSize: '12px',
    color: '#8c959f',
  },
  expandIcon: {
    fontSize: '10px',
    color: '#57606a',
  },

  // Timeline
  chainTimeline: {
    position: 'relative',
    marginTop: '12px',
    marginLeft: '20px',
    paddingLeft: '20px',
  },
  timelineLine: {
    position: 'absolute',
    left: 0,
    top: '50%',
    transform: 'translateY(-50%)',
    width: '4px',
    height: 'calc(100% + 20px)',
    borderRadius: '2px',
  },
  nodeRow: {
    display: 'flex',
    gap: '12px',
    flexWrap: 'wrap',
    padding: '8px 0',
  },
  nodeCard: {
    width: '180px',
    padding: '12px',
    backgroundColor: '#ffffff',
    borderRadius: '8px',
    border: '1px solid #d0d7de',
    cursor: 'pointer',
    transition: 'all 0.15s ease',
    boxShadow: '0 1px 2px rgba(0,0,0,0.04)',
  },
  nodeHeader: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: '8px',
  },
  nodeTypeBadge: {
    fontSize: '10px',
    fontWeight: 600,
    textTransform: 'uppercase',
    padding: '2px 6px',
    borderRadius: '4px',
  },
  nodeId: {
    fontSize: '11px',
    color: '#8c959f',
    fontFamily: 'monospace',
  },
  nodeTitle: {
    fontSize: '13px',
    lineHeight: '1.4',
    color: '#24292f',
    marginBottom: '8px',
    minHeight: '36px',
  },
  nodeTime: {
    fontSize: '11px',
    color: '#8c959f',
  },
  moreIndicator: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    width: '100px',
    padding: '12px',
    backgroundColor: '#ffffff',
    borderRadius: '8px',
    border: '2px dashed #d0d7de',
    color: '#57606a',
    fontSize: '13px',
    cursor: 'pointer',
    transition: 'all 0.15s',
  },

  // Other chains
  otherChains: {
    padding: '16px',
    backgroundColor: '#f6f8fa',
    borderRadius: '8px',
    textAlign: 'center',
  },
  otherLabel: {
    fontSize: '14px',
    color: '#57606a',
  },

  // Commits
  commitSection: {
    padding: '20px 32px',
    borderTop: '1px solid #d0d7de',
    backgroundColor: '#f6f8fa',
  },
  sectionTitle: {
    margin: '0 0 16px 0',
    fontSize: '16px',
    fontWeight: 600,
    color: '#24292f',
  },
  commitList: {
    display: 'flex',
    gap: '24px',
    overflowX: 'auto',
    paddingBottom: '8px',
  },
  commitGroup: {
    minWidth: '200px',
  },
  commitDate: {
    fontSize: '12px',
    fontWeight: 600,
    color: '#57606a',
    marginBottom: '8px',
    textTransform: 'uppercase',
    letterSpacing: '0.5px',
  },
  commitItems: {
    display: 'flex',
    flexDirection: 'column',
    gap: '6px',
  },
  commitItem: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    padding: '8px 12px',
    backgroundColor: '#ffffff',
    borderRadius: '6px',
    border: '1px solid #d0d7de',
  },
  commitHash: {
    fontSize: '12px',
    fontFamily: 'monospace',
    color: '#0969da',
    backgroundColor: '#ddf4ff',
    padding: '2px 6px',
    borderRadius: '4px',
  },
  commitMsg: {
    fontSize: '13px',
    color: '#57606a',
  },
  commitMore: {
    fontSize: '12px',
    color: '#8c959f',
    paddingLeft: '8px',
  },

  // Detail panel
  detailPanel: {
    width: '380px',
    flexShrink: 0,
    borderLeft: '1px solid #d0d7de',
    backgroundColor: '#f6f8fa',
    overflow: 'auto',
  },
};
