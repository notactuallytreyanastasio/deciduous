/**
 * MiniMap Component
 *
 * A small overview of the graph showing the current viewport and
 * indicators for off-screen search matches.
 */

import React, { useMemo } from 'react';
import type { DecisionNode } from '../types/graph';
import { getNodeColor } from '../utils/colors';
import type { VisibilityInfo } from '../hooks/useNodeVisibility';

interface MiniMapProps {
  nodes: DecisionNode[];
  highlightedNodeIds: Set<number>;
  visibilityMap: Map<number, VisibilityInfo>;
  nodePositions: Map<number, { x: number; y: number; width: number; height: number }>;
  graphBounds: { minX: number; maxX: number; minY: number; maxY: number };
  viewportBounds: { x: number; y: number; width: number; height: number };
  zoom: number;
  onNavigateToNode: (node: DecisionNode) => void;
}

const MINIMAP_WIDTH = 150;
const MINIMAP_HEIGHT = 100;
const MINIMAP_PADDING = 10;

export const MiniMap: React.FC<MiniMapProps> = ({
  nodes,
  highlightedNodeIds,
  visibilityMap,
  nodePositions,
  graphBounds,
  viewportBounds,
  zoom,
  onNavigateToNode,
}) => {
  // Calculate scale to fit graph in minimap
  const scale = useMemo(() => {
    const graphWidth = graphBounds.maxX - graphBounds.minX;
    const graphHeight = graphBounds.maxY - graphBounds.minY;
    if (graphWidth === 0 || graphHeight === 0) return 1;

    const scaleX = (MINIMAP_WIDTH - MINIMAP_PADDING * 2) / graphWidth;
    const scaleY = (MINIMAP_HEIGHT - MINIMAP_PADDING * 2) / graphHeight;
    return Math.min(scaleX, scaleY);
  }, [graphBounds]);

  // Get off-screen highlighted nodes
  const offScreenMatches = useMemo(() => {
    return nodes.filter((node) => {
      if (!highlightedNodeIds.has(node.id)) return false;
      const visibility = visibilityMap.get(node.id);
      return visibility?.visibility === 'off-screen';
    });
  }, [nodes, highlightedNodeIds, visibilityMap]);

  // Transform graph coordinates to minimap coordinates
  const toMinimap = (x: number, y: number) => ({
    x: MINIMAP_PADDING + (x - graphBounds.minX) * scale,
    y: MINIMAP_PADDING + (y - graphBounds.minY) * scale,
  });

  // Calculate viewport rectangle in minimap
  const viewportRect = useMemo(() => {
    const topLeft = toMinimap(
      -viewportBounds.x / zoom,
      -viewportBounds.y / zoom
    );
    return {
      x: topLeft.x,
      y: topLeft.y,
      width: (viewportBounds.width / zoom) * scale,
      height: (viewportBounds.height / zoom) * scale,
    };
  }, [viewportBounds, zoom, scale, graphBounds]);

  if (offScreenMatches.length === 0) {
    return null;
  }

  return (
    <div style={styles.container}>
      <div style={styles.header}>
        <span style={styles.title}>Off-screen matches</span>
        <span style={styles.count}>{offScreenMatches.length}</span>
      </div>

      <svg width={MINIMAP_WIDTH} height={MINIMAP_HEIGHT} style={styles.svg}>
        {/* Background */}
        <rect
          x={0}
          y={0}
          width={MINIMAP_WIDTH}
          height={MINIMAP_HEIGHT}
          fill="#f6f8fa"
          rx={4}
        />

        {/* All nodes as small dots */}
        {nodes.map((node) => {
          const pos = nodePositions.get(node.id);
          if (!pos) return null;

          const { x, y } = toMinimap(pos.x, pos.y);
          const isHighlighted = highlightedNodeIds.has(node.id);

          return (
            <circle
              key={node.id}
              cx={x}
              cy={y}
              r={isHighlighted ? 4 : 1.5}
              fill={isHighlighted ? getNodeColor(node.node_type) : '#d0d7de'}
              opacity={isHighlighted ? 1 : 0.5}
            />
          );
        })}

        {/* Viewport rectangle */}
        <rect
          x={Math.max(0, viewportRect.x)}
          y={Math.max(0, viewportRect.y)}
          width={Math.min(viewportRect.width, MINIMAP_WIDTH - viewportRect.x)}
          height={Math.min(viewportRect.height, MINIMAP_HEIGHT - viewportRect.y)}
          fill="none"
          stroke="#0969da"
          strokeWidth={1.5}
          rx={2}
        />
      </svg>

      {/* Clickable list of off-screen nodes */}
      <div style={styles.list}>
        {offScreenMatches.slice(0, 5).map((node) => (
          <div
            key={node.id}
            style={styles.listItem}
            onClick={() => onNavigateToNode(node)}
          >
            <div
              style={{
                ...styles.dot,
                backgroundColor: getNodeColor(node.node_type),
              }}
            />
            <span style={styles.nodeId}>#{node.id}</span>
          </div>
        ))}
        {offScreenMatches.length > 5 && (
          <div style={styles.moreText}>
            +{offScreenMatches.length - 5} more
          </div>
        )}
      </div>
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    position: 'absolute',
    bottom: '20px',
    right: '20px',
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderRadius: '8px',
    boxShadow: '0 4px 12px rgba(0, 0, 0, 0.1)',
    zIndex: 50,
    overflow: 'hidden',
  },
  header: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: '8px 12px',
    borderBottom: '1px solid #d0d7de',
    backgroundColor: '#f6f8fa',
  },
  title: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#57606a',
    textTransform: 'uppercase',
  },
  count: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#cf222e',
    backgroundColor: '#ffebe9',
    padding: '2px 6px',
    borderRadius: '10px',
  },
  svg: {
    display: 'block',
  },
  list: {
    padding: '8px',
    borderTop: '1px solid #d0d7de',
    maxHeight: '100px',
    overflowY: 'auto',
  },
  listItem: {
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
    padding: '4px 6px',
    borderRadius: '4px',
    cursor: 'pointer',
    transition: 'background-color 0.1s',
  },
  dot: {
    width: '8px',
    height: '8px',
    borderRadius: '50%',
    flexShrink: 0,
  },
  nodeId: {
    fontSize: '11px',
    color: '#24292f',
    fontFamily: 'monospace',
  },
  moreText: {
    fontSize: '10px',
    color: '#57606a',
    padding: '4px 6px',
    fontStyle: 'italic',
  },
};

export default MiniMap;
