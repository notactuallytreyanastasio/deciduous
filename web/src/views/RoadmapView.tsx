/**
 * Roadmap View
 *
 * Displays roadmap items with sync status indicators.
 * Shows checkbox state, GitHub issue links, and outcome connections.
 *
 * Keyboard shortcuts:
 * - j/k or Arrow keys: Navigate items
 * - o: Open GitHub issue in browser
 * - Tab: Toggle between Active/Completed views
 * - Enter: Toggle detail panel
 */

import React, { useState, useMemo, useEffect, useCallback } from 'react';
import type { GraphData, DecisionNode } from '../types/graph';
import type { RoadmapItem } from '../types/generated/schema';
import { DetailPanel } from '../components/DetailPanel';

// =============================================================================
// Types
// =============================================================================

interface RoadmapViewProps {
  graphData: GraphData;
  roadmapItems?: RoadmapItem[];
}

type ViewMode = 'active' | 'completed';

// =============================================================================
// Pure Functions (Functional Core)
// =============================================================================

/** Check if an item is complete (checkbox + outcome + issue closed) */
function isItemComplete(item: RoadmapItem): boolean {
  const checkboxChecked = item.checkbox_state === 'checked';
  const hasOutcome = item.outcome_change_id !== null && item.outcome_change_id !== undefined;
  const issueClosed = item.github_issue_state === 'closed';
  return checkboxChecked && hasOutcome && issueClosed;
}

/** Check if an item is partially complete */
function isItemPartial(item: RoadmapItem): boolean {
  const checkboxChecked = item.checkbox_state === 'checked';
  const hasOutcome = item.outcome_change_id !== null && item.outcome_change_id !== undefined;
  const issueClosed = item.github_issue_state === 'closed';
  return (checkboxChecked || hasOutcome || issueClosed) && !isItemComplete(item);
}

/** Filter items by view mode */
function filterByMode(items: RoadmapItem[], mode: ViewMode): RoadmapItem[] {
  return items.filter(item => {
    const complete = isItemComplete(item);
    return mode === 'active' ? !complete : complete;
  });
}

/** Count items by status */
function countByStatus(items: RoadmapItem[]): { active: number; completed: number } {
  const completed = items.filter(isItemComplete).length;
  return { active: items.length - completed, completed };
}

/** Get GitHub issue URL */
function getIssueUrl(item: RoadmapItem): string | null {
  if (!item.github_issue_number) return null;
  return `https://github.com/notactuallytreyanastasio/deciduous/issues/${item.github_issue_number}`;
}

// =============================================================================
// Component
// =============================================================================

