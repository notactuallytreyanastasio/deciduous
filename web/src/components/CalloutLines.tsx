/**
 * Callout Lines Component
 *
 * Draws SVG callout lines from tiny nodes to floating labels.
 * Used when search matches are visible but too small to read.
 */

import React, { useMemo, useState } from 'react';
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

// Expanded hover card dimensions
const HOVER_WIDTH = 320;
const HOVER_HEIGHT = 120;

// Circle size at node position
const NODE_CIRCLE_RADIUS = 12;

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
  // Track which node is being hovered
  const [hoveredNodeId, setHoveredNodeId] = useState<number | null>(null);

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

        const isHovered = hoveredNodeId === callout.node.id;

        // Calculate hover card position (above the label, clamped to viewport)
        let hoverX = callout.labelX;
        let hoverY = callout.labelY - HOVER_HEIGHT - 8;
        if (hoverY < TOP_RESERVED) {
          hoverY = callout.labelY + LABEL_HEIGHT + 8;
        }
        // Clamp horizontally
        if (hoverX + HOVER_WIDTH > containerWidth - LABEL_MARGIN) {
          hoverX = containerWidth - HOVER_WIDTH - LABEL_MARGIN;
        }

        return (
          <g key={callout.node.id}>
            {/* Connection line */}
            <line
              x1={lineStartX}
              y1={lineStartY}
              x2={lineEndX}
              y2={lineEndY}
              stroke={callout.color}
              strokeWidth={isHovered ? 3 : 2}
              strokeOpacity={isHovered ? 1 : 0.7}
              strokeDasharray="6,3"
            />

            {/* Circle at node position - bigger and interactive */}
            <circle
              cx={lineStartX}
              cy={lineStartY}
              r={isHovered ? NODE_CIRCLE_RADIUS + 4 : NODE_CIRCLE_RADIUS}
              fill={callout.color}
              fillOpacity={isHovered ? 1 : 0.9}
              stroke="#fff"
              strokeWidth={isHovered ? 3 : 2}
              style={{ pointerEvents: 'auto', cursor: 'pointer', transition: 'r 0.15s, stroke-width 0.15s' }}
              onMouseEnter={() => setHoveredNodeId(callout.node.id)}
              onMouseLeave={() => setHoveredNodeId(null)}
              onClick={() => onSelectNode(callout.node)}
            />

            {/* Label box */}
            <g
              style={{ pointerEvents: 'auto', cursor: 'pointer' }}
              onClick={() => onSelectNode(callout.node)}
              onMouseEnter={() => setHoveredNodeId(callout.node.id)}
              onMouseLeave={() => setHoveredNodeId(null)}
            >
              <rect
                x={callout.labelX}
                y={callout.labelY}
                width={LABEL_WIDTH}
                height={LABEL_HEIGHT}
                rx={8}
                fill="#ffffff"
                stroke={isHovered ? '#0969da' : callout.color}
                strokeWidth={isHovered ? 3 : 2}
                filter="drop-shadow(0 3px 6px rgba(0,0,0,0.15))"
              />

              {/* Node type dot */}
              <circle
                cx={callout.labelX + 18}
                cy={callout.labelY + LABEL_HEIGHT / 2}
                r={7}
                fill={callout.color}
              />

              {/* Node ID */}
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

              {/* Node title */}
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

            {/* Hover card with more details */}
            {isHovered && (
              <g style={{ pointerEvents: 'auto' }}>
                <rect
                  x={hoverX}
                  y={hoverY}
                  width={HOVER_WIDTH}
                  height={HOVER_HEIGHT}
                  rx={10}
                  fill="#ffffff"
                  stroke="#d0d7de"
                  strokeWidth={1}
                  filter="drop-shadow(0 4px 12px rgba(0,0,0,0.2))"
                />

                {/* Type badge */}
                <rect
                  x={hoverX + 12}
                  y={hoverY + 12}
                  width={70}
                  height={22}
                  rx={4}
                  fill={callout.color}
                  fillOpacity={0.15}
                />
                <text
                  x={hoverX + 47}
                  y={hoverY + 23}
                  fill={callout.color}
                  fontSize="11"
                  fontWeight="600"
                  textAnchor="middle"
                  dominantBaseline="middle"
                >
                  {callout.node.node_type.toUpperCase()}
                </text>

                {/* Node ID */}
                <text
                  x={hoverX + 92}
                  y={hoverY + 23}
                  fill="#6e7781"
                  fontSize="12"
                  fontFamily="monospace"
                  dominantBaseline="middle"
                >
                  #{callout.node.id}
                </text>

                {/* Full title */}
                <text
                  x={hoverX + 12}
                  y={hoverY + 50}
                  fill="#24292f"
                  fontSize="14"
                  fontWeight="600"
                  dominantBaseline="middle"
                >
                  {truncate(callout.node.title, 40)}
                </text>

                {/* Description preview */}
                <text
                  x={hoverX + 12}
                  y={hoverY + 72}
                  fill="#57606a"
                  fontSize="12"
                  dominantBaseline="middle"
                >
                  {callout.node.description
                    ? truncate(callout.node.description, 50)
                    : 'No description'}
                </text>

                {/* Click hint */}
                <text
                  x={hoverX + 12}
                  y={hoverY + 100}
                  fill="#8b949e"
                  fontSize="11"
                  fontStyle="italic"
                  dominantBaseline="middle"
                >
                  Click to view full details
                </text>
              </g>
            )}
          </g>
        );
      })}
    </svg>
  );
};

export default CalloutLines;
