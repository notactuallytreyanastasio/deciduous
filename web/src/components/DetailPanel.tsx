/**
 * Detail Panel Component
 *
 * Shared detail panel for displaying node information.
 * Used by all views.
 */

import React from 'react';
import type { DecisionNode, GraphData, GitCommit } from '../types/graph';
import { getPrompt, getFiles, getBranch, getCommit, shortCommit, githubCommitUrl } from '../types/graph';
import { NodeBadges, EdgeBadge, StatusBadge } from './NodeBadge';

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
  repo = 'notactuallytreyanastasio/deciduous',
  gitHistory = [],
}) => {
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
              href={githubCommitUrl(commitHash, repo)}
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
};
