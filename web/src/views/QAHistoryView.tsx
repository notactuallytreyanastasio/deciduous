/**
 * Q&A History View
 *
 * Displays past Q&A interactions with full-text search.
 * Uses FTS5 for relevance-ranked search with highlighted snippets.
 *
 * Keyboard shortcuts:
 * - j/k or Arrow keys: Navigate items
 * - / : Focus search input
 * - Enter: View full Q&A detail
 * - d: Delete selected Q&A (soft delete)
 * - Escape: Clear search / close detail
 */

import React, { useState, useEffect, useCallback, useRef } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { useQAData } from '../hooks/useQAData';
import { formatTimestamp, truncateText, parseContext } from '../types/qa';
import type { QaInteraction } from '../types/qa';

// =============================================================================
// Styles
// =============================================================================

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: 'flex',
    height: '100%',
    backgroundColor: '#ffffff',
  },
  // Left panel - list
  listPanel: {
    width: '400px',
    borderRight: '1px solid #d0d7de',
    display: 'flex',
    flexDirection: 'column',
    overflow: 'hidden',
  },
  searchContainer: {
    padding: '16px',
    borderBottom: '1px solid #d0d7de',
    backgroundColor: '#f6f8fa',
  },
  searchInput: {
    width: '100%',
    padding: '8px 12px',
    fontSize: '14px',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    outline: 'none',
    boxSizing: 'border-box' as const,
  },
  searchHint: {
    fontSize: '12px',
    color: '#656d76',
    marginTop: '8px',
  },
  listContainer: {
    flex: 1,
    overflowY: 'auto' as const,
  },
  listItem: {
    padding: '12px 16px',
    borderBottom: '1px solid #d0d7de',
    cursor: 'pointer',
    transition: 'background-color 0.1s',
  },
  listItemSelected: {
    backgroundColor: '#ddf4ff',
  },
  listItemHover: {
    backgroundColor: '#f6f8fa',
  },
  listItemPrompt: {
    fontSize: '14px',
    fontWeight: 500,
    color: '#1f2328',
    marginBottom: '4px',
    lineHeight: '1.4',
  },
  listItemMeta: {
    fontSize: '12px',
    color: '#656d76',
    display: 'flex',
    justifyContent: 'space-between',
  },
  highlight: {
    backgroundColor: '#fff8c5',
    padding: '0 2px',
    borderRadius: '2px',
  },
  // Right panel - detail
  detailPanel: {
    flex: 1,
    display: 'flex',
    flexDirection: 'column',
    overflow: 'hidden',
  },
  detailHeader: {
    padding: '16px',
    borderBottom: '1px solid #d0d7de',
    backgroundColor: '#f6f8fa',
  },
  detailTitle: {
    fontSize: '16px',
    fontWeight: 600,
    color: '#1f2328',
    marginBottom: '8px',
  },
  detailMeta: {
    fontSize: '12px',
    color: '#656d76',
  },
  detailContent: {
    flex: 1,
    overflowY: 'auto' as const,
    padding: '16px',
  },
  section: {
    marginBottom: '24px',
  },
  sectionLabel: {
    fontSize: '12px',
    fontWeight: 600,
    color: '#656d76',
    textTransform: 'uppercase' as const,
    marginBottom: '8px',
  },
  promptText: {
    fontSize: '14px',
    color: '#1f2328',
    backgroundColor: '#f6f8fa',
    padding: '12px',
    borderRadius: '6px',
    whiteSpace: 'pre-wrap' as const,
  },
  responseContent: {
    fontSize: '14px',
    color: '#1f2328',
    lineHeight: '1.6',
  },
  contextBadge: {
    display: 'inline-block',
    padding: '2px 8px',
    fontSize: '12px',
    backgroundColor: '#ddf4ff',
    color: '#0969da',
    borderRadius: '12px',
    marginRight: '8px',
    marginBottom: '4px',
  },
  // Empty state
  emptyState: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    height: '100%',
    color: '#656d76',
    padding: '40px',
    textAlign: 'center' as const,
  },
  emptyIcon: {
    fontSize: '48px',
    marginBottom: '16px',
  },
  emptyText: {
    fontSize: '16px',
    marginBottom: '8px',
  },
  emptyHint: {
    fontSize: '14px',
  },
  // Loading
  loadingSpinner: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    padding: '40px',
    color: '#656d76',
  },
  // Stats bar
  statsBar: {
    padding: '8px 16px',
    backgroundColor: '#f6f8fa',
    borderBottom: '1px solid #d0d7de',
    fontSize: '12px',
    color: '#656d76',
  },
};

