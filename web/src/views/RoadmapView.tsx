/**
 * Roadmap View
 *
 * Displays roadmap items with sync status indicators.
 * Shows checkbox state, GitHub issue links, and outcome connections.
 * Items are grouped by section with section headers (matching TUI).
 *
 * Completion Logic (matches src/tui/views/roadmap.rs):
 * - isItemComplete: checkbox checked OR in "Completed" section
 * - isItemFullySynced: checkbox + outcome + issue closed (all three)
 *
 * Keyboard shortcuts:
 * - j/k or Arrow keys: Navigate items
 * - gg: Jump to top (vim-style)
 * - G: Jump to bottom (vim-style)
 * - o: Open GitHub issue in browser
 * - c: Toggle checkbox state
 * - Tab: Toggle between Active/Completed views
 * - Enter: Toggle detail panel
 * - Escape: Close detail panel
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

/**
 * Check if an item is complete for display purposes.
 * An item is complete if:
 * - Checkbox is checked, OR
 * - It's in a "Completed" section (case-insensitive)
 *
 * This matches the TUI logic in src/tui/views/roadmap.rs
 */
function isItemComplete(item: RoadmapItem): boolean {
  const checkboxChecked = item.checkbox_state === 'checked';
  const inCompletedSection = item.section?.toLowerCase().includes('completed') ?? false;
  return checkboxChecked || inCompletedSection;
}

/**
 * Check if an item is fully synced (strict completion check).
 * Requires all three: checkbox + outcome + issue closed.
 * Used for sync status display, not for filtering.
 */
function isItemFullySynced(item: RoadmapItem): boolean {
  const checkboxChecked = item.checkbox_state === 'checked';
  const hasOutcome = item.outcome_change_id !== null && item.outcome_change_id !== undefined;
  const issueClosed = item.github_issue_state === 'closed';
  return checkboxChecked && hasOutcome && issueClosed;
}

/**
 * Check if an item is a section header (not a real task).
 * Section headers have checkbox_state === 'none'.
 * These should be filtered out from the item list.
 */
function isSectionHeader(item: RoadmapItem): boolean {
  return item.checkbox_state === 'none';
}

/** Check if an item is partially complete (for display styling) */
function isItemPartial(item: RoadmapItem): boolean {
  const checkboxChecked = item.checkbox_state === 'checked';
  const hasOutcome = item.outcome_change_id !== null && item.outcome_change_id !== undefined;
  const issueClosed = item.github_issue_state === 'closed';
  return (checkboxChecked || hasOutcome || issueClosed) && !isItemFullySynced(item);
}

/**
 * Filter items by view mode, excluding section headers.
 * This matches the TUI logic in src/tui/views/roadmap.rs
 */
function filterByMode(items: RoadmapItem[], mode: ViewMode): RoadmapItem[] {
  // First filter out section headers (items with checkbox_state === 'none')
  const tasks = items.filter(item => !isSectionHeader(item));

  // Then filter by completion status
  return tasks.filter(item => {
    const complete = isItemComplete(item);
    return mode === 'active' ? !complete : complete;
  });
}

/** Count items by status (excluding section headers) */
function countByStatus(items: RoadmapItem[]): { active: number; completed: number } {
  const tasks = items.filter(item => !isSectionHeader(item));
  const completed = tasks.filter(isItemComplete).length;
  return { active: tasks.length - completed, completed };
}

