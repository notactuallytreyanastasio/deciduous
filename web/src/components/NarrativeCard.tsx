/**
 * NarrativeCard Component
 *
 * Card displaying a narrative summary for the archaeology view.
 */

import React from 'react';
import type { Narrative } from '../types/archaeology';
import { getNodeColor } from '../utils/colors';

interface NarrativeCardProps {
  narrative: Narrative;
  isSelected: boolean;
  onClick: () => void;
}

export const NarrativeCard: React.FC<NarrativeCardProps> = ({
  narrative,
  isSelected,
  onClick,
}) => {
  const { name, root, nodes, pivots, observations, githubLinks, timeRange } = narrative;

  // Format date range
  const formatDate = (date: Date) => {
    return date.toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
    });
  };

  const dateRange = `${formatDate(timeRange.start)} - ${formatDate(timeRange.end)}`;

  // Count unique link types
  const commitCount = githubLinks.filter(l => l.type === 'commit').length;
  const prCount = githubLinks.filter(l => l.type === 'pr').length;
  const issueCount = githubLinks.filter(l => l.type === 'issue').length;

  return (
    <div
      style={{
        ...styles.card,
        ...(isSelected ? styles.cardSelected : {}),
        borderLeftColor: getNodeColor(root.node_type),
      }}
      onClick={onClick}
    >
      {/* Header */}
      <div style={styles.header}>
        <span
          style={{
            ...styles.typeBadge,
            backgroundColor: getNodeColor(root.node_type) + '22',
            color: getNodeColor(root.node_type),
          }}
        >
          {root.node_type}
        </span>
        {isSelected && <span style={styles.selectedIndicator}>Selected</span>}
      </div>

      {/* Title */}
      <h3 style={styles.title}>{name}</h3>

      {/* Stats */}
      <div style={styles.stats}>
        <span style={styles.stat}>{nodes.length} nodes</span>
        {pivots.length > 0 && (
          <span style={{ ...styles.stat, ...styles.pivotStat }}>
            {pivots.length} pivot{pivots.length > 1 ? 's' : ''}
          </span>
        )}
        {observations.length > 0 && (
          <span style={styles.stat}>
            {observations.length} observation{observations.length > 1 ? 's' : ''}
          </span>
        )}
      </div>

      {/* Date Range */}
      <div style={styles.dateRange}>{dateRange}</div>

      {/* GitHub Links */}
      {githubLinks.length > 0 && (
        <div style={styles.links}>
          {commitCount > 0 && (
            <span style={styles.linkChip}>
              {commitCount} commit{commitCount > 1 ? 's' : ''}
            </span>
          )}
          {prCount > 0 && (
            <span style={{ ...styles.linkChip, ...styles.prChip }}>
              {prCount} PR{prCount > 1 ? 's' : ''}
            </span>
          )}
          {issueCount > 0 && (
            <span style={{ ...styles.linkChip, ...styles.issueChip }}>
              {issueCount} issue{issueCount > 1 ? 's' : ''}
            </span>
          )}
        </div>
      )}
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  card: {
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderLeft: '4px solid',
    borderRadius: '8px',
    padding: '16px',
    cursor: 'pointer',
    transition: 'all 0.15s ease',
    marginBottom: '12px',
  },
  cardSelected: {
    backgroundColor: '#ddf4ff',
    borderColor: '#0969da',
  },
  header: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: '8px',
  },
  typeBadge: {
    fontSize: '10px',
    fontWeight: 600,
    textTransform: 'uppercase',
    padding: '3px 8px',
    borderRadius: '4px',
  },
  selectedIndicator: {
    fontSize: '10px',
    color: '#0969da',
    fontWeight: 600,
  },
  title: {
    margin: '0 0 8px 0',
    fontSize: '14px',
    fontWeight: 600,
    color: '#24292f',
    lineHeight: 1.4,
  },
  stats: {
    display: 'flex',
    flexWrap: 'wrap',
    gap: '8px',
    marginBottom: '8px',
  },
  stat: {
    fontSize: '12px',
    color: '#57606a',
  },
  pivotStat: {
    color: '#fb8500',
    fontWeight: 500,
  },
  dateRange: {
    fontSize: '11px',
    color: '#8c959f',
    marginBottom: '8px',
  },
  links: {
    display: 'flex',
    flexWrap: 'wrap',
    gap: '6px',
    marginTop: '8px',
  },
  linkChip: {
    fontSize: '10px',
    padding: '2px 6px',
    borderRadius: '3px',
    backgroundColor: '#f6f8fa',
    color: '#57606a',
    border: '1px solid #d0d7de',
  },
  prChip: {
    backgroundColor: '#dafbe1',
    color: '#1a7f37',
    borderColor: '#aceebb',
  },
  issueChip: {
    backgroundColor: '#fff8c5',
    color: '#9a6700',
    borderColor: '#d4a72c',
  },
};