export const RoadmapView: React.FC<RoadmapViewProps> = ({
  graphData,
  roadmapItems = [],
}) => {
  const [viewMode, setViewMode] = useState<ViewMode>('active');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [showDetail, setShowDetail] = useState(false);
  const [selectedNode, setSelectedNode] = useState<DecisionNode | null>(null);

  // Filter and count items
  const filteredItems = useMemo(() => filterByMode(roadmapItems, viewMode), [roadmapItems, viewMode]);
  const counts = useMemo(() => countByStatus(roadmapItems), [roadmapItems]);

  // Get selected item
  const selectedItem = filteredItems[selectedIndex] ?? null;

  // Clamp selection when items change
  useEffect(() => {
    if (selectedIndex >= filteredItems.length) {
      setSelectedIndex(Math.max(0, filteredItems.length - 1));
    }
  }, [filteredItems.length, selectedIndex]);

  // Open issue in browser
  const openSelectedIssue = useCallback(() => {
    if (selectedItem) {
      const url = getIssueUrl(selectedItem);
      if (url) {
        window.open(url, '_blank', 'noopener,noreferrer');
      }
    }
  }, [selectedItem]);

  // State for status message
  const [statusMessage, setStatusMessage] = useState<string | null>(null);

  // Clear status message after 3 seconds
  useEffect(() => {
    if (statusMessage) {
      const timer = setTimeout(() => setStatusMessage(null), 3000);
      return () => clearTimeout(timer);
    }
  }, [statusMessage]);

  // Toggle checkbox via API (only works when running locally with deciduous serve)
  const toggleCheckbox = useCallback(async () => {
    if (!selectedItem) {
      setStatusMessage('No item selected');
      return;
    }

    const newState = selectedItem.checkbox_state === 'checked' ? 'unchecked' : 'checked';

    try {
      const response = await fetch('/api/roadmap/checkbox', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          item_id: selectedItem.id,
          checkbox_state: newState,
        }),
      });

      if (response.ok) {
        setStatusMessage(`Item marked as ${newState}`);
        // Reload the page to refresh data (simple approach)
        window.location.reload();
      } else {
        const data = await response.json();
        setStatusMessage(data.error || 'Failed to update');
      }
    } catch {
      setStatusMessage('API not available (requires deciduous serve)');
    }
  }, [selectedItem]);

  // Keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't handle if typing in an input
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
        return;
      }

      switch (e.key) {
        case 'j':
        case 'ArrowDown':
          e.preventDefault();
          setSelectedIndex(i => Math.min(i + 1, filteredItems.length - 1));
          break;
        case 'k':
        case 'ArrowUp':
          e.preventDefault();
          setSelectedIndex(i => Math.max(i - 1, 0));
          break;
        case 'o':
          e.preventDefault();
          openSelectedIssue();
          break;
        case 'c':
          e.preventDefault();
          toggleCheckbox();
          break;
        case 'Tab':
          e.preventDefault();
          setViewMode(m => m === 'active' ? 'completed' : 'active');
          setSelectedIndex(0);
          break;
        case 'Enter':
          e.preventDefault();
          setShowDetail(d => !d);
          break;
        case 'Escape':
          if (showDetail) {
            e.preventDefault();
            setShowDetail(false);
          }
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [filteredItems.length, openSelectedIssue, toggleCheckbox, showDetail]);

  const handleSelectOutcome = (outcomeId: number) => {
    const node = graphData.nodes.find(n => n.id === outcomeId);
    if (node) setSelectedNode(node);
  };

  // Show placeholder if no roadmap items
  if (roadmapItems.length === 0) {
    return (
      <div style={styles.empty}>
        <h2>No Roadmap Items</h2>
        <p>Run <code>deciduous roadmap init</code> to initialize the roadmap.</p>
        <p style={styles.hint}>
          Then run <code>deciduous roadmap sync</code> to sync with GitHub Issues.
        </p>
      </div>
    );
  }

  return (
    <div style={styles.container}>
      {/* Sidebar */}
      <div style={styles.sidebar}>
        <h2 style={styles.title}>Roadmap</h2>

        {/* View Mode Toggle */}
        <div style={styles.filterSection}>
          <label style={styles.filterLabel}>View Mode</label>
          <div style={styles.filterButtons}>
            <button
              onClick={() => { setViewMode('active'); setSelectedIndex(0); }}
              style={{
                ...styles.filterBtn,
                ...(viewMode === 'active' ? styles.filterBtnActive : {}),
              }}
            >
              Active ({counts.active})
            </button>
            <button
              onClick={() => { setViewMode('completed'); setSelectedIndex(0); }}
              style={{
                ...styles.filterBtn,
                ...(viewMode === 'completed' ? styles.filterBtnActive : {}),
              }}
            >
              Completed ({counts.completed})
            </button>
          </div>
        </div>

        {/* Keyboard hints */}
        <div style={styles.hints}>
          <div style={styles.hintItem}><kbd>j/k</kbd> Navigate</div>
          <div style={styles.hintItem}><kbd>o</kbd> Open issue</div>
          <div style={styles.hintItem}><kbd>c</kbd> Toggle checkbox</div>
          <div style={styles.hintItem}><kbd>Tab</kbd> Toggle view</div>
          <div style={styles.hintItem}><kbd>Enter</kbd> Detail panel</div>
        </div>

        {/* Status message */}
        {statusMessage && (
          <div style={styles.statusMessage}>{statusMessage}</div>
        )}
      </div>

      {/* Main content */}
      <div style={styles.content}>
        {filteredItems.length === 0 ? (
          <div style={styles.emptyFiltered}>
            {viewMode === 'active'
              ? 'No active items. Press Tab to view completed.'
              : 'No completed items. Press Tab to view active.'}
          </div>
        ) : (
          <div style={styles.itemList}>
            {filteredItems.map((item, index) => (
              <RoadmapItemCard
                key={item.id}
                item={item}
                isSelected={index === selectedIndex}
                onClick={() => setSelectedIndex(index)}
                onSelectOutcome={handleSelectOutcome}
                onOpenIssue={() => {
                  const url = getIssueUrl(item);
                  if (url) window.open(url, '_blank', 'noopener,noreferrer');
                }}
              />
            ))}
          </div>
        )}
      </div>

      {/* Detail panel */}
      {showDetail && selectedItem && (
        <div style={styles.detailSidebar}>
          <ItemDetailPanel item={selectedItem} onClose={() => setShowDetail(false)} />
        </div>
      )}

      {/* Node detail panel for linked outcomes */}
      {selectedNode && (
        <div style={styles.detailPanel}>
          <DetailPanel
            node={selectedNode}
            graphData={graphData}
            onSelectNode={(id) => {
              const node = graphData.nodes.find(n => n.id === id);
              if (node) setSelectedNode(node);
            }}
            onClose={() => setSelectedNode(null)}
          />
        </div>
      )}
    </div>
  );
};

