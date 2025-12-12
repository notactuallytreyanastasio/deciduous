/**
 * Chains View
 *
 * Port of docs/demo/index.html - shows chains and sessions with flow view.
 * Preserves the exact logic from the vanilla JS implementation.
 */

import React, { useState, useMemo } from 'react';
import type { DecisionNode, GraphData, Chain, Session, GitCommit } from '../types/graph';
import { getConfidence, getCommit, truncate, getDuration } from '../types/graph';
import { TypeBadge, ConfidenceBadge, CommitBadge } from '../components/NodeBadge';
import { DetailPanel } from '../components/DetailPanel';
import { getNodeColor } from '../utils/colors';

interface ChainsViewProps {
  graphData: GraphData;
  chains: Chain[];
  sessions: Session[];
  gitHistory?: GitCommit[];
}

// Look up commit message from gitHistory by hash
function getCommitMessage(hash: string | null, gitHistory: GitCommit[]): string | null {
  if (!hash || gitHistory.length === 0) return null;
  const commit = gitHistory.find(c => c.hash === hash || c.short_hash === hash);
  return commit?.message ?? null;
}

type SidebarView = 'chains' | 'sessions' | 'all';

export const ChainsView: React.FC<ChainsViewProps> = ({
  graphData,
  chains,
  sessions,
  gitHistory = [],
}) => {
  const [sidebarView, setSidebarView] = useState<SidebarView>('chains');
  const [selectedChainIndex, setSelectedChainIndex] = useState<number | null>(null);
  const [selectedNode, setSelectedNode] = useState<DecisionNode | null>(null);

  // Get the selected chain
  const selectedChain = selectedChainIndex !== null ? chains[selectedChainIndex] : null;

  // Build edge map for the selected chain (for rationale display)
  const edgeMap = useMemo(() => {
    if (!selectedChain) return new Map<number, typeof graphData.edges[0]>();
    const map = new Map<number, typeof graphData.edges[0]>();
    selectedChain.edges.forEach(e => map.set(e.to_node_id, e));
    return map;
  }, [selectedChain]);

  const handleSelectChain = (index: number) => {
    setSelectedChainIndex(index);
    setSelectedNode(null);
  };

  const handleSelectNode = (id: number) => {
    const node = graphData.nodes.find(n => n.id === id);
    if (node) {
      setSelectedNode(node);
      setSelectedChainIndex(null);
    }
  };

  const handleSelectNodeInChain = (id: number) => {
    const node = graphData.nodes.find(n => n.id === id);
    if (node) {
      setSelectedNode(node);
      // Keep the chain selected to show flow view
    }
  };

  return (
    <div style={styles.container}>
      {/* Sidebar */}
      <div style={styles.sidebar}>
        {/* View Toggle */}
        <div style={styles.viewToggle}>
          {(['chains', 'sessions', 'all'] as SidebarView[]).map(view => (
            <button
              key={view}
              onClick={() => setSidebarView(view)}
              style={{
                ...styles.viewBtn,
                ...(sidebarView === view ? styles.viewBtnActive : {}),
              }}
            >
              {view === 'all' ? 'All Nodes' : view.charAt(0).toUpperCase() + view.slice(1)}
            </button>
          ))}
        </div>

        {/* Sidebar Content */}
        <div style={styles.sidebarContent}>
          {sidebarView === 'chains' && (
            <ChainList
              chains={chains}
              selectedIndex={selectedChainIndex}
              onSelect={handleSelectChain}
            />
          )}
          {sidebarView === 'sessions' && (
            <SessionList
              sessions={sessions}
              chains={chains}
              selectedChainIndex={selectedChainIndex}
              onSelectChain={handleSelectChain}
            />
          )}
          {sidebarView === 'all' && (
            <NodeList
              nodes={graphData.nodes}
              selectedNode={selectedNode}
              onSelect={handleSelectNode}
            />
          )}
        </div>
      </div>

      {/* Detail Panel */}
      <div style={styles.detailPanel}>
        {selectedChain && !selectedNode ? (
          <ChainFlowView
            chain={selectedChain}
            edgeMap={edgeMap}
            selectedNode={selectedNode}
            onSelectNode={handleSelectNodeInChain}
            gitHistory={gitHistory}
          />
        ) : (
          <DetailPanel
            node={selectedNode}
            graphData={graphData}
            onSelectNode={handleSelectNode}
            gitHistory={gitHistory}
          />
        )}
      </div>
    </div>
  );
};

