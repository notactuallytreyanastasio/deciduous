/**
 * Callout Lines Component
 *
 * Draws SVG callout lines from tiny nodes to floating labels.
 * Used when search matches are visible but too small to read.
 *
 * Desktop: Rich cards on the right side with proper spacing
 * Mobile: Tappable circles that open bottom sheet
 */

import React, { useMemo, useState, useEffect } from 'react';
import type { DecisionNode, DecisionEdge } from '../types/graph';
import { truncate } from '../types/graph';
import { getNodeColor } from '../utils/colors';
import type { VisibilityInfo } from '../hooks/useNodeVisibility';

interface CalloutLinesProps {
  nodes: DecisionNode[];
  edges: DecisionEdge[];
  highlightedNodeIds: Set<number>;
  visibilityMap: Map<number, VisibilityInfo>;
  containerWidth: number;
  containerHeight: number;
  onSelectNode: (node: DecisionNode) => void;
  /** Pan/zoom to node location */
  onNavigateToNode?: (node: DecisionNode) => void;
}

// Desktop: Rich card dimensions - taller for connection info
const CARD_WIDTH = 420;
const CARD_HEIGHT = 180;
const CARD_GAP = 12;
const CARD_MARGIN = 20;

// Circle at node position
const NODE_CIRCLE_RADIUS = 10;

// Reserved zones
const TOP_RESERVED = 70;
const RIGHT_PANEL_WIDTH = CARD_WIDTH + CARD_MARGIN * 2;

// Mobile breakpoint
const MOBILE_BREAKPOINT = 768;

interface ConnectionInfo {
  incoming: { node: DecisionNode; edgeType: string; rationale: string | null }[];
  outgoing: { node: DecisionNode; edgeType: string; rationale: string | null }[];
}

// Helper to parse confidence from metadata_json
function getConfidence(node: DecisionNode): number | null {
  if (!node.metadata_json) return null;
  try {
    const meta = JSON.parse(node.metadata_json);
    return typeof meta.confidence === 'number' ? meta.confidence : null;
  } catch {
    return null;
  }
}

interface CalloutData {
  node: DecisionNode;
  nodeX: number;
  nodeY: number;
  cardX: number;
  cardY: number;
  color: string;
  connections: ConnectionInfo;
}

// Calculate card positions - stack on right side with proper spacing
function calculateCardPositions(
  nodes: CalloutData[],
  containerWidth: number,
  containerHeight: number
): CalloutData[] {
  // Sort by Y position for intuitive ordering
  const sorted = [...nodes].sort((a, b) => a.nodeY - b.nodeY);

  // Cards go on the right side
  const cardX = containerWidth - CARD_WIDTH - CARD_MARGIN;

  // Stack from top
  let currentY = TOP_RESERVED;

  return sorted.map((callout, index) => {
    const cardY = currentY;
    currentY += CARD_HEIGHT + CARD_GAP;

    // If we run out of space, wrap (though ideally limit results)
    if (currentY > containerHeight - CARD_MARGIN && index < sorted.length - 1) {
      currentY = TOP_RESERVED;
    }

    return {
      ...callout,
      cardX,
      cardY,
    };
  });
}

