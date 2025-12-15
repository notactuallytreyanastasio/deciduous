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

interface CalloutData {
  node: DecisionNode;
  nodeX: number;
  nodeY: number;
  labelX: number;
  labelY: number;
  color: string;
}

// Calculate label positions that don't overlap
function calculateLabelPositions(
  nodes: CalloutData[],
  containerWidth: number,
  containerHeight: number
): CalloutData[] {
  const labelWidth = 180;
  const labelHeight = 32;
  const margin = 20;
  const padding = 8;

  // Sort nodes by Y position for better layout
  const sorted = [...nodes].sort((a, b) => a.nodeY - b.nodeY);

  // Place labels on the right side of the container by default
  let currentY = margin;
  const rightX = containerWidth - labelWidth - margin;

  return sorted.map((callout) => {
    // Try right side first
    let labelX = rightX;
    let labelY = currentY;

    // If node is on the right side, place label on left
    if (callout.nodeX > containerWidth * 0.6) {
      labelX = margin;
    }

    // Update Y position for next label
    currentY = labelY + labelHeight + padding;

    // If we've gone past the bottom, reset to top on the other side
    if (currentY + labelHeight > containerHeight) {
      currentY = margin;
    }

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
        const labelWidth = 180;
        const labelHeight = 32;

        // Calculate line endpoints
        const lineStartX = callout.nodeX;
        const lineStartY = callout.nodeY;
        const lineEndX = callout.labelX + (callout.labelX < callout.nodeX ? labelWidth : 0);
        const lineEndY = callout.labelY + labelHeight / 2;

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
              strokeOpacity={0.6}
              strokeDasharray="4,2"
            />

            {/* Circle at node position */}
            <circle
              cx={lineStartX}
              cy={lineStartY}
              r={6}
              fill={callout.color}
              fillOpacity={0.8}
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
                width={labelWidth}
                height={labelHeight}
                rx={6}
                fill="#ffffff"
                stroke={callout.color}
                strokeWidth={2}
                filter="drop-shadow(0 2px 4px rgba(0,0,0,0.1))"
              />

              {/* Node type dot */}
              <circle
                cx={callout.labelX + 14}
                cy={callout.labelY + labelHeight / 2}
                r={5}
                fill={callout.color}
              />

              {/* Node ID */}
              <text
                x={callout.labelX + 26}
                y={callout.labelY + labelHeight / 2 + 1}
                fill="#6e7781"
                fontSize="10"
                fontFamily="monospace"
                dominantBaseline="middle"
              >
                #{callout.node.id}
              </text>

              {/* Node title */}
              <text
                x={callout.labelX + 56}
                y={callout.labelY + labelHeight / 2 + 1}
                fill="#24292f"
                fontSize="12"
                dominantBaseline="middle"
              >
                {truncate(callout.node.title, 18)}
              </text>
            </g>
          </g>
        );
      })}
    </svg>
  );
};

export default CalloutLines;
