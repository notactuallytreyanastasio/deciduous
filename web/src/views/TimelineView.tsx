/**
 * Timeline View
 *
 * Port of docs/spelunk-timeline.html - merged git commits + decisions timeline.
 * Simplified initial implementation.
 */

import React, { useState, useMemo } from 'react';
import type { GraphData, DecisionNode, GitCommit, TimelineItem } from '../types/graph';
import { getConfidence, getCommit, truncate } from '../types/graph';
import { buildMergedTimeline } from '../utils/graphProcessing';
import { TypeBadge, ConfidenceBadge, CommitBadge } from '../components/NodeBadge';
import { DetailPanel } from '../components/DetailPanel';
import { getNodeColor } from '../utils/colors';

interface TimelineViewProps {
  graphData: GraphData;
  gitHistory?: GitCommit[];
}

type TimelineFilter = 'all' | 'nodes' | 'commits' | 'linked';

export const TimelineView: React.FC<TimelineViewProps> = ({
  graphData,
  gitHistory = [],
}) => {
  const [filter, setFilter] = useState<TimelineFilter>('all');
  const [searchTerm, setSearchTerm] = useState('');
  const [selectedNode, setSelectedNode] = useState<DecisionNode | null>(null);

  // Build merged timeline
  const timeline = useMemo(() => {
    return buildMergedTimeline(graphData.nodes, gitHistory);
  }, [graphData.nodes, gitHistory]);

  // Apply filters
  const filteredTimeline = useMemo(() => {
    let items = timeline;

    // Type filter
    if (filter === 'nodes') {
      items = items.filter(i => i.type === 'node');
    } else if (filter === 'commits') {
      items = items.filter(i => i.type === 'commit');
    } else if (filter === 'linked') {
      items = items.filter(i =>
        (i.type === 'node' && i.linkedCommits && i.linkedCommits.length > 0) ||
        (i.type === 'commit' && i.linkedNodes && i.linkedNodes.length > 0)
      );
    }

    // Search filter
    if (searchTerm) {
      const term = searchTerm.toLowerCase();
      items = items.filter(i => {
        if (i.type === 'node') {
          return i.node!.title.toLowerCase().includes(term) ||
            (i.node!.description?.toLowerCase().includes(term) ?? false);
        } else {
          return i.commit!.message.toLowerCase().includes(term);
        }
      });
    }

    return items;
  }, [timeline, filter, searchTerm]);

  const handleSelectNode = (id: number) => {
    const node = graphData.nodes.find(n => n.id === id);
    if (node) setSelectedNode(node);
  };

  return (
    <div style={styles.container}>
      {/* Controls */}
      <div style={styles.controls}>
        <h2 style={styles.title}>Timeline</h2>
        <div style={styles.filterButtons}>
          {(['all', 'nodes', 'commits', 'linked'] as TimelineFilter[]).map(f => (
            <button
              key={f}
              onClick={() => setFilter(f)}
              style={{
                ...styles.filterBtn,
                ...(filter === f ? styles.filterBtnActive : {}),
              }}
            >
              {f === 'all' ? 'All' : f.charAt(0).toUpperCase() + f.slice(1)}
            </button>
          ))}
        </div>
        <input
          type="text"
          placeholder="Search..."
          value={searchTerm}
          onChange={e => setSearchTerm(e.target.value)}
          style={styles.search}
        />
      </div>

      {/* Timeline */}
      <div style={styles.timelineContainer}>
        <div style={styles.timeline}>
          {filteredTimeline.map((item, i) => (
            <TimelineItemCard
              key={i}
              item={item}
              onSelectNode={handleSelectNode}
            />
          ))}
          {filteredTimeline.length === 0 && (
            <div style={styles.empty}>
              No items match your filters
            </div>
          )}
        </div>
      </div>

      {/* Detail Panel */}
      <div style={styles.detailPanel}>
        <DetailPanel
          node={selectedNode}
          graphData={graphData}
          onSelectNode={handleSelectNode}
          onClose={() => setSelectedNode(null)}
          gitHistory={gitHistory}
        />
      </div>
    </div>
  );
};

// =============================================================================
// Timeline Item Card
// =============================================================================

interface TimelineItemCardProps {
  item: TimelineItem;
  onSelectNode: (id: number) => void;
}

