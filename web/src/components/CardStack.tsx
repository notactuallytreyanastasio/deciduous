/**
 * CardStack Component
 *
 * Stacked card display with keyboard navigation for archaeology view.
 * Cards are "collapsed" by default (showing title), expanded on Enter/Tab.
 * Navigate with j/k or up/down arrows.
 */

import React, { useEffect, useCallback, useRef } from 'react';
import type { DecisionNode } from '../types/graph';
import { getNodeColor } from '../utils/colors';
import { getConfidence, getCommit, getPrompt } from '../types/graph';

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
        e.preventDefault();
        if (expandedIndex !== null) {
          onExpandIndex(null);
        } else {
          onClose();
        }
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
    <div ref={containerRef} style={styles.container}>
      {/* Header */}
      <div style={styles.header}>
        <span style={styles.headerTitle}>
          {nodes.length} node{nodes.length > 1 ? 's' : ''} in narrative
        </span>
        <span style={styles.headerHint}>
          j/k navigate &bull; Enter expand &bull; Esc close
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

                  {/* Description if present */}
                  {node.description && (
                    <div style={styles.descSection}>
                      <div style={styles.sectionLabel}>Description:</div>
                      <div style={styles.descText}>{node.description}</div>
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
};