// =============================================================================
// Chain List
// =============================================================================

interface ChainListProps {
  chains: Chain[];
  selectedIndex: number | null;
  onSelect: (index: number) => void;
}

const ChainList: React.FC<ChainListProps> = ({ chains, selectedIndex, onSelect }) => {
  return (
    <div style={styles.nodeList}>
      {chains.map((chain, i) => {
        const types = [...new Set(chain.nodes.map(n => n.node_type))];
        const isSelected = selectedIndex === i;

        return (
          <div
            key={i}
            onClick={() => onSelect(i)}
            style={{
              ...styles.chainItem,
              ...(isSelected ? styles.chainItemSelected : {}),
            }}
          >
            <div style={styles.chainSummary}>
              <TypeBadge type={chain.root.node_type} size="sm" />
              <span style={styles.chainTitle}>{truncate(chain.root.title, 40)}</span>
            </div>
            <div style={styles.chainStats}>
              {chain.nodes.length} nodes · {chain.edges.length} edges
            </div>
            <div style={styles.chainTypes}>
              {types.map(t => (
                <span
                  key={t}
                  style={{ ...styles.chainTypeDot, backgroundColor: getNodeColor(t) }}
                  title={t}
                />
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
};

// =============================================================================
// Session List
// =============================================================================

interface SessionListProps {
  sessions: Session[];
  chains: Chain[];
  selectedChainIndex: number | null;
  onSelectChain: (index: number) => void;
}

const SessionList: React.FC<SessionListProps> = ({
  sessions,
  chains,
  selectedChainIndex,
  onSelectChain,
}) => {
  const [expandedSessions, setExpandedSessions] = useState<Set<number>>(new Set());

  const toggleSession = (index: number) => {
    setExpandedSessions(prev => {
      const next = new Set(prev);
      if (next.has(index)) {
        next.delete(index);
      } else {
        next.add(index);
      }
      return next;
    });
  };

  return (
    <div>
      {sessions.map((session, i) => {
        const date = new Date(session.startTime);
        const dateStr = date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
        const timeStr = date.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
        const isExpanded = expandedSessions.has(i);

        return (
          <div key={i} style={styles.sessionGroup}>
            <div
              onClick={() => toggleSession(i)}
              style={{
                ...styles.sessionHeader,
                ...(isExpanded ? styles.sessionHeaderExpanded : {}),
              }}
            >
              <span style={{
                ...styles.sessionToggle,
                transform: isExpanded ? 'rotate(90deg)' : 'none',
              }}>
                ▶
              </span>
              <span style={styles.sessionTitle}>{dateStr} @ {timeStr}</span>
              <span style={styles.sessionCount}>{session.nodes.length} nodes</span>
            </div>
            {isExpanded && (
              <div style={styles.chainList}>
                {session.chains.length > 0 ? (
                  session.chains.map((chain) => {
                    const globalIndex = chains.indexOf(chain);
                    const types = [...new Set(chain.nodes.map(n => n.node_type))];

                    return (
                      <div
                        key={globalIndex}
                        onClick={() => onSelectChain(globalIndex)}
                        style={{
                          ...styles.chainItem,
                          ...(selectedChainIndex === globalIndex ? styles.chainItemSelected : {}),
                        }}
                      >
                        <div style={styles.chainSummary}>
                          <TypeBadge type={chain.root.node_type} size="sm" />
                          <span style={styles.chainTitle}>{truncate(chain.root.title, 35)}</span>
                        </div>
                        <div style={styles.chainStats}>
                          {chain.nodes.length} nodes · {chain.edges.length} edges
                        </div>
                        <div style={styles.chainTypes}>
                          {types.map(t => (
                            <span
                              key={t}
                              style={{ ...styles.chainTypeDot, backgroundColor: getNodeColor(t) }}
                              title={t}
                            />
                          ))}
                        </div>
                      </div>
                    );
                  })
                ) : (
                  <div style={{ color: '#666', fontSize: '12px', padding: '10px' }}>
                    No complete chains in this session
                  </div>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
};

// =============================================================================
// Node List
// =============================================================================

interface NodeListProps {
  nodes: DecisionNode[];
  selectedNode: DecisionNode | null;
  onSelect: (id: number) => void;
}

const NodeList: React.FC<NodeListProps> = ({ nodes, selectedNode, onSelect }) => {
  return (
    <div style={styles.nodeList}>
      {nodes.map(node => (
        <div
          key={node.id}
          onClick={() => onSelect(node.id)}
          style={{
            ...styles.nodeItem,
            ...(selectedNode?.id === node.id ? styles.nodeItemSelected : {}),
          }}
        >
          <TypeBadge type={node.node_type} />
          <div style={styles.nodeTitle}>{node.title}</div>
        </div>
      ))}
    </div>
  );
};

// =============================================================================
// Chain Flow View
// =============================================================================

interface ChainFlowViewProps {
  chain: Chain;
  edgeMap: Map<number, Chain['edges'][0]>;
  selectedNode: DecisionNode | null;
  onSelectNode: (id: number) => void;
  gitHistory: GitCommit[];
}

const ChainFlowView: React.FC<ChainFlowViewProps> = ({
  chain,
  edgeMap,
  selectedNode,
  onSelectNode,
  gitHistory,
}) => {
  const duration = getDuration(
    chain.nodes[0].created_at,
    chain.nodes[chain.nodes.length - 1].created_at
  );

  return (
    <div style={styles.chainFlow}>
      <div style={styles.chainFlowHeader}>
        <TypeBadge type={chain.root.node_type} />
        <h2 style={styles.chainFlowTitle}>{chain.root.title}</h2>
        <div style={styles.chainFlowMeta}>
          {chain.nodes.length} nodes · {chain.edges.length} connections · {duration}
        </div>
      </div>

      <div style={styles.flowTimeline}>
        {chain.nodes.map((node) => {
          const edge = edgeMap.get(node.id);
          const isSelected = selectedNode?.id === node.id;
          const time = new Date(node.created_at).toLocaleTimeString('en-US', {
            hour: 'numeric',
            minute: '2-digit',
          });
          const conf = getConfidence(node);
          const commit = getCommit(node);
          const commitMsg = getCommitMessage(commit, gitHistory);

          return (
            <React.Fragment key={node.id}>
              {edge?.rationale && (
                <div style={styles.flowEdgeLabel}>↳ {edge.rationale}</div>
              )}
              <div
                onClick={() => onSelectNode(node.id)}
                style={{
                  ...styles.flowNode,
                  borderColor: isSelected ? '#0969da' : '#d0d7de',
                  backgroundColor: isSelected ? '#ddf4ff' : '#ffffff',
                }}
              >
                <div style={{
                  ...styles.flowNodeMarker,
                  backgroundColor: getNodeColor(node.node_type),
                  borderColor: getNodeColor(node.node_type),
                }} />
                <div style={styles.flowNodeHeader}>
                  <TypeBadge type={node.node_type} size="sm" />
                  <ConfidenceBadge confidence={conf} />
                  <CommitBadge commit={commit} />
                  <span style={styles.flowNodeTitle}>{node.title}</span>
                  <span style={styles.flowNodeTime}>{time}</span>
                </div>
                {commitMsg && (
                  <div style={styles.commitMessage}>
                    <span style={styles.commitIcon}>📝</span> {commitMsg}
                  </div>
                )}
                {node.description && (
                  <div style={styles.flowNodeDesc}>{node.description}</div>
                )}
              </div>
            </React.Fragment>
          );
        })}
      </div>

      <div style={styles.navLinks}>
        <a href="../decision-graph" style={styles.link}>Learn about the graph →</a>
        <a href="../claude-tooling" style={styles.link}>See the tooling →</a>
      </div>
    </div>
  );
};

// =============================================================================
// Styles
// =============================================================================

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: 'flex',
    height: '100%',
    gap: '0',
  },
  sidebar: {
    width: '380px',
    backgroundColor: '#f6f8fa',
    borderRight: '1px solid #d0d7de',
    display: 'flex',
    flexDirection: 'column',
    flexShrink: 0,
  },
  viewToggle: {
    display: 'flex',
    padding: '10px',
    gap: '5px',
    borderBottom: '1px solid #d0d7de',
  },
  viewBtn: {
    flex: 1,
    padding: '8px',
    border: '1px solid #d0d7de',
    backgroundColor: '#ffffff',
    color: '#57606a',
    borderRadius: '4px',
    cursor: 'pointer',
    fontSize: '12px',
  },
  viewBtnActive: {
    backgroundColor: '#0969da',
    color: '#ffffff',
    borderColor: '#0969da',
  },
  sidebarContent: {
    flex: 1,
    overflowY: 'auto',
  },
  nodeList: {
    padding: '10px',
  },
  chainItem: {
    padding: '10px 12px',
    margin: '4px 0',
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    cursor: 'pointer',
    borderLeft: '3px solid transparent',
  },
  chainItemSelected: {
    borderLeftColor: '#0969da',
    backgroundColor: '#ddf4ff',
  },
  chainSummary: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    marginBottom: '4px',
  },
  chainTitle: {
    fontSize: '13px',
    flex: 1,
    color: '#24292f',
  },
  chainStats: {
    fontSize: '10px',
    color: '#6e7781',
  },
  chainTypes: {
    display: 'flex',
    gap: '3px',
    marginTop: '6px',
  },
  chainTypeDot: {
    width: '8px',
    height: '8px',
    borderRadius: '50%',
  },
  sessionGroup: {
    borderBottom: '1px solid #d0d7de',
  },
  sessionHeader: {
    padding: '12px 15px',
    backgroundColor: '#ffffff',
    cursor: 'pointer',
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
  },
  sessionHeaderExpanded: {},
  sessionToggle: {
    width: '16px',
    height: '16px',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: '#6e7781',
    transition: 'transform 0.2s',
  },
  sessionTitle: {
    flex: 1,
    fontSize: '13px',
    fontWeight: 600,
    color: '#24292f',
  },
  sessionCount: {
    fontSize: '11px',
    color: '#57606a',
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    padding: '2px 8px',
    borderRadius: '10px',
  },
  chainList: {
    padding: '5px 10px 10px',
  },
  nodeItem: {
    padding: '12px',
    marginBottom: '8px',
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    cursor: 'pointer',
    borderLeft: '4px solid transparent',
  },
  nodeItemSelected: {
    borderLeftColor: '#0969da',
    backgroundColor: '#ddf4ff',
  },
  nodeTitle: {
    fontSize: '14px',
    lineHeight: 1.4,
    marginTop: '6px',
    color: '#24292f',
  },
  detailPanel: {
    flex: 1,
    overflowY: 'auto',
    backgroundColor: '#ffffff',
  },
  chainFlow: {
    maxWidth: '700px',
    padding: '25px',
  },
  chainFlowHeader: {
    marginBottom: '25px',
  },
  chainFlowTitle: {
    fontSize: '20px',
    marginTop: '8px',
    marginBottom: '8px',
    color: '#24292f',
  },
  chainFlowMeta: {
    fontSize: '12px',
    color: '#6e7781',
  },
  flowTimeline: {
    position: 'relative',
    paddingLeft: '30px',
  },
  flowEdgeLabel: {
    fontSize: '11px',
    color: '#1a7f37',
    margin: '-10px 0 10px 0',
    paddingLeft: '5px',
    fontWeight: 500,
  },
  flowNode: {
    position: 'relative',
    marginBottom: '20px',
    padding: '15px',
    backgroundColor: '#ffffff',
    borderRadius: '8px',
    border: '1px solid #d0d7de',
    cursor: 'pointer',
    transition: 'all 0.2s',
  },
  flowNodeMarker: {
    position: 'absolute',
    left: '-26px',
    top: '20px',
    width: '12px',
    height: '12px',
    borderRadius: '50%',
    border: '2px solid',
  },
  flowNodeHeader: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    marginBottom: '8px',
    flexWrap: 'wrap',
  },
  flowNodeTitle: {
    fontSize: '14px',
    fontWeight: 500,
    flex: 1,
    color: '#24292f',
  },
  flowNodeTime: {
    fontSize: '10px',
    color: '#6e7781',
  },
  flowNodeDesc: {
    fontSize: '12px',
    color: '#57606a',
    lineHeight: 1.5,
  },
  commitMessage: {
    fontSize: '12px',
    color: '#0969da',
    backgroundColor: '#ddf4ff',
    padding: '6px 10px',
    borderRadius: '4px',
    marginBottom: '6px',
    marginTop: '4px',
    display: 'flex',
    alignItems: 'flex-start',
    gap: '6px',
    whiteSpace: 'pre-wrap',
  },
  commitIcon: {
    fontSize: '11px',
    flexShrink: 0,
  },
  navLinks: {
    marginTop: '20px',
    paddingTop: '20px',
    borderTop: '1px solid #d0d7de',
  },
  link: {
    color: '#0969da',
    textDecoration: 'none',
    marginRight: '20px',
    fontSize: '13px',
  },
};
