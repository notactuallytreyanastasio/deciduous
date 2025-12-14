/**
 * Roadmap View
 *
 * Displays roadmap items with sync status indicators.
 * Shows checkbox state, GitHub issue links, and outcome connections.
 */

import React, { useState, useMemo } from 'react';
import type { GraphData, DecisionNode } from '../types/graph';
import type { RoadmapItem } from '../types/generated/schema';
import { DetailPanel } from '../components/DetailPanel';

interface RoadmapViewProps {
  graphData: GraphData;
  roadmapItems?: RoadmapItem[];
}

type FilterType = 'all' | 'with-issues' | 'without-issues' | 'completed';

export const RoadmapView: React.FC<RoadmapViewProps> = ({
  graphData,
  roadmapItems = [],
}) => {
  const [filter, setFilter] = useState<FilterType>('all');
  const [selectedSection, setSelectedSection] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<DecisionNode | null>(null);

  // Get unique sections
  const sections = useMemo(() => {
    const sectionSet = new Set<string>();
    roadmapItems.forEach(item => {
      if (item.section) sectionSet.add(item.section);
    });
    return Array.from(sectionSet).sort();
  }, [roadmapItems]);

  // Filter items
  const filteredItems = useMemo(() => {
    let items = roadmapItems;

    // Section filter
    if (selectedSection) {
      items = items.filter(item => item.section === selectedSection);
    }

    // Type filter
    switch (filter) {
      case 'with-issues':
        items = items.filter(item => item.github_issue_number !== undefined);
        break;
      case 'without-issues':
        items = items.filter(item => item.github_issue_number === undefined);
        break;
      case 'completed':
        items = items.filter(item =>
          item.checkbox_state === 'checked' &&
          item.github_issue_state === 'closed' &&
          item.outcome_node_id !== undefined
        );
        break;
    }

    return items;
  }, [roadmapItems, selectedSection, filter]);

  // Group items by section
  const groupedItems = useMemo(() => {
    const groups: Record<string, RoadmapItem[]> = {};
    filteredItems.forEach(item => {
      const section = item.section || 'Uncategorized';
      if (!groups[section]) groups[section] = [];
      groups[section].push(item);
    });
    return groups;
  }, [filteredItems]);

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

        {/* Filters */}
        <div style={styles.filterSection}>
          <label style={styles.filterLabel}>Status</label>
          <div style={styles.filterButtons}>
            {(['all', 'with-issues', 'without-issues', 'completed'] as FilterType[]).map(f => (
              <button
                key={f}
                onClick={() => setFilter(f)}
                style={{
                  ...styles.filterBtn,
                  ...(filter === f ? styles.filterBtnActive : {}),
                }}
              >
                {f.replace('-', ' ').replace(/^\w/, c => c.toUpperCase())}
              </button>
            ))}
          </div>
        </div>

        {/* Section filter */}
        {sections.length > 0 && (
          <div style={styles.filterSection}>
            <label style={styles.filterLabel}>Section</label>
            <select
              value={selectedSection || ''}
              onChange={e => setSelectedSection(e.target.value || null)}
              style={styles.select}
            >
              <option value="">All Sections</option>
              {sections.map(s => (
                <option key={s} value={s}>{s}</option>
              ))}
            </select>
          </div>
        )}

        {/* Stats */}
        <div style={styles.stats}>
          <div style={styles.statItem}>
            <span style={styles.statNum}>{filteredItems.length}</span>
            <span style={styles.statLabel}>Items</span>
          </div>
          <div style={styles.statItem}>
            <span style={styles.statNum}>
              {filteredItems.filter(i => i.github_issue_number).length}
            </span>
            <span style={styles.statLabel}>With Issues</span>
          </div>
          <div style={styles.statItem}>
            <span style={styles.statNum}>
              {filteredItems.filter(i => i.checkbox_state === 'checked').length}
            </span>
            <span style={styles.statLabel}>Completed</span>
          </div>
        </div>
      </div>

      {/* Main content */}
      <div style={styles.content}>
        {Object.entries(groupedItems).map(([section, items]) => (
          <div key={section} style={styles.sectionGroup}>
            <h3 style={styles.sectionTitle}>{section}</h3>
            <div style={styles.itemList}>
              {items.map(item => (
                <RoadmapItemCard
                  key={item.id}
                  item={item}
                  onSelectOutcome={handleSelectOutcome}
                />
              ))}
            </div>
          </div>
        ))}

        {filteredItems.length === 0 && (
          <div style={styles.emptyFiltered}>
            No items match your filters
          </div>
        )}
      </div>

      {/* Detail panel for linked nodes */}
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
    </div>
  );
};