// =============================================================================
// Roadmap Item Card
// =============================================================================

interface RoadmapItemCardProps {
  item: RoadmapItem;
  isSelected: boolean;
  onClick: () => void;
  onSelectOutcome: (id: number) => void;
  onOpenIssue: () => void;
}

const RoadmapItemCard: React.FC<RoadmapItemCardProps> = ({
  item,
  isSelected,
  onClick,
  onSelectOutcome,
  onOpenIssue
}) => {
  const complete = isItemComplete(item);
  const partial = isItemPartial(item);

  return (
    <div
      style={{
        ...styles.card,
        ...(complete ? styles.cardComplete : {}),
        ...(isSelected ? styles.cardSelected : {}),
      }}
      onClick={onClick}
    >
      <div style={styles.cardHeader}>
        {/* Checkbox */}
        <span style={{
          ...styles.checkbox,
          color: item.checkbox_state === 'checked' ? '#1a7f37' : '#6e7781',
        }}>
          {item.checkbox_state === 'checked' ? '☑' : '☐'}
        </span>

        {/* Outcome indicator */}
        <span style={{
          ...styles.outcomeIcon,
          color: item.outcome_change_id ? '#9a6700' : '#d0d7de',
        }}>
          ⚡
        </span>

        {/* Title */}
        <span style={{
          ...styles.cardTitle,
          color: complete ? '#1a7f37' : partial ? '#9a6700' : '#24292f',
        }}>
          {item.title}
        </span>

        {/* Badges */}
        <div style={styles.badges}>
          {item.github_issue_number && (
            <button
              onClick={(e) => { e.stopPropagation(); onOpenIssue(); }}
              style={{
                ...styles.issueBadge,
                ...(item.github_issue_state === 'closed' ? styles.issueClosed : styles.issueOpen),
              }}
              title="Click or press 'o' to open issue"
            >
              #{item.github_issue_number}
            </button>
          )}
          {item.outcome_node_id && (
            <button
              onClick={(e) => { e.stopPropagation(); onSelectOutcome(item.outcome_node_id!); }}
              style={styles.outcomeBadge}
            >
              ⚡ Outcome
            </button>
          )}
        </div>
      </div>

      {item.section && (
        <div style={styles.cardSection}>{item.section}</div>
      )}

      {item.description && (
        <p style={styles.cardDesc}>{item.description}</p>
      )}

      {complete && (
        <div style={styles.completeBadge}>
          ✓ Complete
        </div>
      )}
    </div>
  );
};

