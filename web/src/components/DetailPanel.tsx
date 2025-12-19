/**
 * Detail Panel Component
 *
 * Shared detail panel for displaying node information.
 * Used by all views.
 */

import React, { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import type { DecisionNode, GraphData, GitCommit } from '../types/graph';
import { getPrompt, getFiles, getBranch, getCommit, shortCommit, githubCommitUrl, getCommitRepo } from '../types/graph';
import { NodeBadges, EdgeBadge, StatusBadge } from './NodeBadge';
import { formatDuration, getModelShortName } from '../types/trace';

// Trace info for a node - includes span content for context
interface SpanWithSession {
  span_id: number;
  sequence_num: number;
  session_id: string;
  model: string | null;
  duration_ms: number | null;
  started_at: string;
  // Content previews
  thinking_preview: string | null;
  response_preview: string | null;
  tool_names: string | null;
  user_preview: string | null;
}

interface NodeTraceInfo {
  spans: SpanWithSession[];
}

interface DetailPanelProps {
  node: DecisionNode | null;
  graphData: GraphData;
  onSelectNode: (id: number) => void;
  onClose?: () => void;
  repo?: string;
  gitHistory?: GitCommit[];
}

// Look up commit info from gitHistory by hash
function getCommitInfo(hash: string | null, gitHistory: GitCommit[]): GitCommit | null {
  if (!hash || gitHistory.length === 0) return null;
  return gitHistory.find(c => c.hash === hash || c.short_hash === hash) ?? null;
}

export const DetailPanel: React.FC<DetailPanelProps> = ({
  node,
  graphData,
  onSelectNode,
  onClose,
  repo,
  gitHistory = [],
}) => {
  const navigate = useNavigate();
  const [traceInfo, setTraceInfo] = useState<NodeTraceInfo | null>(null);
  const [expandedSpan, setExpandedSpan] = useState<number | null>(null);

  // Navigate to trace view with specific session/span
  const navigateToTrace = (sessionId: string, spanId: number) => {
    navigate(`/traces?session=${sessionId.slice(0, 8)}&span=${spanId}`);
  };

  // Fetch trace info when node changes
  useEffect(() => {
    if (!node) {
      setTraceInfo(null);
      return;
    }

    const fetchTraceInfo = async () => {
      try {
        const res = await fetch(`/api/nodes/${node.id}/traces`);
        const data = await res.json();
        if (data.ok && data.data) {
          setTraceInfo(data.data);
        }
      } catch (e) {
        console.error('Failed to fetch trace info:', e);
        setTraceInfo(null);
      }
    };

    fetchTraceInfo();
  }, [node?.id]);

  // Use repo from config if not explicitly passed
  const effectiveRepo = repo ?? getCommitRepo(graphData);
  if (!node) {
    return (
      <div style={styles.panel}>
        <div style={styles.empty}>
          <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" style={{ opacity: 0.3 }}>
            <circle cx="12" cy="12" r="10" />
            <path d="M12 6v6l4 2" />
          </svg>
          <p style={{ marginTop: '20px' }}>Select a node to see details</p>
        </div>
      </div>
    );
  }

  const incoming = graphData.edges.filter(e => e.to_node_id === node.id);
  const outgoing = graphData.edges.filter(e => e.from_node_id === node.id);
  const prompt = getPrompt(node);
  const files = getFiles(node);
  const branch = getBranch(node);
  const commitHash = getCommit(node);
  const commitInfo = getCommitInfo(commitHash, gitHistory);

  const getNodeTitle = (id: number): string => {
    const n = graphData.nodes.find(n => n.id === id);
    return n?.title || 'Unknown';
  };

  return (
    <div style={styles.panel}>
      {onClose && (
        <button onClick={onClose} style={styles.closeButton}>
          ×
        </button>
      )}

      <div style={styles.header}>
        <NodeBadges node={node} repo={repo} />
        <h2 style={styles.title}>{node.title}</h2>
        <div style={styles.meta}>
          <span>ID: {node.id}</span>
          <span><StatusBadge status={node.status} /></span>
          <span>Created: {new Date(node.created_at).toLocaleDateString()}</span>
        </div>
      </div>

      {node.description && (
        <div style={styles.description}>
          {node.description}
        </div>
      )}

      {prompt && (
        <div style={styles.section}>
          <h3 style={styles.sectionTitle}>Prompt</h3>
          <div style={styles.prompt}>
            {prompt}
          </div>
        </div>
      )}

      {files && files.length > 0 && (
        <div style={styles.section}>
          <h3 style={styles.sectionTitle}>Associated Files</h3>
          <div style={styles.fileList}>
            {files.map((file, i) => (
              <span key={i} style={styles.fileTag}>{file}</span>
            ))}
          </div>
        </div>
      )}

      {branch && (
        <div style={styles.section}>
          <h3 style={styles.sectionTitle}>Branch</h3>
          <span style={styles.branchTag}>{branch}</span>
        </div>
      )}

      {commitHash && (
        <div style={styles.section}>
          <h3 style={styles.sectionTitle}>Linked Commit</h3>
          <div style={styles.commitSection}>
            <a
              href={githubCommitUrl(commitHash, effectiveRepo)}
              target="_blank"
              rel="noopener noreferrer"
              style={styles.commitHash}
            >
              {shortCommit(commitHash)}
            </a>
            {commitInfo && (
              <>
                <div style={styles.commitMessage}>{commitInfo.message}</div>
                <div style={styles.commitMeta}>
                  by {commitInfo.author} · {new Date(commitInfo.date).toLocaleDateString()}
                  {commitInfo.files_changed && ` · ${commitInfo.files_changed} files`}
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {traceInfo && traceInfo.spans.length > 0 && (
        <div style={styles.section}>
          <h3 style={styles.sectionTitle}>Created During Trace</h3>
          {traceInfo.spans.map((span) => {
            const isExpanded = expandedSpan === span.span_id;
            const hasContent = span.thinking_preview || span.response_preview || span.tool_names;
            return (
              <div key={span.span_id} style={styles.traceInfo}>
                <div
                  style={{...styles.traceHeader, cursor: 'pointer'}}
                  onClick={() => hasContent && setExpandedSpan(isExpanded ? null : span.span_id)}
                >
                  <span style={styles.traceSpan}>
                    {hasContent && <span style={{marginRight: '4px'}}>{isExpanded ? '▼' : '▶'}</span>}
                    Span #{span.sequence_num}
                  </span>
                  {span.model && (
                    <span style={styles.traceModel}>{getModelShortName(span.model)}</span>
                  )}
                  {span.duration_ms && (
                    <span style={styles.traceDuration}>{formatDuration(span.duration_ms)}</span>
                  )}
                  <button
                    style={styles.traceLink}
                    onClick={(e) => {
                      e.stopPropagation();
                      navigateToTrace(span.session_id, span.span_id);
                    }}
                    title="View full span in Traces"
                  >
                    ↗ View
                  </button>
                </div>
                <div style={styles.traceSession}>
                  Session: <span
                    style={styles.sessionLink}
                    onClick={() => navigateToTrace(span.session_id, span.span_id)}
                  >{span.session_id.slice(0, 8)}</span>
                  {span.tool_names && (
                    <span style={styles.traceTools}> · Tools: {span.tool_names}</span>
                  )}
                </div>

                {/* Expanded content */}
                {isExpanded && (
                  <div style={styles.traceContent}>
                    {span.user_preview && (
                      <div style={styles.traceContentBlock}>
                        <div style={styles.traceContentLabel}>User</div>
                        <div style={styles.traceContentText}>{span.user_preview}</div>
                      </div>
                    )}
                    {span.thinking_preview && (
                      <div style={styles.traceContentBlock}>
                        <div style={styles.traceContentLabel}>Thinking</div>
                        <div style={styles.traceContentText}>{span.thinking_preview}</div>
                      </div>
                    )}
                    {span.response_preview && (
                      <div style={styles.traceContentBlock}>
                        <div style={styles.traceContentLabel}>Response</div>
                        <div style={styles.traceContentText}>{span.response_preview}</div>
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {incoming.length > 0 && (
        <div style={styles.section}>
          <h3 style={styles.sectionTitle}>Incoming ({incoming.length})</h3>
          {incoming.map(edge => (
            <div
              key={edge.id}
              style={styles.connection}
              onClick={() => onSelectNode(edge.from_node_id)}
            >
              <div style={styles.connectionHeader}>
                <EdgeBadge type={edge.edge_type} />
                <span style={styles.arrow}>←</span>
                <span style={styles.connectionTitle}>{getNodeTitle(edge.from_node_id)}</span>
              </div>
              {edge.rationale && (
                <div style={styles.connectionRationale}>{edge.rationale}</div>
              )}
            </div>
          ))}
        </div>
      )}

      {outgoing.length > 0 && (
        <div style={styles.section}>
          <h3 style={styles.sectionTitle}>Outgoing ({outgoing.length})</h3>
          {outgoing.map(edge => (
            <div
              key={edge.id}
              style={styles.connection}
              onClick={() => onSelectNode(edge.to_node_id)}
            >
              <div style={styles.connectionHeader}>
                <EdgeBadge type={edge.edge_type} />
                <span style={styles.arrow}>→</span>
                <span style={styles.connectionTitle}>{getNodeTitle(edge.to_node_id)}</span>
              </div>
              {edge.rationale && (
                <div style={styles.connectionRationale}>{edge.rationale}</div>
              )}
            </div>
          ))}
        </div>
      )}

      <div style={styles.navLinks}>
        <a href="../decision-graph" style={styles.link}>Learn about the graph →</a>
        <a href="../claude-tooling" style={styles.link}>See the tooling →</a>
      </div>
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  panel: {
    padding: '25px',
    height: '100%',
    overflowY: 'auto',
    backgroundColor: '#ffffff',
    position: 'relative',
  },
  empty: {
    textAlign: 'center',
    color: '#6e7781',
    paddingTop: '80px',
  },
  closeButton: {
    position: 'absolute',
    top: '15px',
    right: '15px',
    width: '30px',
    height: '30px',
    border: 'none',
    background: '#f6f8fa',
    color: '#57606a',
    borderRadius: '4px',
    fontSize: '20px',
    cursor: 'pointer',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
  },
  header: {
    marginBottom: '20px',
  },
  title: {
    fontSize: '24px',
    margin: '12px 0 8px 0',
    color: '#24292f',
  },
  meta: {
    display: 'flex',
    gap: '20px',
    fontSize: '14px',
    color: '#57606a',
    flexWrap: 'wrap',
  },
  description: {
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    padding: '20px',
    borderRadius: '8px',
    marginBottom: '20px',
    lineHeight: 1.6,
    color: '#24292f',
  },
  section: {
    marginTop: '20px',
  },
  sectionTitle: {
    fontSize: '16px',
    marginBottom: '12px',
    color: '#57606a',
  },
  connection: {
    padding: '12px',
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    marginBottom: '8px',
    cursor: 'pointer',
    transition: 'background-color 0.2s',
  },
  connectionHeader: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    flexWrap: 'wrap',
  },
  connectionTitle: {
    color: '#24292f',
    fontSize: '13px',
    flex: 1,
    minWidth: 0,
  },
  connectionRationale: {
    color: '#57606a',
    fontSize: '12px',
    marginTop: '8px',
    paddingTop: '8px',
    borderTop: '1px solid #d0d7de',
    lineHeight: 1.4,
  },
  arrow: {
    color: '#6e7781',
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
  prompt: {
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    padding: '15px',
    borderRadius: '6px',
    fontSize: '14px',
    color: '#24292f',
    lineHeight: 1.6,
    whiteSpace: 'pre-wrap',
    fontStyle: 'italic',
    borderLeft: '3px solid #0969da',
  },
  fileList: {
    display: 'flex',
    flexWrap: 'wrap',
    gap: '8px',
  },
  fileTag: {
    backgroundColor: '#ddf4ff',
    padding: '4px 10px',
    borderRadius: '4px',
    fontSize: '12px',
    color: '#0969da',
    fontFamily: 'monospace',
  },
  branchTag: {
    backgroundColor: '#dafbe1',
    color: '#1a7f37',
    padding: '4px 10px',
    borderRadius: '4px',
    fontSize: '12px',
    fontFamily: 'monospace',
  },
  commitSection: {
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    padding: '15px',
    borderRadius: '6px',
    borderLeft: '3px solid #0969da',
  },
  commitHash: {
    fontFamily: 'monospace',
    fontSize: '13px',
    color: '#0969da',
    textDecoration: 'none',
    backgroundColor: '#ddf4ff',
    padding: '3px 8px',
    borderRadius: '4px',
  },
  commitMessage: {
    fontSize: '14px',
    color: '#24292f',
    marginTop: '10px',
    lineHeight: 1.5,
    whiteSpace: 'pre-wrap',
  },
  commitMeta: {
    fontSize: '12px',
    color: '#57606a',
    marginTop: '8px',
  },
  traceInfo: {
    backgroundColor: '#fff8e6',
    border: '1px solid #f0d77a',
    padding: '12px',
    borderRadius: '6px',
    marginBottom: '8px',
    borderLeft: '3px solid #d4a72c',
  },
  traceHeader: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    flexWrap: 'wrap',
  },
  traceSpan: {
    fontWeight: 500,
    color: '#24292f',
    fontSize: '13px',
  },
  traceModel: {
    backgroundColor: '#8b5cf6',
    color: '#fff',
    padding: '2px 6px',
    borderRadius: '4px',
    fontSize: '11px',
  },
  traceDuration: {
    color: '#57606a',
    fontSize: '12px',
  },
  traceSession: {
    marginTop: '6px',
    fontSize: '12px',
    color: '#57606a',
    fontFamily: 'monospace',
  },
  sessionLink: {
    color: '#0969da',
    cursor: 'pointer',
    textDecoration: 'underline',
  },
  traceLink: {
    marginLeft: 'auto',
    padding: '2px 6px',
    border: '1px solid #d4a72c',
    borderRadius: '4px',
    backgroundColor: 'transparent',
    color: '#d4a72c',
    cursor: 'pointer',
    fontSize: '12px',
    fontWeight: 600,
  },
  traceTools: {
    color: '#6e7781',
  },
  traceContent: {
    marginTop: '10px',
    paddingTop: '10px',
    borderTop: '1px solid #f0d77a',
  },
  traceContentBlock: {
    marginBottom: '10px',
  },
  traceContentLabel: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#57606a',
    textTransform: 'uppercase',
    marginBottom: '4px',
  },
  traceContentText: {
    fontSize: '12px',
    color: '#24292f',
    lineHeight: 1.5,
    whiteSpace: 'pre-wrap',
    backgroundColor: '#fffef5',
    padding: '8px',
    borderRadius: '4px',
    maxHeight: '150px',
    overflow: 'auto',
  },
};
