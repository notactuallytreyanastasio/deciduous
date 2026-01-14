/**
 * CardStack Component
 *
 * Stacked card display with keyboard navigation for archaeology view.
 * Cards are "collapsed" by default (showing title), expanded on Enter/Tab.
 * Navigate with j/k or up/down arrows.
 */

import React, { useEffect, useCallback, useRef, useState } from 'react';
import type { DecisionNode } from '../types/graph';
import { getNodeColor } from '../utils/colors';
import { getConfidence, getCommit, getPrompt, getFiles, getBranch } from '../types/graph';
import { useIsMobile } from '../hooks/useMediaQuery';

interface CardStackProps {
  nodes: DecisionNode[];
  selectedIndex: number;
  expandedIndex: number | null;
  onSelectIndex: (index: number) => void;
  onExpandIndex: (index: number | null) => void;
  onClose: () => void;
}

export const CardStack: React.FC<CardStackProps> = ({
  nodes,
  selectedIndex,
  expandedIndex,
  onSelectIndex,
  onExpandIndex,
  onClose,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const isMobile = useIsMobile();

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

  // Keyboard navigation
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (nodes.length === 0) return;

    switch (e.key) {
      case 'j':
      case 'ArrowDown':
        e.preventDefault();
        onSelectIndex(Math.min(selectedIndex + 1, nodes.length - 1));
        break;
      case 'k':
      case 'ArrowUp':
        e.preventDefault();
        onSelectIndex(Math.max(selectedIndex - 1, 0));
        break;
      case 'Enter':
      case 'Tab':
        e.preventDefault();
        // Toggle expansion
        onExpandIndex(expandedIndex === selectedIndex ? null : selectedIndex);
        break;
      case 'Escape':
      case 'q':
        e.preventDefault();
        if (expandedIndex !== null) {
          onExpandIndex(null);
        } else {
          onClose();
        }
        break;
      // Jump to first/last
      case 'Home':
      case 'g':
        e.preventDefault();
        onSelectIndex(0);
        break;
      case 'End':
      case 'G':
        e.preventDefault();
        onSelectIndex(nodes.length - 1);
        break;
      // Jump by page (10 items)
      case 'PageDown':
        e.preventDefault();
        onSelectIndex(Math.min(selectedIndex + 10, nodes.length - 1));
        break;
      case 'PageUp':
        e.preventDefault();
        onSelectIndex(Math.max(selectedIndex - 10, 0));
        break;
      // Number keys 1-9 for quick jump
      case '1': case '2': case '3': case '4': case '5':
      case '6': case '7': case '8': case '9':
        e.preventDefault();
        const jumpIndex = parseInt(e.key, 10) - 1;
        if (jumpIndex < nodes.length) {
          onSelectIndex(jumpIndex);
        }
        break;
      // Spacebar to expand/collapse
      case ' ':
        e.preventDefault();
        onExpandIndex(expandedIndex === selectedIndex ? null : selectedIndex);
        break;
    }
  }, [nodes.length, selectedIndex, expandedIndex, onSelectIndex, onExpandIndex, onClose]);

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

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
          {isMobile ? 'Swipe to navigate' : 'j/k nav \u2022 Space expand \u2022 q close'}
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

          return (
            <div
              key={node.id}
              style={{
                ...styles.card,
                ...(isSelected ? styles.cardSelected : {}),
                ...(isExpanded ? styles.cardExpanded : {}),
                borderLeftColor: getNodeColor(node.node_type),
                transform: isExpanded
                  ? 'translateY(0)'
                  : `translateY(${offset * 8}px) scale(${1 - Math.abs(offset) * 0.02})`,
                zIndex: isExpanded ? 100 : (100 - Math.abs(offset)),
                opacity: isPeeking || isExpanded ? 1 : 0.6,
              }}
              onClick={() => {
                onSelectIndex(index);
                if (isSelected) {
                  onExpandIndex(isExpanded ? null : index);
                }
              }}
            >
              {/* Collapsed view - just header */}
              <div style={styles.cardHeader}>
                <span
                  style={{
                    ...styles.typeBadge,
                    backgroundColor: getNodeColor(node.node_type) + '22',
                    color: getNodeColor(node.node_type),
                  }}
                >
                  {node.node_type}
                </span>
                <span style={styles.nodeId}>#{node.id}</span>
                {getConfidence(node) && (
                  <span style={styles.confidence}>{getConfidence(node)}%</span>
                )}
              </div>

              <h4 style={styles.cardTitle}>{node.title}</h4>

              {/* Description - always show if present */}
              {node.description && (
                <p style={styles.cardDescription}>{node.description}</p>
              )}

              {/* Files and branch - always show if present */}
              {(getFiles(node)?.length || getBranch(node)) && (
                <div style={styles.metaRow}>
                  {getBranch(node) && (
                    <span style={styles.branchTag}>{getBranch(node)}</span>
                  )}
                  {getFiles(node)?.slice(0, 2).map((file, i) => (
                    <span key={i} style={styles.fileTag}>{file}</span>
                  ))}
                  {(getFiles(node)?.length ?? 0) > 2 && (
                    <span style={styles.moreFiles}>+{(getFiles(node)?.length ?? 0) - 2}</span>
                  )}
                </div>
              )}

              {/* Expanded view - full details */}
              {isExpanded && (
                <div style={styles.expandedContent}>
                  {/* Prompt if present */}
                  {getPrompt(node) && (
                    <div style={styles.promptSection}>
                      <div style={styles.sectionLabel}>Prompt:</div>
                      <div style={styles.promptText}>{getPrompt(node)}</div>
                    </div>
                  )}

                  {/* All files if present */}
                  {getFiles(node) && getFiles(node)!.length > 2 && (
                    <div style={styles.filesSection}>
                      <div style={styles.sectionLabel}>All Files:</div>
                      <div style={styles.filesList}>
                        {getFiles(node)!.map((file, i) => (
                          <span key={i} style={styles.fileTag}>{file}</span>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Commit if present */}
                  {getCommit(node) && (
                    <div style={styles.commitSection}>
                      <span style={styles.commitBadge}>
                        Commit: {getCommit(node)}
                      </span>
                    </div>
                  )}

                  {/* Timestamps */}
                  <div style={styles.timestamps}>
                    <span>Created: {new Date(node.created_at).toLocaleDateString()}</span>
                    <span>Updated: {new Date(node.updated_at).toLocaleDateString()}</span>
                  </div>
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
    top: '20px',
    bottom: '20px',
    width: '380px',
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
    boxShadow: '0 4px 12px rgba(0,0,0,0.15)',
    borderColor: '#0969da',
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
  },
  filesSection: {
    marginBottom: '12px',
  },
  filesList: {
    display: 'flex',
    flexWrap: 'wrap',
    gap: '6px',
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
    gap: '16px',
    fontSize: '11px',
    color: '#8c959f',
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