// Mobile bottom sheet component
const MobileBottomSheet: React.FC<{
  nodes: CalloutData[];
  selectedNode: DecisionNode | null;
  onSelectNode: (node: DecisionNode) => void;
  onClose: () => void;
}> = ({ nodes, selectedNode, onSelectNode, onClose }) => {
  if (!selectedNode) return null;

  const callout = nodes.find(c => c.node.id === selectedNode.id);
  if (!callout) return null;

  return (
    <div
      style={{
        position: 'fixed',
        bottom: 0,
        left: 0,
        right: 0,
        backgroundColor: '#fff',
        borderTopLeftRadius: 16,
        borderTopRightRadius: 16,
        boxShadow: '0 -4px 20px rgba(0,0,0,0.15)',
        zIndex: 100,
        padding: '16px 20px 24px',
        maxHeight: '50vh',
        overflowY: 'auto',
      }}
    >
      {/* Handle bar */}
      <div
        style={{
          width: 40,
          height: 4,
          backgroundColor: '#d0d7de',
          borderRadius: 2,
          margin: '0 auto 16px',
        }}
      />

      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 12 }}>
        <div
          style={{
            width: 12,
            height: 12,
            borderRadius: '50%',
            backgroundColor: callout.color,
          }}
        />
        <span
          style={{
            backgroundColor: callout.color + '20',
            color: callout.color,
            padding: '4px 10px',
            borderRadius: 4,
            fontSize: 11,
            fontWeight: 600,
          }}
        >
          {selectedNode.node_type.toUpperCase()}
        </span>
        <span style={{ color: '#6e7781', fontFamily: 'monospace', fontSize: 13 }}>
          #{selectedNode.id}
        </span>
        <button
          onClick={onClose}
          style={{
            marginLeft: 'auto',
            background: 'none',
            border: 'none',
            fontSize: 24,
            color: '#6e7781',
            cursor: 'pointer',
            padding: 4,
          }}
        >
          ×
        </button>
      </div>

      {/* Title */}
      <h3 style={{ margin: '0 0 8px', fontSize: 16, color: '#24292f' }}>
        {selectedNode.title}
      </h3>

      {/* Description */}
      {selectedNode.description && (
        <p style={{ margin: '0 0 16px', fontSize: 14, color: '#57606a', lineHeight: 1.5 }}>
          {selectedNode.description}
        </p>
      )}

      {/* Action button */}
      <button
        onClick={() => onSelectNode(selectedNode)}
        style={{
          width: '100%',
          padding: '12px 16px',
          backgroundColor: '#0969da',
          color: '#fff',
          border: 'none',
          borderRadius: 8,
          fontSize: 14,
          fontWeight: 500,
          cursor: 'pointer',
        }}
      >
        View Full Details
      </button>

      {/* Other matches */}
      {nodes.length > 1 && (
        <div style={{ marginTop: 16, paddingTop: 16, borderTop: '1px solid #d0d7de' }}>
          <div style={{ fontSize: 12, color: '#6e7781', marginBottom: 8 }}>
            Other matches ({nodes.length - 1})
          </div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
            {nodes
              .filter(c => c.node.id !== selectedNode.id)
              .slice(0, 6)
              .map(c => (
                <button
                  key={c.node.id}
                  onClick={() => onSelectNode(c.node)}
                  style={{
                    padding: '6px 10px',
                    backgroundColor: '#f6f8fa',
                    border: '1px solid #d0d7de',
                    borderRadius: 6,
                    fontSize: 12,
                    cursor: 'pointer',
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                  }}
                >
                  <span
                    style={{
                      width: 8,
                      height: 8,
                      borderRadius: '50%',
                      backgroundColor: c.color,
                    }}
                  />
                  #{c.node.id}
                </button>
              ))}
          </div>
        </div>
      )}
    </div>
  );
};

// Build full chain tree (all ancestors and descendants)
function buildChainTree(
  nodeId: number,
  nodes: DecisionNode[],
  edges: DecisionEdge[]
): { ancestors: DecisionNode[]; descendants: DecisionNode[] } {
  const ancestors: DecisionNode[] = [];
  const descendants: DecisionNode[] = [];
  const nodeMap = new Map(nodes.map(n => [n.id, n]));

  // BFS for ancestors (nodes pointing TO this node)
  const ancestorQueue = [nodeId];
  const ancestorVisited = new Set<number>([nodeId]);
  while (ancestorQueue.length > 0) {
    const current = ancestorQueue.shift()!;
    const incoming = edges.filter(e => e.to_node_id === current);
    for (const edge of incoming) {
      if (!ancestorVisited.has(edge.from_node_id)) {
        ancestorVisited.add(edge.from_node_id);
        const node = nodeMap.get(edge.from_node_id);
        if (node) {
          ancestors.push(node);
          ancestorQueue.push(edge.from_node_id);
        }
      }
    }
  }

  // BFS for descendants (nodes this points TO)
  const descendantQueue = [nodeId];
  const descendantVisited = new Set<number>([nodeId]);
  while (descendantQueue.length > 0) {
    const current = descendantQueue.shift()!;
    const outgoing = edges.filter(e => e.from_node_id === current);
    for (const edge of outgoing) {
      if (!descendantVisited.has(edge.to_node_id)) {
        descendantVisited.add(edge.to_node_id);
        const node = nodeMap.get(edge.to_node_id);
        if (node) {
          descendants.push(node);
          descendantQueue.push(edge.to_node_id);
        }
      }
    }
  }

  return { ancestors, descendants };
}