// =============================================================================
// Roadmap Item Card
// =============================================================================

interface RoadmapItemCardProps {
  item: RoadmapItem;
  onSelectOutcome: (id: number) => void;
}

const RoadmapItemCard: React.FC<RoadmapItemCardProps> = ({ item, onSelectOutcome }) => {
  const isComplete = item.checkbox_state === 'checked' &&
    item.github_issue_state === 'closed' &&
    item.outcome_node_id !== undefined;

  return (
    <div style={{
      ...styles.card,
      ...(isComplete ? styles.cardComplete : {}),
    }}>
      <div style={styles.cardHeader}>
        {/* Checkbox */}
        <span style={styles.checkbox}>
          {item.checkbox_state === 'checked' ? '☑' : '☐'}
        </span>

        {/* Title */}
        <span style={styles.cardTitle}>{item.title}</span>

        {/* Badges */}
        <div style={styles.badges}>
          {item.github_issue_number && (
            <a
              href={`https://github.com/notactuallytreyanastasio/deciduous/issues/${item.github_issue_number}`}
              target="_blank"
              rel="noopener noreferrer"
              style={{
                ...styles.issueBadge,
                ...(item.github_issue_state === 'closed' ? styles.issueClosed : styles.issueOpen),
              }}
            >
              #{item.github_issue_number}
            </a>
          )}
          {item.outcome_node_id && (
            <button
              onClick={() => onSelectOutcome(item.outcome_node_id!)}
              style={styles.outcomeBadge}
            >
              ⚡ Outcome
            </button>
          )}
        </div>
      </div>

      {item.description && (
        <p style={styles.cardDesc}>{item.description}</p>
      )}

      {/* Completion status */}
      {isComplete && (
        <div style={styles.completeBadge}>
          ✓ Complete
        </div>
      )}
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
    width: '220px',
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
  select: {
    width: '100%',
    padding: '8px',
    fontSize: '12px',
    border: '1px solid #d0d7de',
    borderRadius: '4px',
    backgroundColor: '#ffffff',
  },
  stats: {
    display: 'flex',
    flexDirection: 'column',
    gap: '8px',
    marginTop: '20px',
    paddingTop: '20px',
    borderTop: '1px solid #d0d7de',
  },
  statItem: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  statNum: {
    fontSize: '14px',
    fontWeight: 600,
    color: '#24292f',
  },
  statLabel: {
    fontSize: '12px',
    color: '#6e7781',
  },
  content: {
    flex: 2,
    overflowY: 'auto',
    padding: '20px',
    backgroundColor: '#ffffff',
  },
  sectionGroup: {
    marginBottom: '24px',
  },
  sectionTitle: {
    fontSize: '14px',
    fontWeight: 600,
    color: '#6e7781',
    margin: '0 0 12px 0',
    textTransform: 'uppercase',
    letterSpacing: '0.5px',
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
    transition: 'border-color 0.2s',
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
    color: '#57606a',
  },
  cardTitle: {
    flex: 1,
    fontSize: '14px',
    color: '#24292f',
  },
  badges: {
    display: 'flex',
    gap: '6px',
  },
  issueBadge: {
    fontSize: '11px',
    padding: '2px 8px',
    borderRadius: '10px',
    textDecoration: 'none',
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
    margin: '8px 0 0 24px',
    lineHeight: 1.4,
  },
  completeBadge: {
    marginTop: '8px',
    marginLeft: '24px',
    fontSize: '11px',
    color: '#1a7f37',
    fontWeight: 500,
  },
  detailPanel: {
    flex: 1,
    minWidth: '300px',
    borderLeft: '1px solid #d0d7de',
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
};
