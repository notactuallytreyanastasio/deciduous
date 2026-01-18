/**
 * CardStack Component
 *
 * Stacked card display with keyboard navigation for archaeology view.
 * Cards are "collapsed" by default (showing title), expanded on Enter/Tab.
 * Navigate with j/k or up/down arrows.
 */

import React, { useEffect, useCallback, useRef, useState, useMemo } from 'react';
import type { DecisionNode, DecisionEdge } from '../types/graph';
import { getNodeColor } from '../utils/colors';
import { getConfidence, getCommit, getPrompt, getFiles, getBranch } from '../types/graph';
import { useIsMobile } from '../hooks/useMediaQuery';

interface CardStackProps {
  nodes: DecisionNode[];
  edges: DecisionEdge[];
  selectedIndex: number;
  expandedIndex: number | null;
  onSelectIndex: (index: number) => void;
  onExpandIndex: (index: number | null) => void;
  onNodeClick: (nodeId: number) => void;
  onClose: () => void;
}

export const CardStack: React.FC<CardStackProps> = ({
  nodes,
  edges,
  selectedIndex,
  expandedIndex,
  onSelectIndex,
  onExpandIndex,
  onNodeClick,
  onClose,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const isMobile = useIsMobile();

  // Build node map for quick lookups
  const nodeMap = useMemo(() => new Map(nodes.map(n => [n.id, n])), [nodes]);

  // Get parent nodes (nodes that link TO this node)
  const getParentNodes = useCallback((nodeId: number) => {
    return edges
      .filter(e => e.to_node_id === nodeId)
      .map(e => ({ node: nodeMap.get(e.from_node_id), edge: e }))
      .filter((item): item is { node: DecisionNode; edge: DecisionEdge } => item.node !== undefined);
  }, [edges, nodeMap]);

  // Get child nodes (nodes this node links TO)
  const getChildNodes = useCallback((nodeId: number) => {
    return edges
      .filter(e => e.from_node_id === nodeId)
      .map(e => ({ node: nodeMap.get(e.to_node_id), edge: e }))
      .filter((item): item is { node: DecisionNode; edge: DecisionEdge } => item.node !== undefined);
  }, [edges, nodeMap]);

  // Touch swipe state
  const [touchStart, setTouchStart] = useState<{ x: number; y: number } | null>(null);
  const [touchEnd, setTouchEnd] = useState<{ x: number; y: number } | null>(null);

  // Minimum swipe distance to trigger navigation
  const minSwipeDistance = 50;

  const handleTouchStart = useCallback((e: React.TouchEvent) => {
    setTouchEnd(null);
    setTouchStart({
      x: e.targetTouches[0].clientX,
      y: e.targetTouches[0].clientY,
    });
  }, []);

  const handleTouchMove = useCallback((e: React.TouchEvent) => {
    setTouchEnd({
      x: e.targetTouches[0].clientX,
      y: e.targetTouches[0].clientY,
    });
  }, []);

  const handleTouchEnd = useCallback(() => {
    if (!touchStart || !touchEnd) return;

    const distanceX = touchStart.x - touchEnd.x;
    const distanceY = touchStart.y - touchEnd.y;
    const isHorizontalSwipe = Math.abs(distanceX) > Math.abs(distanceY);

    if (isHorizontalSwipe && Math.abs(distanceX) > minSwipeDistance) {
      if (distanceX > 0) {
        // Swiped left - next card
        onSelectIndex(Math.min(selectedIndex + 1, nodes.length - 1));
      } else {
        // Swiped right - previous card
        onSelectIndex(Math.max(selectedIndex - 1, 0));
      }
    } else if (!isHorizontalSwipe && Math.abs(distanceY) > minSwipeDistance) {
      if (distanceY < 0) {
        // Swiped down - close
        onClose();
      } else {
        // Swiped up - expand
        onExpandIndex(expandedIndex === selectedIndex ? null : selectedIndex);
      }
    }

    setTouchStart(null);
    setTouchEnd(null);
  }, [touchStart, touchEnd, selectedIndex, expandedIndex, nodes.length, onSelectIndex, onExpandIndex, onClose]);

  // Scroll selected card into view
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const selectedCard = container.children[selectedIndex] as HTMLElement;
    if (selectedCard) {
      selectedCard.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    }
  }, [selectedIndex]);

  if (nodes.length === 0) return null;

  return (
    <div
      ref={containerRef}
      style={{
        ...styles.container,
        ...(isMobile ? styles.containerMobile : {}),
      }}
      onTouchStart={handleTouchStart}
      onTouchMove={handleTouchMove}
      onTouchEnd={handleTouchEnd}
    >
      {/* Header */}
      <div style={styles.header}>
        <span style={styles.headerTitle}>
          {nodes.length} node{nodes.length > 1 ? 's' : ''} in narrative
        </span>
        <span style={styles.headerHint}>
          {isMobile ? 'Swipe to navigate' : 'j/k nav · Space expand · q close'}
        </span>
        <button style={styles.closeBtn} onClick={onClose}>
          &times;
        </button>
      </div>

      {/* Card stack */}
      <div style={styles.stack}>
        {nodes.map((node, index) => {
          const isSelected = index === selectedIndex;
          const isExpanded = index === expandedIndex;
          const isPeeking = !isExpanded && Math.abs(index - selectedIndex) <= 2;
          const offset = index - selectedIndex;

          // Selected cards automatically show full details
          const showFullDetails = isSelected || isExpanded;

          // Parse metadata once for efficiency
          const confidence = getConfidence(node);
          const commit = getCommit(node);
          const prompt = getPrompt(node);
          const files = getFiles(node);
          const branch = getBranch(node);

          // Extract additional metadata
          let githubPr: string | number | undefined;
          let githubIssue: string | number | undefined;
          let githubRepo: string | undefined;
          try {
            if (node.metadata_json) {
              const meta = JSON.parse(node.metadata_json);
              githubPr = meta.github_pr;
              githubIssue = meta.github_issue;
              githubRepo = meta.github_repo;
            }
          } catch {
            // Ignore parse errors
          }

          return (
            <div
              key={node.id}
              style={{
                ...styles.card,
                ...(isSelected ? styles.cardSelected : {}),
                ...(showFullDetails ? styles.cardExpanded : {}),
                borderLeftColor: getNodeColor(node.node_type),
                transform: showFullDetails
                  ? 'translateY(0)'
                  : `translateY(${offset * 8}px) scale(${1 - Math.abs(offset) * 0.02})`,
                zIndex: showFullDetails ? 100 : (100 - Math.abs(offset)),
                opacity: isPeeking || showFullDetails ? 1 : 0.6,
              }}
              onClick={() => {
                onSelectIndex(index);
                // Toggle expand if clicking on already-selected card
                if (isSelected) {
                  onExpandIndex(isExpanded ? null : index);
                }
              }}
            >
              {/* Header - always visible */}
              <div style={styles.cardHeader}>
                <span
                  style={{
                    ...styles.typeBadge,
                    backgroundColor: getNodeColor(node.node_type) + '22',
                    color: getNodeColor(node.node_type),
                  }}
                >
                  {node.node_type.toUpperCase()}
                </span>
                <span style={styles.nodeId}>#{node.id}</span>
                {/* Status badge */}
                <span style={{
                  ...styles.statusBadge,
                  backgroundColor: node.status === 'active' ? '#dafbe1' :
                                   node.status === 'completed' ? '#ddf4ff' :
                                   node.status === 'rejected' ? '#ffebe9' : '#f6f8fa',
                  color: node.status === 'active' ? '#1a7f37' :
                         node.status === 'completed' ? '#0969da' :
                         node.status === 'rejected' ? '#cf222e' : '#57606a',
                }}>
                  {node.status}
                </span>
                {confidence !== null && (
                  <span style={styles.confidence}>{confidence}%</span>
                )}
              </div>

              <h4 style={styles.cardTitle}>{node.title}</h4>

              {/* Description - always show full text, no truncation */}
              {node.description && (
                <p style={styles.cardDescription}>{node.description}</p>
              )}

              {/* Collapsed view - minimal metadata */}
              {!showFullDetails && (
                <>
                  {/* Files and branch - show first 2 */}
                  {(files?.length || branch) && (
                    <div style={styles.metaRow}>
                      {branch && (
                        <span style={styles.branchTag}>{branch}</span>
                      )}
                      {files?.slice(0, 2).map((file, i) => (
                        <span key={i} style={styles.fileTag}>{file}</span>
                      ))}
                      {(files?.length ?? 0) > 2 && (
                        <span style={styles.moreFiles}>+{(files?.length ?? 0) - 2}</span>
                      )}
                    </div>
                  )}
                </>
              )}

              {/* Full details view - shown when selected */}
              {showFullDetails && (
                <div style={styles.expandedContent}>
                  {/* Branch - show prominently at top */}
                  {branch && (
                    <div style={styles.branchSection}>
                      <div style={styles.sectionLabel}>Branch:</div>
                      <span style={styles.branchTagLarge}>{branch}</span>
                    </div>
                  )}

                  {/* Prompt if present - full text, no truncation */}
                  {prompt && (
                    <div style={styles.promptSection}>
                      <div style={styles.sectionLabel}>Prompt:</div>
                      <div style={styles.promptText}>{prompt}</div>
                    </div>
                  )}

                  {/* All files - always show full list */}
                  {files && files.length > 0 && (
                    <div style={styles.filesSection}>
                      <div style={styles.sectionLabel}>Files ({files.length}):</div>
                      <div style={styles.filesList}>
                        {files.map((file, i) => (
                          <span key={i} style={styles.fileTag}>{file}</span>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Commit if present - show full hash */}
                  {commit && (
                    <div style={styles.commitSection}>
                      <div style={styles.sectionLabel}>Commit:</div>
                      <span style={styles.commitBadge}>{commit}</span>
                    </div>
                  )}

                  {/* GitHub Links */}
                  {(githubPr || githubIssue) && (
                    <div style={styles.githubSection}>
                      <div style={styles.sectionLabel}>GitHub:</div>
                      <div style={styles.githubLinks}>
                        {githubPr && (
                          <a
                            href={`https://github.com/${githubRepo || 'owner/repo'}/pull/${githubPr}`}
                            target="_blank"
                            rel="noopener noreferrer"
                            style={styles.githubLink}
                            onClick={(e) => e.stopPropagation()}
                          >
                            PR #{githubPr}
                          </a>
                        )}
                        {githubIssue && (
                          <a
                            href={`https://github.com/${githubRepo || 'owner/repo'}/issues/${githubIssue}`}
                            target="_blank"
                            rel="noopener noreferrer"
                            style={styles.githubLink}
                            onClick={(e) => e.stopPropagation()}
                          >
                            Issue #{githubIssue}
                          </a>
                        )}
                      </div>
                    </div>
                  )}

                  {/* Change ID (UUID) */}
                  {node.change_id && (
                    <div style={styles.changeIdSection}>
                      <div style={styles.sectionLabel}>Change ID:</div>
                      <span style={styles.changeIdBadge}>{node.change_id}</span>
                    </div>
                  )}

                  {/* Timestamps - show full datetime */}
                  <div style={styles.timestampsSection}>
                    <div style={styles.sectionLabel}>Timestamps:</div>
                    <div style={styles.timestamps}>
                      <span>Created: {new Date(node.created_at).toLocaleString()}</span>
                      <span>Updated: {new Date(node.updated_at).toLocaleString()}</span>
                    </div>
                  </div>

                  {/* Parent nodes (incoming links) */}
                  {(() => {
                    const parents = getParentNodes(node.id);
                    return (
                      <div style={styles.linksSection}>
                        <div style={styles.sectionLabel}>
                          Parent Nodes ({parents.length}):
                        </div>
                        {parents.length === 0 ? (
                          <div style={styles.noLinks}>No parent nodes</div>
                        ) : (
                          <div style={styles.linksList}>
                            {parents.map(({ node: parentNode, edge }) => (
                              <button
                                key={parentNode.id}
                                style={{
                                  ...styles.linkedNode,
                                  borderLeftColor: getNodeColor(parentNode.node_type),
                                }}
                                onClick={(e) => {
                                  e.stopPropagation();
                                  onNodeClick(parentNode.id);
                                }}
                              >
                                <span style={{
                                  ...styles.linkedTypeBadge,
                                  backgroundColor: getNodeColor(parentNode.node_type) + '22',
                                  color: getNodeColor(parentNode.node_type),
                                }}>
                                  {parentNode.node_type}
                                </span>
                                <span style={styles.linkedTitle}>#{parentNode.id}: {parentNode.title}</span>
                                {edge.rationale && (
                                  <span style={styles.linkRationale}>{edge.rationale}</span>
                                )}
                              </button>
                            ))}
                          </div>
                        )}
                      </div>
                    );
                  })()}

                  {/* Child nodes (outgoing links) */}
                  {(() => {
                    const children = getChildNodes(node.id);
                    return (
                      <div style={styles.linksSection}>
                        <div style={styles.sectionLabel}>
                          Child Nodes ({children.length}):
                        </div>
                        {children.length === 0 ? (
                          <div style={styles.noLinks}>No child nodes</div>
                        ) : (
                          <div style={styles.linksList}>
                            {children.map(({ node: childNode, edge }) => (
                              <button
                                key={childNode.id}
                                style={{
                                  ...styles.linkedNode,
                                  borderLeftColor: getNodeColor(childNode.node_type),
                                }}
                                onClick={(e) => {
                                  e.stopPropagation();
                                  onNodeClick(childNode.id);
                                }}
                              >
                                <span style={{
                                  ...styles.linkedTypeBadge,
                                  backgroundColor: getNodeColor(childNode.node_type) + '22',
                                  color: getNodeColor(childNode.node_type),
                                }}>
                                  {childNode.node_type}
                                </span>
                                <span style={styles.linkedTitle}>#{childNode.id}: {childNode.title}</span>
                                {edge.rationale && (
                                  <span style={styles.linkRationale}>{edge.rationale}</span>
                                )}
                              </button>
                            ))}
                          </div>
                        )}
                      </div>
                    );
                  })()}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    position: 'absolute',
    right: '20px',
    top: '10px',
    bottom: '10px',
    width: '420px',
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: 'rgba(255, 255, 255, 0.98)',
    borderRadius: '12px',
    boxShadow: '0 8px 32px rgba(0,0,0,0.2)',
    overflow: 'hidden',
    zIndex: 1000,
  },
  header: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    padding: '12px 16px',
    borderBottom: '1px solid #e1e4e8',
    backgroundColor: '#f6f8fa',
  },
  headerTitle: {
    fontSize: '13px',
    fontWeight: 600,
    color: '#24292f',
  },
  headerHint: {
    fontSize: '11px',
    color: '#8c959f',
    marginLeft: 'auto',
  },
  closeBtn: {
    background: 'none',
    border: 'none',
    fontSize: '20px',
    cursor: 'pointer',
    color: '#57606a',
    padding: '0 4px',
  },
  stack: {
    flex: 1,
    padding: '16px',
    overflowY: 'auto',
    display: 'flex',
    flexDirection: 'column',
    gap: '8px',
  },
  card: {
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderLeft: '4px solid',
    borderRadius: '8px',
    padding: '12px 16px',
    cursor: 'pointer',
    transition: 'all 0.2s ease',
    flexShrink: 0,
  },
  cardSelected: {
    boxShadow: '0 6px 20px rgba(9, 105, 218, 0.25), 0 2px 8px rgba(0,0,0,0.1)',
    borderColor: '#0969da',
    borderWidth: '2px',
    backgroundColor: '#fafcff',
  },
  cardExpanded: {
    padding: '16px',
  },
  cardHeader: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    marginBottom: '6px',
  },
  typeBadge: {
    fontSize: '10px',
    fontWeight: 600,
    textTransform: 'uppercase',
    padding: '2px 6px',
    borderRadius: '4px',
  },
  nodeId: {
    fontSize: '11px',
    color: '#8c959f',
  },
  confidence: {
    fontSize: '11px',
    color: '#57606a',
    marginLeft: 'auto',
    backgroundColor: '#f6f8fa',
    padding: '2px 6px',
    borderRadius: '4px',
  },
  cardTitle: {
    margin: 0,
    fontSize: '14px',
    fontWeight: 500,
    color: '#24292f',
    lineHeight: 1.4,
  },
  cardDescription: {
    margin: '6px 0 0 0',
    fontSize: '13px',
    color: '#57606a',
    lineHeight: 1.5,
  },
  metaRow: {
    display: 'flex',
    flexWrap: 'wrap',
    gap: '6px',
    marginTop: '8px',
  },
  branchTag: {
    fontSize: '11px',
    fontWeight: 500,
    backgroundColor: '#dafbe1',
    color: '#1a7f37',
    padding: '2px 8px',
    borderRadius: '10px',
  },
  fileTag: {
    fontSize: '10px',
    fontFamily: 'monospace',
    backgroundColor: '#f6f8fa',
    padding: '2px 6px',
    borderRadius: '4px',
    color: '#0969da',
  },
  moreFiles: {
    fontSize: '10px',
    color: '#8c959f',
    padding: '2px 4px',
  },
  expandedContent: {
    marginTop: '12px',
    paddingTop: '12px',
    borderTop: '1px solid #e1e4e8',
  },
  sectionLabel: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#57606a',
    textTransform: 'uppercase',
    marginBottom: '4px',
  },
  promptSection: {
    marginBottom: '12px',
    backgroundColor: '#f0f6fc',
    padding: '10px',
    borderRadius: '6px',
    borderLeft: '3px solid #0969da',
  },
  promptText: {
    fontSize: '13px',
    color: '#24292f',
    lineHeight: 1.5,
    fontStyle: 'italic',
    whiteSpace: 'pre-wrap',
    maxHeight: '150px',
    overflowY: 'auto',
  },
  filesSection: {
    marginBottom: '12px',
  },
  filesList: {
    display: 'flex',
    flexWrap: 'wrap',
    gap: '6px',
    maxHeight: '100px',
    overflowY: 'auto',
  },
  descSection: {
    marginBottom: '12px',
  },
  descText: {
    fontSize: '13px',
    color: '#24292f',
    lineHeight: 1.5,
  },
  commitSection: {
    marginBottom: '12px',
  },
  commitBadge: {
    fontSize: '12px',
    fontFamily: 'monospace',
    backgroundColor: '#f6f8fa',
    padding: '4px 8px',
    borderRadius: '4px',
    color: '#0969da',
  },
  timestamps: {
    display: 'flex',
    flexDirection: 'column',
    gap: '4px',
    fontSize: '11px',
    color: '#8c959f',
  },
  linksSection: {
    marginTop: '12px',
    paddingTop: '12px',
    borderTop: '1px solid #e1e4e8',
  },
  linksList: {
    display: 'flex',
    flexDirection: 'column',
    gap: '6px',
    maxHeight: '200px',
    overflowY: 'auto',
  },
  linkedNode: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'flex-start',
    gap: '4px',
    padding: '8px 10px',
    backgroundColor: '#f6f8fa',
    border: 'none',
    borderLeft: '3px solid',
    borderRadius: '6px',
    cursor: 'pointer',
    textAlign: 'left',
    width: '100%',
    transition: 'background-color 0.15s ease',
  },
  linkedTypeBadge: {
    fontSize: '9px',
    fontWeight: 600,
    textTransform: 'uppercase',
    padding: '1px 5px',
    borderRadius: '3px',
  },
  linkedTitle: {
    fontSize: '12px',
    fontWeight: 500,
    color: '#24292f',
    lineHeight: 1.3,
  },
  linkRationale: {
    fontSize: '11px',
    color: '#57606a',
    fontStyle: 'italic',
  },
  noLinks: {
    fontSize: '12px',
    color: '#8c959f',
    fontStyle: 'italic',
    padding: '4px 0',
  },
  // Status badge
  statusBadge: {
    fontSize: '10px',
    fontWeight: 600,
    textTransform: 'uppercase',
    padding: '2px 6px',
    borderRadius: '4px',
  },
  // Branch section for expanded view
  branchSection: {
    marginBottom: '12px',
  },
  branchTagLarge: {
    fontSize: '12px',
    fontWeight: 600,
    backgroundColor: '#dafbe1',
    color: '#1a7f37',
    padding: '4px 10px',
    borderRadius: '6px',
    fontFamily: 'monospace',
  },
  // GitHub links section
  githubSection: {
    marginBottom: '12px',
  },
  githubLinks: {
    display: 'flex',
    flexWrap: 'wrap',
    gap: '8px',
  },
  githubLink: {
    fontSize: '12px',
    color: '#0969da',
    backgroundColor: '#ddf4ff',
    padding: '4px 10px',
    borderRadius: '6px',
    textDecoration: 'none',
    fontWeight: 500,
  },
  // Change ID section
  changeIdSection: {
    marginBottom: '12px',
  },
  changeIdBadge: {
    fontSize: '11px',
    fontFamily: 'monospace',
    backgroundColor: '#f6f8fa',
    padding: '4px 8px',
    borderRadius: '4px',
    color: '#57606a',
    wordBreak: 'break-all',
    display: 'inline-block',
  },
  // Timestamps section
  timestampsSection: {
    marginBottom: '12px',
  },

  // Mobile responsive styles
  containerMobile: {
    position: 'fixed',
    left: '10px',
    right: '10px',
    top: '70px',
    bottom: '10px',
    width: 'auto',
    borderRadius: '16px',
    zIndex: 1100,
  },
};
