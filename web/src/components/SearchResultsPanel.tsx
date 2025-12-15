/**
 * Search Results Panel
 *
 * A floating side panel that displays search results categorized by visibility.
 * Shows which matches are visible, too small to read, or off-screen.
 */

import React, { useCallback, useState } from 'react';
import type { DecisionNode } from '../types/graph';
import { truncate } from '../types/graph';
import { getNodeColor } from '../utils/colors';
import type { NodeVisibility } from '../hooks/useNodeVisibility';

interface SearchResult {
  node: DecisionNode;
  matchType: 'title' | 'description' | 'commit' | 'prompt' | 'files';
  matchText: string;
}

interface SearchResultsPanelProps {
  results: SearchResult[];
  visibilityMap: Map<number, { visibility: NodeVisibility }>;
  onSelectNode: (node: DecisionNode) => void;
  onNavigateToNode: (node: DecisionNode) => void;
  onFocusResults: () => void;
  isOpen: boolean;
  onClose: () => void;
}

type FilterMode = 'all' | 'visible' | 'hidden';

export const SearchResultsPanel: React.FC<SearchResultsPanelProps> = ({
  results,
  visibilityMap,
  onSelectNode,
  onNavigateToNode,
  onFocusResults,
  isOpen,
  onClose,
}) => {
  const [filterMode, setFilterMode] = useState<FilterMode>('all');
  const [selectedIndex, setSelectedIndex] = useState(0);

  // Categorize results by visibility
  const categorizedResults = results.map((result) => {
    const visibility = visibilityMap.get(result.node.id)?.visibility ?? 'off-screen';
    return { ...result, visibility };
  });

  // Filter based on current mode
  const filteredResults = categorizedResults.filter((result) => {
    if (filterMode === 'all') return true;
    if (filterMode === 'visible') return result.visibility === 'visible';
    return result.visibility !== 'visible'; // hidden = too-small or off-screen
  });

  // Counts
  const visibleCount = categorizedResults.filter((r) => r.visibility === 'visible').length;
  const hiddenCount = categorizedResults.filter((r) => r.visibility !== 'visible').length;

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault();
          setSelectedIndex((prev) => Math.min(prev + 1, filteredResults.length - 1));
          break;
        case 'ArrowUp':
          e.preventDefault();
          setSelectedIndex((prev) => Math.max(prev - 1, 0));
          break;
        case 'Enter':
          e.preventDefault();
          if (filteredResults[selectedIndex]) {
            onSelectNode(filteredResults[selectedIndex].node);
          }
          break;
        case 'g':
          // 'g' for go to / navigate
          e.preventDefault();
          if (filteredResults[selectedIndex]) {
            onNavigateToNode(filteredResults[selectedIndex].node);
          }
          break;
        case 'Escape':
          e.preventDefault();
          onClose();
          break;
      }
    },
    [filteredResults, selectedIndex, onSelectNode, onNavigateToNode, onClose]
  );

  if (!isOpen || results.length === 0) {
    return null;
  }

  return (
    <div style={styles.panel} onKeyDown={handleKeyDown} tabIndex={0}>
      {/* Header */}
      <div style={styles.header}>
        <div style={styles.headerTitle}>
          <span style={styles.matchCount}>{results.length}</span>
          <span style={styles.headerText}>Search Results</span>
        </div>
        <button onClick={onClose} style={styles.closeBtn} title="Close panel">
          ×
        </button>
      </div>

      {/* Filter Tabs */}
      <div style={styles.filterTabs}>
        <button
          onClick={() => {
            setFilterMode('all');
            setSelectedIndex(0);
          }}
          style={{
            ...styles.filterTab,
            ...(filterMode === 'all' ? styles.filterTabActive : {}),
          }}
        >
          All ({results.length})
        </button>
        <button
          onClick={() => {
            setFilterMode('visible');
            setSelectedIndex(0);
          }}
          style={{
            ...styles.filterTab,
            ...(filterMode === 'visible' ? styles.filterTabActive : {}),
          }}
        >
          Visible ({visibleCount})
        </button>
        <button
          onClick={() => {
            setFilterMode('hidden');
            setSelectedIndex(0);
          }}
          style={{
            ...styles.filterTab,
            ...(filterMode === 'hidden' ? styles.filterTabActive : {}),
          }}
        >
          Hidden ({hiddenCount})
        </button>
      </div>

      {/* Focus Button */}
      {results.length > 1 && (
        <button onClick={onFocusResults} style={styles.focusBtn}>
          Focus on these {results.length} results
        </button>
      )}

      {/* Results List */}
      <div style={styles.resultsList}>
        {filteredResults.map((result, index) => (
          <div
            key={result.node.id}
            style={{
              ...styles.resultItem,
              ...(index === selectedIndex ? styles.resultItemSelected : {}),
            }}
            onClick={() => onSelectNode(result.node)}
          >
            <div style={styles.resultMain}>
              <div
                style={{
                  ...styles.nodeTypeDot,
                  backgroundColor: getNodeColor(result.node.node_type),
                }}
              />
              <span style={styles.nodeId}>#{result.node.id}</span>
              <span style={styles.resultTitle}>{truncate(result.node.title, 25)}</span>
            </div>
            <div style={styles.resultMeta}>
              <VisibilityBadge visibility={result.visibility} />
              {result.visibility !== 'visible' && (
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onNavigateToNode(result.node);
                  }}
                  style={styles.navigateBtn}
                  title="Navigate to this node"
                >
                  Go
                </button>
              )}
            </div>
          </div>
        ))}

        {filteredResults.length === 0 && (
          <div style={styles.emptyState}>
            No {filterMode === 'visible' ? 'visible' : 'hidden'} results
          </div>
        )}
      </div>

      {/* Footer hints */}
      <div style={styles.footer}>
        <span style={styles.footerHint}>↑↓ navigate · Enter view · G go to · Esc close</span>
      </div>
    </div>
  );
};

