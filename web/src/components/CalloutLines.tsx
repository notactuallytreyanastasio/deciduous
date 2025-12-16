/**
 * Callout Lines Component
 *
 * Draws SVG callout lines from tiny nodes to floating labels.
 * Used when search matches are visible but too small to read.
 */

import React, { useMemo } from 'react';
import type { DecisionNode } from '../types/graph';
import { truncate } from '../types/graph';
import { getNodeColor } from '../utils/colors';
import type { VisibilityInfo } from '../hooks/useNodeVisibility';

interface CalloutLinesProps {
  nodes: DecisionNode[];
  highlightedNodeIds: Set<number>;
  visibilityMap: Map<number, VisibilityInfo>;
  containerWidth: number;
  containerHeight: number;
  onSelectNode: (node: DecisionNode) => void;
}

// Label dimensions - sized to show more text while fitting mobile
const LABEL_WIDTH = 280;
const LABEL_HEIGHT = 40;
const LABEL_MARGIN = 16;
const LABEL_PADDING = 10;

// Reserved zone at top (for top bar)
const TOP_RESERVED = 80;

interface CalloutData {
  node: DecisionNode;
  nodeX: number;
  nodeY: number;
  labelX: number;
  labelY: number;
  color: string;
}

// Calculate label positions - always on the right side to avoid legend
function calculateLabelPositions(
  nodes: CalloutData[],
  containerWidth: number,
  containerHeight: number
): CalloutData[] {
  // Sort nodes by Y position for intuitive vertical stacking
  const sorted = [...nodes].sort((a, b) => a.nodeY - b.nodeY);

  // All labels go on the right side
  const labelX = containerWidth - LABEL_WIDTH - LABEL_MARGIN;

  // Start stacking from top (below top bar)
  let currentY = TOP_RESERVED + LABEL_MARGIN;

  // Track used Y positions to avoid overlap
  const usedSlots: number[] = [];

  return sorted.map((callout) => {
    // Find next available Y slot
    let labelY = currentY;

    // Check if this slot overlaps with any used slot
    while (usedSlots.some(usedY => Math.abs(usedY - labelY) < LABEL_HEIGHT + LABEL_PADDING)) {
      labelY += LABEL_HEIGHT + LABEL_PADDING;
    }

    // If we've gone past the bottom, wrap to a second column further left
    if (labelY + LABEL_HEIGHT > containerHeight - LABEL_MARGIN) {
      // Don't wrap into the left reserved zone
      labelY = TOP_RESERVED + LABEL_MARGIN;
    }

    usedSlots.push(labelY);
    currentY = labelY + LABEL_HEIGHT + LABEL_PADDING;

    return {
      ...callout,
      labelX,
      labelY,
    };
  });
}

export const CalloutLines: React.FC<CalloutLinesProps> = ({
  nodes,
  highlightedNodeIds,
  visibilityMap,
  containerWidth,
  containerHeight,
  onSelectNode,
}) => {
  // Get nodes that need callouts (too-small but highlighted)
  const calloutsNeeded = useMemo(() => {
    const callouts: CalloutData[] = [];

    nodes.forEach((node) => {
      if (!highlightedNodeIds.has(node.id)) return;

      const visibility = visibilityMap.get(node.id);
      if (!visibility || visibility.visibility !== 'too-small') return;

      callouts.push({
        node,
        nodeX: visibility.screenX + visibility.screenWidth / 2,
        nodeY: visibility.screenY + visibility.screenHeight / 2,
        labelX: 0, // Will be calculated
        labelY: 0, // Will be calculated
        color: getNodeColor(node.node_type),
      });
    });

    return calculateLabelPositions(callouts, containerWidth, containerHeight);
  }, [nodes, highlightedNodeIds, visibilityMap, containerWidth, containerHeight]);

  if (calloutsNeeded.length === 0) {
    return null;
  }

  return (
    <svg
      style={{
        position: 'absolute',
        top: 0,
        left: 0,
        width: '100%',
        height: '100%',
        pointerEvents: 'none',
        zIndex: 40,
      }}
    >
      {calloutsNeeded.map((callout) => {
        // Calculate line endpoints - line connects from node to left edge of label
        const lineStartX = callout.nodeX;
        const lineStartY = callout.nodeY;
        const lineEndX = callout.labelX;
        const lineEndY = callout.labelY + LABEL_HEIGHT / 2;

        return (
          <g key={callout.node.id}>
            {/* Connection line */}
            <line
              x1={lineStartX}
              y1={lineStartY}
              x2={lineEndX}
              y2={lineEndY}
              stroke={callout.color}
              strokeWidth={2}
              strokeOpacity={0.7}
              strokeDasharray="6,3"
            />

            {/* Circle at node position - bigger */}
            <circle
              cx={lineStartX}
              cy={lineStartY}
              r={8}
              fill={callout.color}
              fillOpacity={0.9}
              stroke="#fff"
              strokeWidth={2}
            />

            {/* Label box */}
            <g
              style={{ pointerEvents: 'auto', cursor: 'pointer' }}
              onClick={() => onSelectNode(callout.node)}
            >
              <rect
                x={callout.labelX}
                y={callout.labelY}
                width={LABEL_WIDTH}
                height={LABEL_HEIGHT}
                rx={8}
                fill="#ffffff"
                stroke={callout.color}
                strokeWidth={2}
                filter="drop-shadow(0 3px 6px rgba(0,0,0,0.15))"
              />

              {/* Node type dot - bigger */}
              <circle
                cx={callout.labelX + 18}
                cy={callout.labelY + LABEL_HEIGHT / 2}
                r={7}
                fill={callout.color}
              />

              {/* Node ID - bigger font */}
              <text
                x={callout.labelX + 34}
                y={callout.labelY + LABEL_HEIGHT / 2 + 1}
                fill="#6e7781"
                fontSize="12"
                fontFamily="monospace"
                fontWeight="500"
                dominantBaseline="middle"
              >
                #{callout.node.id}
              </text>

              {/* Node title - bigger font, more chars */}
              <text
                x={callout.labelX + 72}
                y={callout.labelY + LABEL_HEIGHT / 2 + 1}
                fill="#24292f"
                fontSize="13"
                fontWeight="500"
                dominantBaseline="middle"
              >
                {truncate(callout.node.title, 32)}
              </text>
            </g>
          </g>
        );
      })}
    </svg>
  );
};

export default CalloutLines;
