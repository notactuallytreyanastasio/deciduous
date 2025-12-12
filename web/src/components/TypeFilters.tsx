/**
 * Type Filters Component
 *
 * Filter buttons for node types.
 */

import React from 'react';
import type { NodeType } from '../types/graph';
import { NODE_TYPES } from '../types/graph';
import { getNodeColor } from '../utils/colors';

export type FilterValue = 'all' | NodeType;

interface TypeFiltersProps {
  value: FilterValue;
  onChange: (value: FilterValue) => void;
  showAll?: boolean;
}

export const TypeFilters: React.FC<TypeFiltersProps> = ({
  value,
  onChange,
  showAll = true,
}) => {
  const filters: FilterValue[] = showAll ? ['all', ...NODE_TYPES] : [...NODE_TYPES];

  return (
    <div style={styles.container}>
      {filters.map((filter) => (
        <button
          key={filter}
          onClick={() => onChange(filter)}
          style={{
            ...styles.button,
            ...(value === filter ? styles.buttonActive : {}),
            ...(filter !== 'all' ? { borderLeftColor: getNodeColor(filter as NodeType) } : {}),
          }}
        >
          {filter === 'all' ? 'All' : filter.charAt(0).toUpperCase() + filter.slice(1)}
        </button>
      ))}
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: 'flex',
    gap: '4px',
    flexWrap: 'wrap',
  },
  button: {
    padding: '6px 12px',
    fontSize: '11px',
    border: '1px solid #d0d7de',
    borderLeft: '3px solid transparent',
    backgroundColor: '#ffffff',
    color: '#57606a',
    borderRadius: '4px',
    cursor: 'pointer',
    transition: 'all 0.2s',
  },
  buttonActive: {
    backgroundColor: '#ddf4ff',
    color: '#0969da',
    borderColor: '#0969da',
  },
};