// Visibility badge component
const VisibilityBadge: React.FC<{ visibility: NodeVisibility }> = ({ visibility }) => {
  const config = {
    visible: { label: 'Visible', color: '#2da44e', bg: '#dafbe1' },
    'too-small': { label: 'Small', color: '#bf8700', bg: '#fff8c5' },
    'off-screen': { label: 'Off-screen', color: '#cf222e', bg: '#ffebe9' },
  };
  const { label, color, bg } = config[visibility];

  return (
    <span
      style={{
        ...styles.visibilityBadge,
        color,
        backgroundColor: bg,
      }}
    >
      {label}
    </span>
  );
};

const styles: Record<string, React.CSSProperties> = {
  panel: {
    position: 'absolute',
    top: '70px',
    right: '20px',
    width: '320px',
    maxHeight: 'calc(100vh - 140px)',
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderRadius: '12px',
    boxShadow: '0 8px 24px rgba(0, 0, 0, 0.12)',
    display: 'flex',
    flexDirection: 'column',
    zIndex: 50,
    outline: 'none',
  },
  header: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: '12px 16px',
    borderBottom: '1px solid #d0d7de',
  },
  headerTitle: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
  },
  matchCount: {
    fontSize: '14px',
    fontWeight: 700,
    color: '#f59e0b',
    backgroundColor: '#fef3c7',
    padding: '2px 8px',
    borderRadius: '10px',
  },
  headerText: {
    fontSize: '14px',
    fontWeight: 600,
    color: '#24292f',
  },
  closeBtn: {
    width: '28px',
    height: '28px',
    padding: 0,
    backgroundColor: 'transparent',
    border: 'none',
    borderRadius: '6px',
    color: '#57606a',
    fontSize: '18px',
    cursor: 'pointer',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
  },
  filterTabs: {
    display: 'flex',
    padding: '8px',
    gap: '4px',
    borderBottom: '1px solid #d0d7de',
  },
  filterTab: {
    flex: 1,
    padding: '6px 8px',
    backgroundColor: 'transparent',
    border: '1px solid transparent',
    borderRadius: '6px',
    color: '#57606a',
    fontSize: '11px',
    fontWeight: 500,
    cursor: 'pointer',
    transition: 'all 0.15s',
  },
  filterTabActive: {
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    color: '#24292f',
  },
  focusBtn: {
    margin: '8px 12px',
    padding: '8px 12px',
    backgroundColor: '#0969da',
    border: 'none',
    borderRadius: '6px',
    color: '#ffffff',
    fontSize: '12px',
    fontWeight: 500,
    cursor: 'pointer',
    transition: 'background-color 0.15s',
  },
  resultsList: {
    flex: 1,
    overflowY: 'auto',
    padding: '8px',
  },
  resultItem: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: '10px 12px',
    borderRadius: '8px',
    cursor: 'pointer',
    marginBottom: '4px',
    transition: 'background-color 0.1s',
    border: '1px solid transparent',
  },
  resultItemSelected: {
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
  },
  resultMain: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    flex: 1,
    minWidth: 0,
  },
  nodeTypeDot: {
    width: '8px',
    height: '8px',
    borderRadius: '50%',
    flexShrink: 0,
  },
  nodeId: {
    fontSize: '11px',
    color: '#6e7781',
    fontFamily: 'monospace',
    flexShrink: 0,
  },
  resultTitle: {
    fontSize: '13px',
    color: '#24292f',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  },
  resultMeta: {
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
    flexShrink: 0,
  },
  visibilityBadge: {
    fontSize: '9px',
    fontWeight: 600,
    padding: '2px 6px',
    borderRadius: '4px',
    textTransform: 'uppercase',
  },
  navigateBtn: {
    padding: '4px 8px',
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    borderRadius: '4px',
    color: '#0969da',
    fontSize: '10px',
    fontWeight: 600,
    cursor: 'pointer',
  },
  emptyState: {
    padding: '24px',
    textAlign: 'center',
    color: '#57606a',
    fontSize: '13px',
  },
  footer: {
    padding: '8px 12px',
    backgroundColor: '#f6f8fa',
    borderTop: '1px solid #d0d7de',
    borderRadius: '0 0 12px 12px',
  },
  footerHint: {
    fontSize: '11px',
    color: '#6e7781',
  },
};

export default SearchResultsPanel;