// =============================================================================
// Component
// =============================================================================

export const QAHistoryView: React.FC = () => {
  // URL-based selection
  const { id: urlId } = useParams<{ id?: string }>();
  const navigate = useNavigate();

  const {
    items,
    searchResults,
    total,
    loading,
    error,
    isSearchMode,
    searchQuery,
    setSearchQuery,
    deleteItem,
  } = useQAData({ limit: 50 });

  const [selectedIndex, setSelectedIndexState] = useState(0);
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Sync selection with URL param
  useEffect(() => {
    if (urlId && items.length > 0) {
      const targetId = parseInt(urlId, 10);
      const idx = items.findIndex(item => item.id === targetId);
      if (idx >= 0) {
        setSelectedIndexState(idx);
      }
    }
  }, [urlId, items]);

  // Update URL when selection changes
  const setSelectedIndex = useCallback((indexOrFn: number | ((prev: number) => number)) => {
    setSelectedIndexState(prev => {
      const newIndex = typeof indexOrFn === 'function' ? indexOrFn(prev) : indexOrFn;
      const item = items[newIndex];
      if (item) {
        navigate(`/qa-history/${item.id}`, { replace: true });
      }
      return newIndex;
    });
  }, [items, navigate]);

  const selectedItem = items[selectedIndex] ?? null;

  // Clamp selection when items change
  useEffect(() => {
    if (selectedIndex >= items.length && items.length > 0) {
      setSelectedIndex(items.length - 1);
    }
  }, [items.length, selectedIndex]);

  // Keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't intercept if typing in search
      if (document.activeElement === searchInputRef.current) {
        if (e.key === 'Escape') {
          setSearchQuery('');
          searchInputRef.current?.blur();
        }
        return;
      }

      switch (e.key) {
        case 'j':
        case 'ArrowDown':
          e.preventDefault();
          setSelectedIndex(prev => Math.min(prev + 1, items.length - 1));
          break;
        case 'k':
        case 'ArrowUp':
          e.preventDefault();
          setSelectedIndex(prev => Math.max(prev - 1, 0));
          break;
        case '/':
          e.preventDefault();
          searchInputRef.current?.focus();
          break;
        case 'd':
          if (selectedItem) {
            deleteItem(selectedItem.id);
          }
          break;
        case 'Escape':
          if (isSearchMode) {
            setSearchQuery('');
          }
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [items.length, selectedItem, deleteItem, setSearchQuery, isSearchMode]);

  // Scroll selected item into view
  useEffect(() => {
    if (listRef.current) {
      const selectedEl = listRef.current.querySelector(`[data-index="${selectedIndex}"]`);
      selectedEl?.scrollIntoView({ block: 'nearest' });
    }
  }, [selectedIndex]);

  const handleSearchChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    setSearchQuery(e.target.value);
    setSelectedIndex(0);
  }, [setSearchQuery]);

  const renderHighlightedText = (text: string) => {
    // Replace <mark> tags from FTS5 with styled spans
    const parts = text.split(/(<mark>.*?<\/mark>)/g);
    return parts.map((part, i) => {
      if (part.startsWith('<mark>')) {
        const content = part.replace(/<\/?mark>/g, '');
        return <span key={i} style={styles.highlight}>{content}</span>;
      }
      return part;
    });
  };

  const renderListItem = (item: QaInteraction, index: number) => {
    const isSelected = index === selectedIndex;
    const isHovered = index === hoveredIndex;
    const searchResult = searchResults?.[index];

    return (
      <div
        key={item.id}
        data-index={index}
        style={{
          ...styles.listItem,
          ...(isSelected ? styles.listItemSelected : {}),
          ...(isHovered && !isSelected ? styles.listItemHover : {}),
        }}
        onClick={() => setSelectedIndex(index)}
        onMouseEnter={() => setHoveredIndex(index)}
        onMouseLeave={() => setHoveredIndex(null)}
      >
        <div style={styles.listItemPrompt}>
          {searchResult ? (
            renderHighlightedText(truncateText(searchResult.snippet_prompt, 100))
          ) : (
            truncateText(item.user_prompt, 100)
          )}
        </div>
        <div style={styles.listItemMeta}>
          <span>{formatTimestamp(item.inserted_at)}</span>
          {searchResult && (
            <span title="Relevance score">
              {Math.abs(searchResult.rank).toFixed(2)}
            </span>
          )}
        </div>
      </div>
    );
  };

  const renderDetail = (item: QaInteraction) => {
    const context = parseContext(item.context_json);

    return (
      <div style={styles.detailPanel}>
        <div style={styles.detailHeader}>
          <div style={styles.detailTitle}>Q&A #{item.id}</div>
          <div style={styles.detailMeta}>
            {formatTimestamp(item.inserted_at)}
            {context?.current_branch && (
              <span> on branch {context.current_branch}</span>
            )}
          </div>
        </div>
        <div style={styles.detailContent}>
          {/* Context badges */}
          {context && (
            <div style={styles.section}>
              <div style={styles.sectionLabel}>Context</div>
              {context.narrative && (
                <span style={styles.contextBadge}>
                  Narrative: {context.narrative.name}
                </span>
              )}
              {context.selected_node_id && (
                <span style={styles.contextBadge}>
                  Node #{context.selected_node_id}
                </span>
              )}
              {context.visible_node_ids && (
                <span style={styles.contextBadge}>
                  {context.visible_node_ids.length} nodes visible
                </span>
              )}
            </div>
          )}

          {/* User prompt */}
          <div style={styles.section}>
            <div style={styles.sectionLabel}>Question</div>
            <div style={styles.promptText}>{item.user_prompt}</div>
          </div>

          {/* Response */}
          <div style={styles.section}>
            <div style={styles.sectionLabel}>Response</div>
            <div style={styles.responseContent}>
              <ReactMarkdown remarkPlugins={[remarkGfm]}>
                {item.response}
              </ReactMarkdown>
            </div>
          </div>
        </div>
      </div>
    );
  };

  const renderEmptyState = () => (
    <div style={styles.emptyState}>
      <div style={styles.emptyIcon}>💬</div>
      <div style={styles.emptyText}>
        {isSearchMode ? 'No matching Q&As found' : 'No Q&A history yet'}
      </div>
      <div style={styles.emptyHint}>
        {isSearchMode
          ? 'Try different search terms'
          : 'Ask questions in the Archaeology view to build your Q&A history'}
      </div>
    </div>
  );

  return (
    <div style={styles.container}>
      {/* Left panel - List */}
      <div style={styles.listPanel}>
        {/* Search */}
        <div style={styles.searchContainer}>
          <input
            ref={searchInputRef}
            type="text"
            placeholder="Search Q&A history..."
            value={searchQuery}
            onChange={handleSearchChange}
            style={styles.searchInput}
          />
          <div style={styles.searchHint}>
            Press / to focus, Escape to clear
          </div>
        </div>

        {/* Stats */}
        <div style={styles.statsBar}>
          {isSearchMode ? (
            <span>{items.length} results for "{searchQuery}"</span>
          ) : (
            <span>{total} Q&A interactions</span>
          )}
        </div>

        {/* List */}
        <div ref={listRef} style={styles.listContainer}>
          {loading ? (
            <div style={styles.loadingSpinner}>Loading...</div>
          ) : error ? (
            <div style={{ ...styles.emptyState, color: '#cf222e' }}>
              {error}
            </div>
          ) : items.length === 0 ? (
            renderEmptyState()
          ) : (
            items.map((item, index) => renderListItem(item, index))
          )}
        </div>
      </div>

      {/* Right panel - Detail */}
      {selectedItem ? (
        renderDetail(selectedItem)
      ) : (
        <div style={styles.detailPanel}>
          <div style={styles.emptyState}>
            <div style={styles.emptyIcon}>📖</div>
            <div style={styles.emptyText}>Select a Q&A to view details</div>
            <div style={styles.emptyHint}>
              Use j/k or arrow keys to navigate
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