const TimelineItemCard: React.FC<TimelineItemCardProps> = ({ item, onSelectNode }) => {
  const dateStr = item.timestamp.toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
  });
  const timeStr = item.timestamp.toLocaleTimeString('en-US', {
    hour: 'numeric',
    minute: '2-digit',
  });

  if (item.type === 'node' && item.node) {
    const node = item.node;
    const conf = getConfidence(node);
    const commit = getCommit(node);

    return (
      <div
        style={styles.timelineItem}
        onClick={() => onSelectNode(node.id)}
      >
        <div style={{
          ...styles.marker,
          backgroundColor: getNodeColor(node.node_type),
        }} />
        <div style={styles.itemContent}>
          <div style={styles.itemHeader}>
            <TypeBadge type={node.node_type} size="sm" />
            <ConfidenceBadge confidence={conf} />
            {commit && <CommitBadge commit={commit} />}
            <span style={styles.itemTime}>{dateStr} {timeStr}</span>
          </div>
          <div style={styles.itemTitle}>{node.title}</div>
          {node.description && (
            <div style={styles.itemDesc}>{truncate(node.description, 100)}</div>
          )}
          {item.linkedCommits && item.linkedCommits.length > 0 && (
            <div style={styles.linked}>
              Linked to {item.linkedCommits.length} commit(s)
            </div>
          )}
        </div>
      </div>
    );
  }

  if (item.type === 'commit' && item.commit) {
    const commit = item.commit;

    return (
      <div style={styles.timelineItem}>
        <div style={{ ...styles.marker, backgroundColor: '#3b82f6' }} />
        <div style={styles.itemContent}>
          <div style={styles.itemHeader}>
            <span style={styles.commitBadgeSmall}>commit</span>
            <CommitBadge commit={commit.hash} />
            <span style={styles.itemTime}>{dateStr} {timeStr}</span>
          </div>
          <div style={styles.itemTitle}>{commit.message}</div>
          <div style={styles.itemMeta}>
            by {commit.author}
            {commit.files_changed && ` · ${commit.files_changed} files`}
          </div>
          {item.linkedNodes && item.linkedNodes.length > 0 && (
            <div style={styles.linked}>
              Linked to {item.linkedNodes.length} decision(s)
            </div>
          )}
        </div>
      </div>
    );
  }

  return null;
};

// =============================================================================
// Styles
// =============================================================================

const styles: Record<string, React.CSSProperties> = {
  container: {
    height: '100%',
    display: 'flex',
    gap: '0',
  },
  controls: {
    width: '200px',
    padding: '20px',
    backgroundColor: '#f6f8fa',
    borderRight: '1px solid #d0d7de',
    flexShrink: 0,
  },
  title: {
    fontSize: '16px',
    margin: '0 0 15px 0',
    color: '#24292f',
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
  search: {
    width: '100%',
    padding: '8px',
    marginTop: '15px',
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderRadius: '4px',
    color: '#24292f',
    fontSize: '12px',
  },
  timelineContainer: {
    flex: 2,
    overflowY: 'auto',
    padding: '20px',
    backgroundColor: '#ffffff',
  },
  timeline: {
    maxWidth: '700px',
    position: 'relative',
    paddingLeft: '30px',
  },
  timelineItem: {
    position: 'relative',
    marginBottom: '15px',
    padding: '15px',
    backgroundColor: '#ffffff',
    borderRadius: '8px',
    border: '1px solid #d0d7de',
    cursor: 'pointer',
    transition: 'border-color 0.2s',
  },
  marker: {
    position: 'absolute',
    left: '-24px',
    top: '20px',
    width: '12px',
    height: '12px',
    borderRadius: '50%',
    border: '2px solid #ffffff',
  },
  itemContent: {},
  itemHeader: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    marginBottom: '6px',
    flexWrap: 'wrap',
  },
  itemTime: {
    fontSize: '11px',
    color: '#6e7781',
    marginLeft: 'auto',
  },
  itemTitle: {
    fontSize: '14px',
    color: '#24292f',
    marginBottom: '4px',
  },
  itemDesc: {
    fontSize: '12px',
    color: '#57606a',
    lineHeight: 1.4,
  },
  itemMeta: {
    fontSize: '11px',
    color: '#6e7781',
  },
  linked: {
    fontSize: '11px',
    color: '#1a7f37',
    marginTop: '8px',
  },
  commitBadgeSmall: {
    fontSize: '9px',
    padding: '2px 6px',
    backgroundColor: '#ddf4ff',
    color: '#0969da',
    borderRadius: '3px',
    textTransform: 'uppercase',
  },
  detailPanel: {
    flex: 1,
    minWidth: '350px',
    borderLeft: '1px solid #d0d7de',
  },
  empty: {
    textAlign: 'center',
    color: '#6e7781',
    padding: '40px',
  },
};