// Node type priority for sorting
const NODE_TYPE_PRIORITY: Record<string, number> = {
  goal: 0,
  outcome: 1,
  decision: 2,
  option: 3,
  action: 4,
  observation: 5,
};

export const CalloutLines: React.FC<CalloutLinesProps> = ({
  nodes,
  edges,
  highlightedNodeIds,
  visibilityMap,
  containerWidth,
  containerHeight,
  onSelectNode,
  onNavigateToNode,
}) => {
  const [hoveredNodeId, setHoveredNodeId] = useState<number | null>(null);
  const [mobileSelectedNode, setMobileSelectedNode] = useState<DecisionNode | null>(null);
  const [isMobile, setIsMobile] = useState(false);
  const [expandedNodeId, setExpandedNodeId] = useState<number | null>(null);

  // Helper to get node by ID
  const getNode = (id: number) => nodes.find(n => n.id === id);

  // Handle card click - navigate (pan/zoom) to node
  const handleCardClick = (node: DecisionNode) => {
    if (onNavigateToNode) {
      onNavigateToNode(node);
    } else {
      onSelectNode(node);
    }
  };

  // Detect mobile
  useEffect(() => {
    const checkMobile = () => setIsMobile(window.innerWidth < MOBILE_BREAKPOINT);
    checkMobile();
    window.addEventListener('resize', checkMobile);
    return () => window.removeEventListener('resize', checkMobile);
  }, []);

  // Get nodes that need callouts with connection info
  const calloutsNeeded = useMemo(() => {
    const callouts: CalloutData[] = [];

    nodes.forEach((node) => {
      if (!highlightedNodeIds.has(node.id)) return;

      const visibility = visibilityMap.get(node.id);
      if (!visibility || visibility.visibility !== 'too-small') return;

      // Get incoming and outgoing connections
      const incomingEdges = edges.filter(e => e.to_node_id === node.id);
      const outgoingEdges = edges.filter(e => e.from_node_id === node.id);

      const connections: ConnectionInfo = {
        incoming: incomingEdges.map(e => ({
          node: getNode(e.from_node_id)!,
          edgeType: e.edge_type,
          rationale: e.rationale,
        })).filter(c => c.node),
        outgoing: outgoingEdges.map(e => ({
          node: getNode(e.to_node_id)!,
          edgeType: e.edge_type,
          rationale: e.rationale,
        })).filter(c => c.node),
      };

      callouts.push({
        node,
        nodeX: visibility.screenX + visibility.screenWidth / 2,
        nodeY: visibility.screenY + visibility.screenHeight / 2,
        cardX: 0,
        cardY: 0,
        color: getNodeColor(node.node_type),
        connections,
      });
    });

    return calculateCardPositions(callouts, containerWidth, containerHeight);
  }, [nodes, edges, highlightedNodeIds, visibilityMap, containerWidth, containerHeight]);

  // Keep expanded panel even if no callouts (node might have scrolled into view)
  const expandedNode = expandedNodeId !== null ? getNode(expandedNodeId) : null;

  if (calloutsNeeded.length === 0 && !expandedNode) {
    return null;
  }

  // Mobile: just show circles, tap to open bottom sheet
  if (isMobile) {
    return (
      <>
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
          {calloutsNeeded.map((callout) => (
            <g key={callout.node.id}>
              {/* Pulsing indicator circle */}
              <circle
                cx={callout.nodeX}
                cy={callout.nodeY}
                r={NODE_CIRCLE_RADIUS + 6}
                fill={callout.color}
                fillOpacity={0.3}
              >
                <animate
                  attributeName="r"
                  values={`${NODE_CIRCLE_RADIUS + 4};${NODE_CIRCLE_RADIUS + 10};${NODE_CIRCLE_RADIUS + 4}`}
                  dur="2s"
                  repeatCount="indefinite"
                />
                <animate
                  attributeName="fill-opacity"
                  values="0.3;0.1;0.3"
                  dur="2s"
                  repeatCount="indefinite"
                />
              </circle>

              {/* Tappable circle */}
              <circle
                cx={callout.nodeX}
                cy={callout.nodeY}
                r={NODE_CIRCLE_RADIUS + 4}
                fill={callout.color}
                stroke="#fff"
                strokeWidth={2}
                style={{ pointerEvents: 'auto', cursor: 'pointer' }}
                onClick={() => setMobileSelectedNode(callout.node)}
              />

              {/* Node ID label */}
              <text
                x={callout.nodeX}
                y={callout.nodeY + NODE_CIRCLE_RADIUS + 16}
                fill={callout.color}
                fontSize="11"
                fontWeight="600"
                textAnchor="middle"
                style={{ pointerEvents: 'none' }}
              >
                #{callout.node.id}
              </text>
            </g>
          ))}
        </svg>

        {/* Bottom sheet for mobile */}
        <MobileBottomSheet
          nodes={calloutsNeeded}
          selectedNode={mobileSelectedNode}
          onSelectNode={onSelectNode}
          onClose={() => setMobileSelectedNode(null)}
        />
      </>
    );
  }

  // Build chain data for expanded node (outside render to avoid recalc)
  const chainData = expandedNode ? (() => {
    const chain = buildChainTree(expandedNodeId!, nodes, edges);
    const sortByType = (a: DecisionNode, b: DecisionNode) => {
      const aPriority = NODE_TYPE_PRIORITY[a.node_type] ?? 99;
      const bPriority = NODE_TYPE_PRIORITY[b.node_type] ?? 99;
      return aPriority - bPriority;
    };
    return {
      ancestors: [...chain.ancestors].sort(sortByType),
      descendants: [...chain.descendants].sort(sortByType),
    };
  })() : null;

  // Desktop: rich cards on the right side
  return (
    <>
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
      {/* Background panel for cards area */}
      {calloutsNeeded.length > 0 && (
        <rect
          x={containerWidth - RIGHT_PANEL_WIDTH}
          y={0}
          width={RIGHT_PANEL_WIDTH}
          height={containerHeight}
          fill="#f6f8fa"
          fillOpacity={0.85}
        />
      )}

      {calloutsNeeded.map((callout) => {
        const isHovered = hoveredNodeId === callout.node.id;

        // Line from node to card
        const lineEndX = callout.cardX;
        const lineEndY = callout.cardY + CARD_HEIGHT / 2;

        return (
          <g key={callout.node.id}>
            {/* Connection line - curved bezier */}
            <path
              d={`M ${callout.nodeX} ${callout.nodeY}
                  Q ${callout.nodeX + (lineEndX - callout.nodeX) * 0.5} ${callout.nodeY}
                    ${lineEndX} ${lineEndY}`}
              stroke={callout.color}
              strokeWidth={isHovered ? 3 : 2}
              strokeOpacity={isHovered ? 0.9 : 0.5}
              fill="none"
              strokeDasharray={isHovered ? 'none' : '8,4'}
            />

            {/* Circle at node position */}
            <circle
              cx={callout.nodeX}
              cy={callout.nodeY}
              r={isHovered ? NODE_CIRCLE_RADIUS + 3 : NODE_CIRCLE_RADIUS}
              fill={callout.color}
              stroke="#fff"
              strokeWidth={2}
              style={{
                pointerEvents: 'auto',
                cursor: 'pointer',
                transition: 'r 0.15s',
              }}
              onMouseEnter={() => setHoveredNodeId(callout.node.id)}
              onMouseLeave={() => setHoveredNodeId(null)}
              onClick={() => handleCardClick(callout.node)}
            />

            {/* Rich card */}
            <g
              style={{ pointerEvents: 'auto', cursor: 'pointer' }}
              onClick={() => handleCardClick(callout.node)}
              onMouseEnter={() => setHoveredNodeId(callout.node.id)}
              onMouseLeave={() => setHoveredNodeId(null)}
            >
              {/* Card background */}
              <rect
                x={callout.cardX}
                y={callout.cardY}
                width={CARD_WIDTH}
                height={CARD_HEIGHT}
                rx={10}
                fill="#ffffff"
                stroke={isHovered ? callout.color : '#d0d7de'}
                strokeWidth={isHovered ? 2 : 1}
                filter={isHovered ? 'drop-shadow(0 6px 16px rgba(0,0,0,0.18))' : 'drop-shadow(0 2px 6px rgba(0,0,0,0.1))'}
              />

              {/* Colored left border accent */}
              <rect
                x={callout.cardX}
                y={callout.cardY}
                width={4}
                height={CARD_HEIGHT}
                rx={2}
                fill={callout.color}
              />

              {/* Row 1: Type badge + confidence + node ID + date */}
              {/* Type badge */}
              <rect
                x={callout.cardX + 14}
                y={callout.cardY + 10}
                width={72}
                height={20}
                rx={4}
                fill={callout.color}
                fillOpacity={0.15}
              />
              <text
                x={callout.cardX + 50}
                y={callout.cardY + 20}
                fill={callout.color}
                fontSize="10"
                fontWeight="600"
                textAnchor="middle"
                dominantBaseline="middle"
              >
                {callout.node.node_type.toUpperCase()}
              </text>

              {/* Confidence badge */}
              {(() => {
                const conf = getConfidence(callout.node);
                if (conf === null) return null;
                return (
                  <>
                    <rect
                      x={callout.cardX + 92}
                      y={callout.cardY + 10}
                      width={42}
                      height={20}
                      rx={4}
                      fill={conf >= 80 ? '#22c55e' : conf >= 50 ? '#f59e0b' : '#ef4444'}
                      fillOpacity={0.15}
                    />
                    <text
                      x={callout.cardX + 113}
                      y={callout.cardY + 20}
                      fill={conf >= 80 ? '#16a34a' : conf >= 50 ? '#d97706' : '#dc2626'}
                      fontSize="10"
                      fontWeight="600"
                      textAnchor="middle"
                      dominantBaseline="middle"
                    >
                      {conf}%
                    </text>
                  </>
                );
              })()}

              {/* Node ID */}
              <text
                x={callout.cardX + 145}
                y={callout.cardY + 20}
                fill="#6e7781"
                fontSize="11"
                fontFamily="monospace"
                fontWeight="500"
                dominantBaseline="middle"
              >
                Node #{callout.node.id}
              </text>

              {/* Date */}
              <text
                x={callout.cardX + CARD_WIDTH - 14}
                y={callout.cardY + 20}
                fill="#8b949e"
                fontSize="10"
                textAnchor="end"
                dominantBaseline="middle"
              >
                {new Date(callout.node.created_at).toLocaleDateString()}
              </text>

              {/* Row 2: Title */}
              <text
                x={callout.cardX + 14}
                y={callout.cardY + 44}
                fill="#24292f"
                fontSize="14"
                fontWeight="600"
                dominantBaseline="middle"
              >
                {truncate(callout.node.title, 48)}
              </text>

              {/* Row 3: Description (if any) */}
              {callout.node.description && (
                <text
                  x={callout.cardX + 14}
                  y={callout.cardY + 64}
                  fill="#57606a"
                  fontSize="12"
                  dominantBaseline="middle"
                >
                  {truncate(callout.node.description, 55)}
                </text>
              )}

              {/* Separator line */}
              <line
                x1={callout.cardX + 14}
                y1={callout.cardY + 82}
                x2={callout.cardX + CARD_WIDTH - 14}
                y2={callout.cardY + 82}
                stroke="#e5e7eb"
                strokeWidth={1}
              />

              {/* Row 4: Connections section */}
              {/* INCOMING label and count */}
              <text
                x={callout.cardX + 14}
                y={callout.cardY + 98}
                fill="#6e7781"
                fontSize="9"
                fontWeight="600"
                dominantBaseline="middle"
                letterSpacing="0.5"
              >
                INCOMING ({callout.connections.incoming.length})
              </text>

              {/* Incoming connection preview */}
              {callout.connections.incoming.length > 0 ? (
                <g>
                  {callout.connections.incoming.slice(0, 1).map((conn, idx) => (
                    <g key={`in-${idx}`}>
                      <rect
                        x={callout.cardX + 14}
                        y={callout.cardY + 106}
                        width={CARD_WIDTH - 28}
                        height={24}
                        rx={4}
                        fill="#f6f8fa"
                      />
                      <rect
                        x={callout.cardX + 18}
                        y={callout.cardY + 110}
                        width={50}
                        height={16}
                        rx={3}
                        fill={getNodeColor(conn.node.node_type)}
                        fillOpacity={0.15}
                      />
                      <text
                        x={callout.cardX + 43}
                        y={callout.cardY + 118}
                        fill={getNodeColor(conn.node.node_type)}
                        fontSize="8"
                        fontWeight="600"
                        textAnchor="middle"
                        dominantBaseline="middle"
                      >
                        {conn.node.node_type.toUpperCase()}
                      </text>
                      <text
                        x={callout.cardX + 74}
                        y={callout.cardY + 118}
                        fill="#24292f"
                        fontSize="11"
                        dominantBaseline="middle"
                      >
                        {truncate(conn.node.title, 38)}
                      </text>
                    </g>
                  ))}
                  {callout.connections.incoming.length > 1 && (
                    <text
                      x={callout.cardX + CARD_WIDTH - 14}
                      y={callout.cardY + 118}
                      fill="#6e7781"
                      fontSize="9"
                      textAnchor="end"
                      dominantBaseline="middle"
                    >
                      +{callout.connections.incoming.length - 1} more
                    </text>
                  )}
                </g>
              ) : (
                <text
                  x={callout.cardX + 14}
                  y={callout.cardY + 118}
                  fill="#9ca3af"
                  fontSize="10"
                  fontStyle="italic"
                  dominantBaseline="middle"
                >
                  No incoming connections
                </text>
              )}

              {/* OUTGOING label and count */}
              <text
                x={callout.cardX + 14}
                y={callout.cardY + 140}
                fill="#6e7781"
                fontSize="9"
                fontWeight="600"
                dominantBaseline="middle"
                letterSpacing="0.5"
              >
                OUTGOING ({callout.connections.outgoing.length})
              </text>

              {/* Outgoing connection preview */}
              {callout.connections.outgoing.length > 0 ? (
                <g>
                  {callout.connections.outgoing.slice(0, 1).map((conn, idx) => (
                    <g key={`out-${idx}`}>
                      <rect
                        x={callout.cardX + 14}
                        y={callout.cardY + 148}
                        width={CARD_WIDTH - 28}
                        height={24}
                        rx={4}
                        fill="#f6f8fa"
                      />
                      <rect
                        x={callout.cardX + 18}
                        y={callout.cardY + 152}
                        width={50}
                        height={16}
                        rx={3}
                        fill={getNodeColor(conn.node.node_type)}
                        fillOpacity={0.15}
                      />
                      <text
                        x={callout.cardX + 43}
                        y={callout.cardY + 160}
                        fill={getNodeColor(conn.node.node_type)}
                        fontSize="8"
                        fontWeight="600"
                        textAnchor="middle"
                        dominantBaseline="middle"
                      >
                        {conn.node.node_type.toUpperCase()}
                      </text>
                      <text
                        x={callout.cardX + 74}
                        y={callout.cardY + 160}
                        fill="#24292f"
                        fontSize="11"
                        dominantBaseline="middle"
                      >
                        {truncate(conn.node.title, 38)}
                      </text>
                    </g>
                  ))}
                  {callout.connections.outgoing.length > 1 && (
                    <text
                      x={callout.cardX + CARD_WIDTH - 14}
                      y={callout.cardY + 160}
                      fill="#6e7781"
                      fontSize="9"
                      textAnchor="end"
                      dominantBaseline="middle"
                    >
                      +{callout.connections.outgoing.length - 1} more
                    </text>
                  )}
                </g>
              ) : (
                <text
                  x={callout.cardX + 14}
                  y={callout.cardY + 160}
                  fill="#9ca3af"
                  fontSize="10"
                  fontStyle="italic"
                  dominantBaseline="middle"
                >
                  No outgoing connections
                </text>
              )}

              {/* Expand Chain Button */}
              <g
                style={{ pointerEvents: 'auto', cursor: 'pointer' }}
                onClick={(e) => {
                  e.stopPropagation();
                  setExpandedNodeId(callout.node.id);
                  // Pan/zoom to the node when expanding
                  if (onNavigateToNode) {
                    onNavigateToNode(callout.node);
                  }
                }}
              >
                <rect
                  x={callout.cardX + CARD_WIDTH - 40}
                  y={callout.cardY + 8}
                  width={26}
                  height={26}
                  rx={6}
                  fill={isHovered ? '#0969da' : '#f6f8fa'}
                  stroke={isHovered ? '#0969da' : '#d0d7de'}
                  strokeWidth={1}
                />
                <text
                  x={callout.cardX + CARD_WIDTH - 27}
                  y={callout.cardY + 21}
                  fill={isHovered ? '#fff' : '#57606a'}
                  fontSize="16"
                  fontWeight="600"
                  textAnchor="middle"
                  dominantBaseline="middle"
                >
                  +
                </text>
              </g>
            </g>
          </g>
        );
      })}

      {/* Match count header */}
      <text
        x={containerWidth - RIGHT_PANEL_WIDTH + CARD_MARGIN}
        y={TOP_RESERVED - 16}
        fill="#57606a"
        fontSize="12"
        fontWeight="500"
      >
        {calloutsNeeded.length} match{calloutsNeeded.length !== 1 ? 'es' : ''} found
      </text>

    </svg>

    {/* Chain Panel - outside SVG for proper DOM rendering */}
    {expandedNode && chainData && (
      <div
        style={{
          position: 'absolute',
          bottom: 16,
          left: 16,
          width: 320,
          maxHeight: 'calc(100vh - 200px)',
          backgroundColor: '#fff',
          borderRadius: 10,
          padding: 16,
          overflow: 'auto',
          boxShadow: '0 4px 20px rgba(0,0,0,0.15)',
          border: '1px solid #d0d7de',
          zIndex: 50,
          pointerEvents: 'auto',
        }}
      >
        {/* Header */}
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span
              style={{
                backgroundColor: getNodeColor(expandedNode.node_type) + '20',
                color: getNodeColor(expandedNode.node_type),
                padding: '3px 8px',
                borderRadius: 4,
                fontSize: 10,
                fontWeight: 600,
              }}
            >
              {expandedNode.node_type.toUpperCase()}
            </span>
            <span style={{ fontWeight: 600, fontSize: 14 }}>#{expandedNode.id}</span>
          </div>
          <button
            onClick={() => setExpandedNodeId(null)}
            style={{
              background: 'none',
              border: 'none',
              fontSize: 20,
              cursor: 'pointer',
              color: '#6e7781',
              padding: 0,
              lineHeight: 1,
            }}
          >
            ×
          </button>
        </div>

        <h3 style={{ margin: '0 0 12px', fontSize: 14, fontWeight: 600, lineHeight: 1.3 }}>
          {truncate(expandedNode.title, 50)}
        </h3>

        {/* Go to Node button */}
        <button
          onClick={() => {
            handleCardClick(expandedNode);
            setExpandedNodeId(null);
          }}
          style={{
            width: '100%',
            padding: '8px 12px',
            backgroundColor: '#0969da',
            color: '#fff',
            border: 'none',
            borderRadius: 6,
            fontSize: 13,
            fontWeight: 500,
            cursor: 'pointer',
            marginBottom: 16,
          }}
        >
          Go to Node
        </button>

        {/* Ancestors Section */}
        <div style={{ marginBottom: 16 }}>
          <div style={{ fontSize: 11, color: '#6e7781', fontWeight: 600, marginBottom: 6 }}>
            UPSTREAM ({chainData.ancestors.length})
          </div>
          {chainData.ancestors.length === 0 ? (
            <div style={{ color: '#9ca3af', fontSize: 12, fontStyle: 'italic' }}>Root node</div>
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              {chainData.ancestors.slice(0, 5).map((node) => (
                <div
                  key={node.id}
                  onClick={() => {
                    handleCardClick(node);
                    setExpandedNodeId(null);
                  }}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                    padding: '6px 8px',
                    backgroundColor: '#f6f8fa',
                    borderRadius: 4,
                    cursor: 'pointer',
                    fontSize: 12,
                  }}
                >
                  <span
                    style={{
                      backgroundColor: getNodeColor(node.node_type) + '20',
                      color: getNodeColor(node.node_type),
                      padding: '1px 5px',
                      borderRadius: 3,
                      fontSize: 9,
                      fontWeight: 600,
                    }}
                  >
                    {node.node_type.slice(0, 3).toUpperCase()}
                  </span>
                  <span style={{ color: '#24292f', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {truncate(node.title, 30)}
                  </span>
                </div>
              ))}
              {chainData.ancestors.length > 5 && (
                <div style={{ fontSize: 11, color: '#6e7781' }}>+{chainData.ancestors.length - 5} more</div>
              )}
            </div>
          )}
        </div>

        {/* Current Node */}
        <div
          style={{
            padding: '8px 10px',
            backgroundColor: getNodeColor(expandedNode.node_type) + '15',
            borderLeft: `3px solid ${getNodeColor(expandedNode.node_type)}`,
            borderRadius: 4,
            marginBottom: 16,
            fontSize: 12,
          }}
        >
          <div style={{ fontSize: 10, color: '#6e7781', marginBottom: 2 }}>CURRENT</div>
          <div style={{ fontWeight: 600 }}>{truncate(expandedNode.title, 35)}</div>
        </div>

        {/* Descendants Section */}
        <div>
          <div style={{ fontSize: 11, color: '#6e7781', fontWeight: 600, marginBottom: 6 }}>
            DOWNSTREAM ({chainData.descendants.length})
          </div>
          {chainData.descendants.length === 0 ? (
            <div style={{ color: '#9ca3af', fontSize: 12, fontStyle: 'italic' }}>Leaf node</div>
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              {chainData.descendants.slice(0, 5).map((node) => (
                <div
                  key={node.id}
                  onClick={() => {
                    handleCardClick(node);
                    setExpandedNodeId(null);
                  }}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                    padding: '6px 8px',
                    backgroundColor: '#f6f8fa',
                    borderRadius: 4,
                    cursor: 'pointer',
                    fontSize: 12,
                  }}
                >
                  <span
                    style={{
                      backgroundColor: getNodeColor(node.node_type) + '20',
                      color: getNodeColor(node.node_type),
                      padding: '1px 5px',
                      borderRadius: 3,
                      fontSize: 9,
                      fontWeight: 600,
                    }}
                  >
                    {node.node_type.slice(0, 3).toUpperCase()}
                  </span>
                  <span style={{ color: '#24292f', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {truncate(node.title, 30)}
                  </span>
                </div>
              ))}
              {chainData.descendants.length > 5 && (
                <div style={{ fontSize: 11, color: '#6e7781' }}>+{chainData.descendants.length - 5} more</div>
              )}
            </div>
          )}
        </div>
      </div>
    )}
    </>
  );
};

export default CalloutLines;