// =============================================================================
// Item Detail Panel
// =============================================================================

interface ItemDetailPanelProps {
  item: RoadmapItem;
  onClose: () => void;
}

const ItemDetailPanel: React.FC<ItemDetailPanelProps> = ({ item, onClose }) => {
  const complete = isItemComplete(item);

  return (
    <div style={styles.detailContent}>
      <div style={styles.detailHeader}>
        <h3 style={styles.detailTitle}>{item.title}</h3>
        <button onClick={onClose} style={styles.closeBtn}>×</button>
      </div>

      {item.section && (
        <div style={styles.detailRow}>
          <span style={styles.detailLabel}>Section:</span>
          <span>{item.section}</span>
        </div>
      )}

      {item.description && (
        <div style={styles.detailRow}>
          <span style={styles.detailLabel}>Description:</span>
          <p style={styles.detailDesc}>{item.description}</p>
        </div>
      )}

      <div style={styles.detailDivider}>Completion Status</div>

      <div style={styles.statusRow}>
        <span style={{ color: item.checkbox_state === 'checked' ? '#1a7f37' : '#cf222e' }}>
          {item.checkbox_state === 'checked' ? '☑' : '☐'} Checkbox: {item.checkbox_state}
        </span>
      </div>

      <div style={styles.statusRow}>
        <span style={{ color: item.outcome_change_id ? '#1a7f37' : '#cf222e' }}>
          ⚡ Outcome: {item.outcome_change_id ? `Linked (${item.outcome_change_id.slice(0, 8)})` : 'Not linked'}
        </span>
      </div>

      <div style={styles.statusRow}>
        <span style={{ color: item.github_issue_state === 'closed' ? '#1a7f37' : '#cf222e' }}>
          {item.github_issue_state === 'closed' ? '🔒' : '🔓'} Issue: {
            item.github_issue_number
              ? `#${item.github_issue_number} (${item.github_issue_state || 'unknown'})`
              : 'No issue'
          }
        </span>
      </div>

      <div style={{
        ...styles.overallStatus,
        backgroundColor: complete ? '#dafbe1' : '#fff8c5',
        color: complete ? '#1a7f37' : '#9a6700',
      }}>
        {complete ? '✓ COMPLETE' : '○ INCOMPLETE'}
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
    gap: 0,
  },
  sidebar: {
    width: '200px',
    padding: '20px',
    backgroundColor: '#f6f8fa',
    borderRight: '1px solid #d0d7de',
    flexShrink: 0,
  },
  title: {
    fontSize: '16px',
    margin: '0 0 20px 0',
    color: '#24292f',
  },
  filterSection: {
    marginBottom: '20px',
  },
  filterLabel: {
    display: 'block',
    fontSize: '11px',
    color: '#6e7781',
    marginBottom: '8px',
    textTransform: 'uppercase',
  },
  filterButtons: {
    display: 'flex',
    flexDirection: 'column',
    gap: '4px',
  },
  filterBtn: {
    padding: '8px 12px',
    fontSize: '12px',
    border: '1px solid #d0d7de',
    backgroundColor: '#ffffff',
    color: '#57606a',
    borderRadius: '4px',
    cursor: 'pointer',
    textAlign: 'left',
  },
  filterBtnActive: {
    backgroundColor: '#0969da',
    color: '#ffffff',
    borderColor: '#0969da',
  },
  hints: {
    marginTop: '20px',
    paddingTop: '20px',
    borderTop: '1px solid #d0d7de',
  },
  hintItem: {
    fontSize: '11px',
    color: '#6e7781',
    marginBottom: '6px',
  },
  content: {
    flex: 1,
    overflowY: 'auto',
    padding: '20px',
    backgroundColor: '#ffffff',
  },
  itemList: {
    display: 'flex',
    flexDirection: 'column',
    gap: '8px',
  },
  card: {
    padding: '12px 16px',
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    cursor: 'pointer',
    transition: 'border-color 0.2s, background-color 0.2s',
  },
  cardSelected: {
    borderColor: '#0969da',
    backgroundColor: '#f0f7ff',
  },
  cardComplete: {
    backgroundColor: '#f0fdf4',
    borderColor: '#86efac',
  },
  cardHeader: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
  },
  checkbox: {
    fontSize: '16px',
  },
  outcomeIcon: {
    fontSize: '14px',
  },
  cardTitle: {
    flex: 1,
    fontSize: '14px',
    fontWeight: 500,
  },
  cardSection: {
    fontSize: '11px',
    color: '#6e7781',
    marginTop: '4px',
    marginLeft: '40px',
  },
  badges: {
    display: 'flex',
    gap: '6px',
  },
  issueBadge: {
    fontSize: '11px',
    padding: '2px 8px',
    borderRadius: '10px',
    border: 'none',
    cursor: 'pointer',
    fontWeight: 500,
  },
  issueOpen: {
    backgroundColor: '#ddf4ff',
    color: '#0969da',
  },
  issueClosed: {
    backgroundColor: '#8250df20',
    color: '#8250df',
  },
  outcomeBadge: {
    fontSize: '11px',
    padding: '2px 8px',
    borderRadius: '10px',
    backgroundColor: '#fff8c5',
    color: '#9a6700',
    border: 'none',
    cursor: 'pointer',
  },
  cardDesc: {
    fontSize: '12px',
    color: '#57606a',
    margin: '8px 0 0 40px',
    lineHeight: 1.4,
  },
  completeBadge: {
    marginTop: '8px',
    marginLeft: '40px',
    fontSize: '11px',
    color: '#1a7f37',
    fontWeight: 500,
  },
  detailSidebar: {
    width: '300px',
    borderLeft: '1px solid #d0d7de',
    backgroundColor: '#ffffff',
    flexShrink: 0,
  },
  detailPanel: {
    width: '300px',
    borderLeft: '1px solid #d0d7de',
    flexShrink: 0,
  },
  detailContent: {
    padding: '16px',
  },
  detailHeader: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'flex-start',
    marginBottom: '16px',
  },
  detailTitle: {
    fontSize: '14px',
    fontWeight: 600,
    color: '#24292f',
    margin: 0,
  },
  closeBtn: {
    background: 'none',
    border: 'none',
    fontSize: '20px',
    cursor: 'pointer',
    color: '#6e7781',
    padding: '0 4px',
  },
  detailRow: {
    marginBottom: '12px',
  },
  detailLabel: {
    fontSize: '11px',
    color: '#6e7781',
    textTransform: 'uppercase',
    display: 'block',
    marginBottom: '4px',
  },
  detailDesc: {
    fontSize: '13px',
    color: '#24292f',
    margin: 0,
    lineHeight: 1.5,
  },
  detailDivider: {
    fontSize: '11px',
    color: '#6e7781',
    textTransform: 'uppercase',
    margin: '16px 0 12px 0',
    paddingTop: '12px',
    borderTop: '1px solid #d0d7de',
  },
  statusRow: {
    fontSize: '13px',
    marginBottom: '8px',
  },
  overallStatus: {
    marginTop: '16px',
    padding: '8px 12px',
    borderRadius: '4px',
    fontSize: '12px',
    fontWeight: 600,
    textAlign: 'center',
  },
  empty: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    height: '100%',
    color: '#57606a',
    textAlign: 'center',
  },
  hint: {
    fontSize: '14px',
    color: '#6e7781',
  },
  emptyFiltered: {
    textAlign: 'center',
    color: '#6e7781',
    padding: '40px',
  },
  statusMessage: {
    marginTop: '12px',
    padding: '8px 12px',
    backgroundColor: '#ddf4ff',
    color: '#0969da',
    borderRadius: '4px',
    fontSize: '12px',
    textAlign: 'center',
  },
};