/** Group items by section for grouped rendering */
function groupBySection(items: RoadmapItem[]): Map<string, RoadmapItem[]> {
  const groups = new Map<string, RoadmapItem[]>();

  for (const item of items) {
    const section = item.section || 'Uncategorized';
    const existing = groups.get(section) || [];
    groups.set(section, [...existing, item]);
  }

  return groups;
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

  // Local state for items (allows optimistic updates)
  const [items, setItems] = useState<RoadmapItem[]>(roadmapItems);

  // Sync with props when they change
  useEffect(() => {
    setItems(roadmapItems);
  }, [roadmapItems]);

  // Filter and count items
  const filteredItems = useMemo(() => filterByMode(items, viewMode), [items, viewMode]);
  const counts = useMemo(() => countByStatus(items), [items]);

  // Group filtered items by section for rendering
  const groupedItems = useMemo(() => groupBySection(filteredItems), [filteredItems]);

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

  // Track pending 'g' for gg command (vim-style jump to top)
  const [pendingG, setPendingG] = useState(false);

  // Keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't handle if typing in an input
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
        return;
      }

      // Handle pending 'g' for gg command
      if (pendingG) {
        setPendingG(false);
        if (e.key === 'g') {
          e.preventDefault();
          setSelectedIndex(0); // Jump to top
          return;
        }
        // Invalid g-sequence, fall through to normal handling
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
        case 'g':
          e.preventDefault();
          setPendingG(true);
          break;
        case 'G':
          e.preventDefault();
          setSelectedIndex(filteredItems.length - 1); // Jump to bottom
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
  }, [filteredItems.length, openSelectedIssue, toggleCheckbox, showDetail, pendingG]);

  const handleSelectOutcome = (outcomeId: number) => {
    const node = graphData.nodes.find(n => n.id === outcomeId);
    if (node) setSelectedNode(node);
  };

  // Show placeholder if no roadmap items
  if (roadmapItems.length === 0) {
    return (
      <div style={styles.empty}>
        <div style={styles.emptyIcon}>📋</div>
        <h2 style={styles.emptyTitle}>No Roadmap Items</h2>
        <p style={styles.emptyText}>
          Run <code style={styles.code}>deciduous roadmap init</code> to initialize the roadmap.
        </p>
        <p style={styles.emptyHint}>
          Then run <code style={styles.code}>deciduous roadmap sync</code> to sync with GitHub Issues.
        </p>
      </div>
    );
  }

  return (
    <div style={styles.container}>
      {/* Sidebar */}
      <div style={styles.sidebar}>
        <div style={styles.sidebarHeader}>
          <h2 style={styles.title}>Roadmap</h2>
          <span style={styles.itemCount}>{filteredItems.length} items</span>
        </div>

        {/* View Mode Toggle */}
        <div style={styles.toggleContainer}>
          <button
            onClick={() => { setViewMode('active'); setSelectedIndex(0); }}
            style={{
              ...styles.toggleBtn,
              ...(viewMode === 'active' ? styles.toggleBtnActive : {}),
            }}
          >
            <span style={getToggleDotStyle(viewMode === 'active', false)} />
            Active
            <span style={styles.toggleCount}>{counts.active}</span>
          </button>
          <button
            onClick={() => { setViewMode('completed'); setSelectedIndex(0); }}
            style={{
              ...styles.toggleBtn,
              ...(viewMode === 'completed' ? styles.toggleBtnActiveGreen : {}),
            }}
          >
            <span style={getToggleDotStyle(viewMode === 'completed', true)} />
            Done
            <span style={styles.toggleCount}>{counts.completed}</span>
          </button>
        </div>

        {/* Keyboard hints */}
        <div style={styles.hints}>
          <div style={styles.hintsTitle}>Keyboard</div>
          <div style={styles.hintGrid}>
            <kbd style={styles.kbd}>j/k</kbd><span style={styles.hintText}>Navigate</span>
            <kbd style={styles.kbd}>gg/G</kbd><span style={styles.hintText}>Top/Bottom</span>
            <kbd style={styles.kbd}>o</kbd><span style={styles.hintText}>Open issue</span>
            <kbd style={styles.kbd}>c</kbd><span style={styles.hintText}>Toggle done</span>
            <kbd style={styles.kbd}>Tab</kbd><span style={styles.hintText}>Switch view</span>
            <kbd style={styles.kbd}>Enter</kbd><span style={styles.hintText}>Details</span>
          </div>
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
            <div style={styles.emptyFilteredIcon}>
              {viewMode === 'active' ? '🎉' : '📝'}
            </div>
            <p style={styles.emptyFilteredText}>
              {viewMode === 'active'
                ? 'All caught up! No active items.'
                : 'No completed items yet.'}
            </p>
            <button
              onClick={() => { setViewMode(viewMode === 'active' ? 'completed' : 'active'); setSelectedIndex(0); }}
              style={styles.emptyFilteredBtn}
            >
              View {viewMode === 'active' ? 'completed' : 'active'} items
            </button>
          </div>
        ) : (
          <div style={styles.itemList}>
            {(() => {
              // Render items grouped by section with section headers
              // Track the flat index for selection
              let flatIndex = 0;
              const elements: React.ReactNode[] = [];

              for (const [section, items] of groupedItems) {
                // Add section header
                elements.push(
                  <SectionHeader key={`section-${section}`} title={section} count={items.length} />
                );

                // Add items in this section
                for (const item of items) {
                  const currentIndex = flatIndex;
                  elements.push(
                    <RoadmapItemCard
                      key={item.id}
                      item={item}
                      isSelected={currentIndex === selectedIndex}
                      onClick={() => setSelectedIndex(currentIndex)}
                      onToggleCheckbox={async () => {
                        const newState = item.checkbox_state === 'checked' ? 'unchecked' : 'checked';

                        // Optimistic update - update local state immediately
                        setItems(prev => prev.map(i =>
                          i.id === item.id ? { ...i, checkbox_state: newState } : i
                        ));
                        setStatusMessage(`Item marked as ${newState}`);

                        // Fire-and-forget API call (only works with deciduous serve)
                        fetch('/api/roadmap/checkbox', {
                          method: 'POST',
                          headers: { 'Content-Type': 'application/json' },
                          body: JSON.stringify({ item_id: item.id, checkbox_state: newState }),
                        }).catch(() => {
                          // API not available - that's fine, local state already updated
                        });
                      }}
                      onSelectOutcome={handleSelectOutcome}
                      onOpenIssue={() => {
                        const url = getIssueUrl(item);
                        if (url) window.open(url, '_blank', 'noopener,noreferrer');
                      }}
                    />
                  );
                  flatIndex++;
                }
              }

              return elements;
            })()}
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
// Section Header Component
// =============================================================================

interface SectionHeaderProps {
  title: string;
  count: number;
}

const SectionHeader: React.FC<SectionHeaderProps> = ({ title, count }) => {
  return (
    <div style={styles.sectionHeader}>
      <div style={styles.sectionLine} />
      <span style={styles.sectionTitle}>{title}</span>
      <span style={styles.sectionCount}>{count}</span>
      <div style={styles.sectionLine} />
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
  onToggleCheckbox: () => void;
  onSelectOutcome: (id: number) => void;
  onOpenIssue: () => void;
}

const RoadmapItemCard: React.FC<RoadmapItemCardProps> = ({
  item,
  isSelected,
  onClick,
  onToggleCheckbox,
  onSelectOutcome,
  onOpenIssue
}) => {
  const complete = isItemComplete(item);
  const partial = isItemPartial(item);
  const synced = isItemFullySynced(item);

  return (
    <div
      style={{
        ...styles.card,
        ...(isSelected ? styles.cardSelected : {}),
      }}
      onClick={onClick}
    >
      {/* Selection indicator */}
      <div style={{
        ...styles.selectionBar,
        opacity: isSelected ? 1 : 0,
      }} />

      <div style={styles.cardContent}>
        {/* Checkbox */}
        <button
          style={{
            ...styles.checkbox,
            ...(complete ? styles.checkboxChecked : {}),
          }}
          onClick={(e) => { e.stopPropagation(); onToggleCheckbox(); }}
        >
          {complete && (
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
              <path d="M2.5 6L5 8.5L9.5 3.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          )}
        </button>

        {/* Main content */}
        <div style={styles.cardMain}>
          <span style={{
            ...styles.cardTitle,
            ...(complete ? styles.cardTitleComplete : {}),
          }}>
            {item.title}
          </span>

          {/* Meta row */}
          <div style={styles.cardMeta}>
            {item.github_issue_number && (
              <button
                onClick={(e) => { e.stopPropagation(); onOpenIssue(); }}
                style={{
                  ...styles.badge,
                  ...(item.github_issue_state === 'closed' ? styles.badgePurple : styles.badgeBlue),
                }}
                title="Open in GitHub"
              >
                <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" style={{ marginRight: 4 }}>
                  <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/>
                </svg>
                #{item.github_issue_number}
              </button>
            )}
            {item.outcome_node_id && (
              <button
                onClick={(e) => { e.stopPropagation(); onSelectOutcome(item.outcome_node_id!); }}
                style={{ ...styles.badge, ...styles.badgeAmber }}
              >
                <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" style={{ marginRight: 4 }}>
                  <path d="M8.22 1.754a.25.25 0 00-.44 0L1.698 13.132a.25.25 0 00.22.368h12.164a.25.25 0 00.22-.368L8.22 1.754zm-1.763-.707c.659-1.234 2.427-1.234 3.086 0l6.082 11.378A1.75 1.75 0 0114.082 15H1.918a1.75 1.75 0 01-1.543-2.575L6.457 1.047z"/>
                </svg>
                Outcome
              </button>
            )}
            {synced && (
              <span style={{ ...styles.statusPill, ...styles.statusSynced }}>
                <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor" style={{ marginRight: 4 }}>
                  <path d="M8 16A8 8 0 108 0a8 8 0 000 16zm3.78-9.72a.75.75 0 00-1.06-1.06L6.75 9.19 5.28 7.72a.75.75 0 00-1.06 1.06l2 2a.75.75 0 001.06 0l4.5-4.5z"/>
                </svg>
                Synced
              </span>
            )}
            {!synced && partial && (
              <span style={{ ...styles.statusPill, ...styles.statusPartial }}>
                In Progress
              </span>
            )}
          </div>
        </div>
      </div>
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
  const synced = isItemFullySynced(item);

  return (
    <div style={styles.detailContent}>
      <div style={styles.detailHeader}>
        <h3 style={styles.detailTitle}>{item.title}</h3>
        <button onClick={onClose} style={styles.closeBtn}>
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M18 6L6 18M6 6l12 12"/>
          </svg>
        </button>
      </div>

      {item.section && (
        <div style={styles.detailChip}>{item.section}</div>
      )}

      {item.description && (
        <p style={styles.detailDesc}>{item.description}</p>
      )}

      <div style={styles.detailDivider}>Sync Status</div>

      <div style={styles.syncGrid}>
        <SyncStatusRow
          label="Checkbox"
          checked={item.checkbox_state === 'checked'}
          detail={item.checkbox_state}
        />
        <SyncStatusRow
          label="Outcome"
          checked={!!item.outcome_change_id}
          detail={item.outcome_change_id ? item.outcome_change_id.slice(0, 8) : 'Not linked'}
        />
        <SyncStatusRow
          label="Issue"
          checked={item.github_issue_state === 'closed'}
          detail={item.github_issue_number ? `#${item.github_issue_number} (${item.github_issue_state || 'unknown'})` : 'No issue'}
        />
      </div>

      {/* Overall status */}
      <div style={{
        ...styles.overallStatus,
        ...(synced ? styles.overallSynced : complete ? styles.overallComplete : styles.overallIncomplete),
      }}>
        {synced ? (
          <>
            <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
              <path d="M8 16A8 8 0 108 0a8 8 0 000 16zm3.78-9.72a.75.75 0 00-1.06-1.06L6.75 9.19 5.28 7.72a.75.75 0 00-1.06 1.06l2 2a.75.75 0 001.06 0l4.5-4.5z"/>
            </svg>
            Fully Synced
          </>
        ) : complete ? (
          <>
            <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
              <path d="M8 0a8 8 0 100 16A8 8 0 008 0zm3.78 6.28a.75.75 0 00-1.06-1.06L6.75 9.19 5.28 7.72a.75.75 0 00-1.06 1.06l2 2a.75.75 0 001.06 0l4.5-4.5z"/>
            </svg>
            Complete (not synced)
          </>
        ) : (
          <>
            <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
              <path d="M8 0a8 8 0 100 16A8 8 0 008 0zM1.5 8a6.5 6.5 0 1113 0 6.5 6.5 0 01-13 0z"/>
            </svg>
            Incomplete
          </>
        )}
      </div>
    </div>
  );
};

// Sync status row component
const SyncStatusRow: React.FC<{ label: string; checked: boolean; detail: string }> = ({ label, checked, detail }) => (
  <div style={styles.syncRow}>
    <div style={{
      ...styles.syncIcon,
      backgroundColor: checked ? '#dcfce7' : '#fef2f2',
      color: checked ? '#16a34a' : '#dc2626',
    }}>
      {checked ? (
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
          <path d="M2.5 6L5 8.5L9.5 3.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round"/>
        </svg>
      ) : (
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
          <path d="M3 3l6 6M9 3L3 9" stroke="currentColor" strokeWidth="2" strokeLinecap="round"/>
        </svg>
      )}
    </div>
    <div style={styles.syncInfo}>
      <span style={styles.syncLabel}>{label}</span>
      <span style={styles.syncDetail}>{detail}</span>
    </div>
  </div>
);

// =============================================================================
// Styles
// =============================================================================

/** Helper function for toggle dot styling */
const getToggleDotStyle = (active: boolean, green: boolean): React.CSSProperties => ({
  width: '8px',
  height: '8px',
  borderRadius: '50%',
  backgroundColor: active ? (green ? '#22c55e' : '#3b82f6') : '#d1d5db',
  transition: 'background-color 0.15s ease',
});

const styles: Record<string, React.CSSProperties> = {
  container: {
    height: '100%',
    display: 'flex',
    backgroundColor: '#fafafa',
  },
  sidebar: {
    width: '220px',
    padding: '24px 20px',
    backgroundColor: '#ffffff',
    borderRight: '1px solid #e5e7eb',
    display: 'flex',
    flexDirection: 'column',
    gap: '24px',
  },
  sidebarHeader: {
    display: 'flex',
    alignItems: 'baseline',
    justifyContent: 'space-between',
  },
  title: {
    fontSize: '18px',
    fontWeight: 600,
    margin: 0,
    color: '#111827',
    letterSpacing: '-0.025em',
  },
  itemCount: {
    fontSize: '12px',
    color: '#9ca3af',
    fontWeight: 500,
  },
  toggleContainer: {
    display: 'flex',
    flexDirection: 'column',
    gap: '8px',
  },
  toggleBtn: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    padding: '10px 14px',
    fontSize: '13px',
    fontWeight: 500,
    border: '1px solid #e5e7eb',
    backgroundColor: '#ffffff',
    color: '#6b7280',
    borderRadius: '8px',
    cursor: 'pointer',
    transition: 'all 0.15s ease',
  },
  toggleBtnActive: {
    backgroundColor: '#eff6ff',
    borderColor: '#3b82f6',
    color: '#1d4ed8',
  },
  toggleBtnActiveGreen: {
    backgroundColor: '#f0fdf4',
    borderColor: '#22c55e',
    color: '#16a34a',
  },
  toggleCount: {
    marginLeft: 'auto',
    fontSize: '12px',
    fontWeight: 600,
    color: 'inherit',
    opacity: 0.7,
  },
  hints: {
    marginTop: 'auto',
  },
  hintsTitle: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#9ca3af',
    textTransform: 'uppercase',
    letterSpacing: '0.05em',
    marginBottom: '12px',
  },
  hintGrid: {
    display: 'grid',
    gridTemplateColumns: 'auto 1fr',
    gap: '8px 12px',
    alignItems: 'center',
  },
  kbd: {
    display: 'inline-block',
    padding: '2px 6px',
    fontSize: '11px',
    fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace',
    fontWeight: 500,
    color: '#374151',
    backgroundColor: '#f3f4f6',
    borderRadius: '4px',
    border: '1px solid #e5e7eb',
    boxShadow: '0 1px 0 #d1d5db',
  },
  hintText: {
    fontSize: '12px',
    color: '#6b7280',
  },
  statusMessage: {
    padding: '10px 14px',
    backgroundColor: '#eff6ff',
    color: '#1d4ed8',
    borderRadius: '8px',
    fontSize: '13px',
    fontWeight: 500,
    textAlign: 'center',
  },
  content: {
    flex: 1,
    overflowY: 'auto',
    padding: '24px',
  },
  itemList: {
    display: 'flex',
    flexDirection: 'column',
    gap: '4px',
    maxWidth: '800px',
  },
  sectionHeader: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    padding: '20px 0 12px 0',
  },
  sectionLine: {
    flex: 1,
    height: '1px',
    backgroundColor: '#e5e7eb',
  },
  sectionTitle: {
    fontSize: '12px',
    fontWeight: 600,
    color: '#6b7280',
    textTransform: 'uppercase',
    letterSpacing: '0.05em',
  },
  sectionCount: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#9ca3af',
    backgroundColor: '#f3f4f6',
    padding: '2px 8px',
    borderRadius: '10px',
  },
  card: {
    position: 'relative',
    backgroundColor: '#ffffff',
    borderRadius: '10px',
    border: '1px solid #e5e7eb',
    cursor: 'pointer',
    transition: 'all 0.15s ease',
    overflow: 'hidden',
  },
  cardSelected: {
    borderColor: '#3b82f6',
    boxShadow: '0 0 0 3px rgba(59, 130, 246, 0.1), 0 4px 6px -1px rgba(0, 0, 0, 0.05)',
  },
  selectionBar: {
    position: 'absolute',
    left: 0,
    top: 0,
    bottom: 0,
    width: '3px',
    backgroundColor: '#3b82f6',
    transition: 'opacity 0.15s ease',
  },
  cardContent: {
    display: 'flex',
    alignItems: 'flex-start',
    gap: '12px',
    padding: '14px 16px',
  },
  checkbox: {
    width: '20px',
    height: '20px',
    borderRadius: '6px',
    border: '2px solid #d1d5db',
    backgroundColor: '#ffffff',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    cursor: 'pointer',
    transition: 'all 0.15s ease',
    flexShrink: 0,
    marginTop: '1px',
  },
  checkboxChecked: {
    backgroundColor: '#22c55e',
    borderColor: '#22c55e',
    color: '#ffffff',
  },
  cardMain: {
    flex: 1,
    minWidth: 0,
  },
  cardTitle: {
    fontSize: '14px',
    fontWeight: 500,
    color: '#111827',
    lineHeight: 1.4,
    display: 'block',
    marginBottom: '6px',
  },
  cardTitleComplete: {
    color: '#6b7280',
    textDecoration: 'line-through',
  },
  cardMeta: {
    display: 'flex',
    flexWrap: 'wrap',
    gap: '6px',
    alignItems: 'center',
  },
  badge: {
    display: 'inline-flex',
    alignItems: 'center',
    padding: '3px 8px',
    fontSize: '11px',
    fontWeight: 500,
    borderRadius: '6px',
    border: 'none',
    cursor: 'pointer',
    transition: 'all 0.15s ease',
  },
  badgeBlue: {
    backgroundColor: '#eff6ff',
    color: '#2563eb',
  },
  badgePurple: {
    backgroundColor: '#f5f3ff',
    color: '#7c3aed',
  },
  badgeAmber: {
    backgroundColor: '#fffbeb',
    color: '#d97706',
  },
  statusPill: {
    display: 'inline-flex',
    alignItems: 'center',
    padding: '3px 8px',
    fontSize: '11px',
    fontWeight: 500,
    borderRadius: '10px',
  },
  statusSynced: {
    backgroundColor: '#dcfce7',
    color: '#16a34a',
  },
  statusPartial: {
    backgroundColor: '#fef3c7',
    color: '#d97706',
  },
  detailSidebar: {
    width: '320px',
    borderLeft: '1px solid #e5e7eb',
    backgroundColor: '#ffffff',
    flexShrink: 0,
    overflowY: 'auto',
  },
  detailPanel: {
    width: '320px',
    borderLeft: '1px solid #e5e7eb',
    flexShrink: 0,
    backgroundColor: '#ffffff',
  },
  detailContent: {
    padding: '24px',
  },
  detailHeader: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'flex-start',
    gap: '12px',
    marginBottom: '16px',
  },
  detailTitle: {
    fontSize: '16px',
    fontWeight: 600,
    color: '#111827',
    margin: 0,
    lineHeight: 1.4,
  },
  closeBtn: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    width: '28px',
    height: '28px',
    background: 'none',
    border: 'none',
    borderRadius: '6px',
    cursor: 'pointer',
    color: '#9ca3af',
    transition: 'all 0.15s ease',
  },
  detailChip: {
    display: 'inline-block',
    padding: '4px 10px',
    fontSize: '12px',
    fontWeight: 500,
    color: '#6b7280',
    backgroundColor: '#f3f4f6',
    borderRadius: '6px',
    marginBottom: '16px',
  },
  detailDesc: {
    fontSize: '14px',
    color: '#4b5563',
    lineHeight: 1.6,
    margin: '0 0 16px 0',
  },
  detailDivider: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#9ca3af',
    textTransform: 'uppercase',
    letterSpacing: '0.05em',
    margin: '24px 0 16px 0',
    paddingTop: '16px',
    borderTop: '1px solid #e5e7eb',
  },
  syncGrid: {
    display: 'flex',
    flexDirection: 'column',
    gap: '12px',
  },
  syncRow: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
  },
  syncIcon: {
    width: '28px',
    height: '28px',
    borderRadius: '6px',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    flexShrink: 0,
  },
  syncInfo: {
    flex: 1,
    display: 'flex',
    flexDirection: 'column',
    gap: '2px',
  },
  syncLabel: {
    fontSize: '13px',
    fontWeight: 500,
    color: '#374151',
  },
  syncDetail: {
    fontSize: '12px',
    color: '#9ca3af',
  },
  overallStatus: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '8px',
    marginTop: '20px',
    padding: '12px 16px',
    borderRadius: '8px',
    fontSize: '13px',
    fontWeight: 600,
  },
  overallSynced: {
    backgroundColor: '#dcfce7',
    color: '#16a34a',
  },
  overallComplete: {
    backgroundColor: '#eff6ff',
    color: '#2563eb',
  },
  overallIncomplete: {
    backgroundColor: '#fef3c7',
    color: '#d97706',
  },
  empty: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    height: '100%',
    padding: '40px',
    textAlign: 'center',
  },
  emptyIcon: {
    fontSize: '48px',
    marginBottom: '16px',
  },
  emptyTitle: {
    fontSize: '20px',
    fontWeight: 600,
    color: '#111827',
    margin: '0 0 8px 0',
  },
  emptyText: {
    fontSize: '14px',
    color: '#6b7280',
    margin: '0 0 4px 0',
  },
  emptyHint: {
    fontSize: '14px',
    color: '#9ca3af',
    margin: 0,
  },
  code: {
    padding: '2px 6px',
    fontSize: '13px',
    fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace',
    backgroundColor: '#f3f4f6',
    borderRadius: '4px',
  },
  emptyFiltered: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    padding: '80px 40px',
    textAlign: 'center',
  },
  emptyFilteredIcon: {
    fontSize: '48px',
    marginBottom: '16px',
  },
  emptyFilteredText: {
    fontSize: '15px',
    color: '#6b7280',
    margin: '0 0 16px 0',
  },
  emptyFilteredBtn: {
    padding: '10px 20px',
    fontSize: '13px',
    fontWeight: 500,
    color: '#3b82f6',
    backgroundColor: '#eff6ff',
    border: 'none',
    borderRadius: '8px',
    cursor: 'pointer',
  },
};
