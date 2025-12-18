/**
 * Callout Lines Component
 *
 * Draws SVG callout lines from tiny nodes to floating labels.
 * Used when search matches are visible but too small to read.
 *
 * Desktop: Scrollable panel on right with HTML cards
 * Mobile: Tappable circles that open bottom sheet
 */

import React, { useMemo, useState, useEffect, useRef } from 'react';
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

// Desktop card dimensions
const CARD_WIDTH = 380;
const CARD_MARGIN = 12;

// Circle at node position
const NODE_CIRCLE_RADIUS = 10;

// Reserved zones
const TOP_RESERVED = 55;
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
  color: string;
  connections: ConnectionInfo;
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

// Result Card Component - HTML-based for proper scrolling
const ResultCard: React.FC<{
  callout: CalloutData;
  isHovered: boolean;
  onHover: (nodeId: number | null) => void;
  onExpand: (nodeId: number) => void;
  onNavigate: (node: DecisionNode) => void;
  onOpenModal: (node: DecisionNode) => void;
}> = ({ callout, isHovered, onHover, onExpand, onNavigate, onOpenModal }) => {
  const conf = getConfidence(callout.node);

  return (
    <div
      style={{
        backgroundColor: '#fff',
        border: `1px solid ${isHovered ? callout.color : '#d0d7de'}`,
        borderLeft: `4px solid ${callout.color}`,
        borderRadius: 8,
        padding: 12,
        marginBottom: 10,
        cursor: 'pointer',
        boxShadow: isHovered ? '0 4px 12px rgba(0,0,0,0.12)' : '0 1px 3px rgba(0,0,0,0.08)',
        transition: 'all 0.15s',
      }}
      onMouseEnter={() => onHover(callout.node.id)}
      onMouseLeave={() => onHover(null)}
      onClick={() => onExpand(callout.node.id)}
    >
      {/* Row 1: Type badge + confidence + node ID + Chain button */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8, flexWrap: 'wrap' }}>
        <span
          style={{
            backgroundColor: callout.color + '20',
            color: callout.color,
            padding: '3px 8px',
            borderRadius: 4,
            fontSize: 10,
            fontWeight: 600,
            cursor: 'pointer',
          }}
          onClick={(e) => {
            e.stopPropagation();
            onOpenModal(callout.node);
          }}
          title="Click to open modal and snap to node"
        >
          {callout.node.node_type.toUpperCase()}
        </span>
        {conf !== null && (
          <span
            style={{
              backgroundColor: conf >= 80 ? '#dcfce7' : conf >= 50 ? '#fef3c7' : '#fee2e2',
              color: conf >= 80 ? '#16a34a' : conf >= 50 ? '#d97706' : '#dc2626',
              padding: '3px 8px',
              borderRadius: 4,
              fontSize: 10,
              fontWeight: 600,
            }}
          >
            {conf}%
          </span>
        )}
        <span style={{ color: '#6e7781', fontFamily: 'monospace', fontSize: 11 }}>
          #{callout.node.id}
        </span>
        <button
          onClick={(e) => {
            e.stopPropagation();
            onNavigate(callout.node);
            onExpand(callout.node.id);
          }}
          style={{
            marginLeft: 'auto',
            padding: '4px 10px',
            backgroundColor: isHovered ? '#0969da' : '#f6f8fa',
            color: isHovered ? '#fff' : '#57606a',
            border: `1px solid ${isHovered ? '#0969da' : '#d0d7de'}`,
            borderRadius: 4,
            fontSize: 11,
            fontWeight: 500,
            cursor: 'pointer',
            transition: 'all 0.15s',
          }}
          title="View node chain"
        >
          Chain
        </button>
      </div>

      {/* Row 2: Title */}
      <div style={{ fontSize: 13, fontWeight: 600, color: '#24292f', marginBottom: 6 }}>
        {truncate(callout.node.title, 50)}
      </div>

      {/* Row 3: Connections summary */}
      <div style={{ display: 'flex', gap: 12, fontSize: 11, color: '#6e7781' }}>
        <span>
          <strong style={{ color: '#57606a' }}>INCOMING</strong> ({callout.connections.incoming.length})
        </span>
        <span>
          <strong style={{ color: '#57606a' }}>OUTGOING</strong> ({callout.connections.outgoing.length})
        </span>
      </div>

      {/* Row 4: Connection preview */}
      {callout.connections.incoming.length > 0 && (
        <div style={{ marginTop: 8, padding: '6px 8px', backgroundColor: '#f6f8fa', borderRadius: 4, fontSize: 11 }}>
          <span
            style={{
              backgroundColor: getNodeColor(callout.connections.incoming[0].node.node_type) + '20',
              color: getNodeColor(callout.connections.incoming[0].node.node_type),
              padding: '1px 6px',
              borderRadius: 3,
              fontSize: 9,
              fontWeight: 600,
              marginRight: 6,
              cursor: 'pointer',
            }}
            onClick={(e) => {
              e.stopPropagation();
              onOpenModal(callout.connections.incoming[0].node);
            }}
            title="Click to open modal and snap to node"
          >
            {callout.connections.incoming[0].node.node_type.toUpperCase()}
          </span>
          <span style={{ color: '#24292f' }}>
            {truncate(callout.connections.incoming[0].node.title, 35)}
          </span>
          {callout.connections.incoming.length > 1 && (
            <span style={{ color: '#9ca3af', marginLeft: 8 }}>
              +{callout.connections.incoming.length - 1}
            </span>
          )}
        </div>
      )}
    </div>
  );
};

export const CalloutLines: React.FC<CalloutLinesProps> = ({
  nodes,
  edges,
  highlightedNodeIds,
  visibilityMap,
  containerWidth: passedWidth,
  containerHeight: passedHeight,
  onSelectNode,
  onNavigateToNode,
}) => {
  const [hoveredNodeId, setHoveredNodeId] = useState<number | null>(null);
  const [mobileSelectedNode, setMobileSelectedNode] = useState<DecisionNode | null>(null);
  const [isMobile, setIsMobile] = useState(false);
  const [expandedNodeId, setExpandedNodeId] = useState<number | null>(null);
  const [chainSearch, setChainSearch] = useState('');
  const panelRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [measuredDimensions, setMeasuredDimensions] = useState({ width: 0, height: 0 });

  // Get window dimensions as ultimate fallback
  const [windowDimensions, setWindowDimensions] = useState({
    width: typeof window !== 'undefined' ? window.innerWidth : 1920,
    height: typeof window !== 'undefined' ? window.innerHeight : 1080,
  });

  // Track window size changes
  useEffect(() => {
    const handleResize = () => {
      setWindowDimensions({
        width: window.innerWidth,
        height: window.innerHeight,
      });
    };
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []);

  // Measure container dimensions directly for accurate line positioning
  // Re-measure when passed dimensions change (indicates parent resized)
  useEffect(() => {
    const measure = () => {
      if (containerRef.current) {
        const rect = containerRef.current.getBoundingClientRect();
        if (rect.width > 0) {
          setMeasuredDimensions({ width: rect.width, height: rect.height });
        }
      }
    };
    // Measure immediately and after delays to handle layout settling
    measure();
    const timer1 = setTimeout(measure, 50);
    const timer2 = setTimeout(measure, 200);
    window.addEventListener('resize', measure);
    return () => {
      clearTimeout(timer1);
      clearTimeout(timer2);
      window.removeEventListener('resize', measure);
    };
  }, [passedWidth, passedHeight]);

  // Use measured dimensions, with fallbacks: measured -> passed -> window
  // The container should be roughly window-sized in fullscreen mode
  const containerWidth = measuredDimensions.width > 100
    ? measuredDimensions.width
    : (passedWidth > 100 ? passedWidth : windowDimensions.width);
  const containerHeight = measuredDimensions.height > 100
    ? measuredDimensions.height
    : (passedHeight > 100 ? passedHeight : windowDimensions.height);

  // Clear chain search when expanded node changes
  useEffect(() => {
    setChainSearch('');
  }, [expandedNodeId]);

  // Helper to get node by ID
  const getNode = (id: number) => nodes.find(n => n.id === id);

  // Handle card click - navigate (pan/zoom) to node
  const handleCardClick = (node: DecisionNode) => {
    if (onNavigateToNode) {
      onNavigateToNode(node);
    }
  };

  // Handle opening modal and snapping to node
  const handleOpenModal = (node: DecisionNode) => {
    onSelectNode(node);
    if (onNavigateToNode) {
      onNavigateToNode(node);
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
        color: getNodeColor(node.node_type),
        connections,
      });
    });

    // Sort by Y position for intuitive ordering
    return callouts.sort((a, b) => a.nodeY - b.nodeY);
  }, [nodes, edges, highlightedNodeIds, visibilityMap]);

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

  // Build chain data for expanded node
  const chainData = expandedNode ? (() => {
    const chain = buildChainTree(expandedNodeId!, nodes, edges);
    const sortByType = (a: DecisionNode, b: DecisionNode) => {
      const aPriority = NODE_TYPE_PRIORITY[a.node_type] ?? 99;
      const bPriority = NODE_TYPE_PRIORITY[b.node_type] ?? 99;
      return aPriority - bPriority;
    };
    // Filter by search if present
    const lowerSearch = chainSearch.toLowerCase().trim();
    const matchesSearch = (node: DecisionNode) => {
      if (!lowerSearch) return true;
      return (
        node.title.toLowerCase().includes(lowerSearch) ||
        node.node_type.toLowerCase().includes(lowerSearch) ||
        node.description?.toLowerCase().includes(lowerSearch) ||
        String(node.id).includes(lowerSearch)
      );
    };
    return {
      ancestors: [...chain.ancestors].filter(matchesSearch).sort(sortByType),
      descendants: [...chain.descendants].filter(matchesSearch).sort(sortByType),
      totalAncestors: chain.ancestors.length,
      totalDescendants: chain.descendants.length,
    };
  })() : null;

  // panelX is simply containerWidth - RIGHT_PANEL_WIDTH
  // since both nodeX/nodeY and the panel are in the same coordinate system (container-relative)
  const panelX = containerWidth - RIGHT_PANEL_WIDTH;

  // Always render wrapper to get ref for measuring, but only show content when dimensions are valid
  const dimensionsValid = containerWidth > 0 && containerHeight > 0;

  // Desktop: SVG for lines, HTML for scrollable cards panel
  return (
    <div
      ref={containerRef}
      style={{
        position: 'absolute',
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        pointerEvents: 'none',
      }}
    >
      {/* SVG Layer for connection lines only - only render when dimensions are valid */}
      {dimensionsValid && panelX > 0 && (
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
          const isHovered = hoveredNodeId === callout.node.id;
          // Line goes to the panel edge
          const lineEndX = panelX;
          const lineEndY = Math.min(
            Math.max(callout.nodeY, TOP_RESERVED + 40),
            containerHeight - 40
          );

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
            </g>
          );
        })}
      </svg>
      )}

      {/* HTML Panel for scrollable cards - always render so we can measure it */}
      {calloutsNeeded.length > 0 && (
        <div
          ref={panelRef}
          style={{
            position: 'absolute',
            top: TOP_RESERVED,
            right: 0,
            width: RIGHT_PANEL_WIDTH,
            bottom: 0,
            backgroundColor: 'rgba(246, 248, 250, 0.95)',
            borderLeft: '1px solid #d0d7de',
            zIndex: 41,
            display: 'flex',
            flexDirection: 'column',
            pointerEvents: 'auto',
          }}
        >
          {/* Header */}
          <div style={{
            padding: '10px 12px',
            borderBottom: '1px solid #d0d7de',
            backgroundColor: '#fff',
            flexShrink: 0,
          }}>
            <span style={{ fontSize: 12, fontWeight: 600, color: '#57606a' }}>
              {calloutsNeeded.length} match{calloutsNeeded.length !== 1 ? 'es' : ''} found
            </span>
          </div>

          {/* Scrollable cards area */}
          <div style={{
            flex: 1,
            overflowY: 'auto',
            padding: CARD_MARGIN,
          }}>
            {calloutsNeeded.map((callout) => (
              <ResultCard
                key={callout.node.id}
                callout={callout}
                isHovered={hoveredNodeId === callout.node.id}
                onHover={setHoveredNodeId}
                onExpand={setExpandedNodeId}
                onNavigate={handleCardClick}
                onOpenModal={handleOpenModal}
              />
            ))}
          </div>
        </div>
      )}

      {/* Chain Panel - outside SVG for proper DOM rendering */}
      {expandedNode && chainData && (
        <div
          style={{
            position: 'absolute',
            bottom: 16,
            left: 16,
            width: 380,
            height: '50%',
            minHeight: 300,
            maxHeight: 'calc(100vh - 160px)',
            backgroundColor: '#fff',
            borderRadius: 10,
            display: 'flex',
            flexDirection: 'column',
            boxShadow: '0 4px 20px rgba(0,0,0,0.15)',
            border: '1px solid #d0d7de',
            zIndex: 50,
            pointerEvents: 'auto',
            resize: 'vertical',
            overflow: 'hidden',
          }}
        >
          {/* Fixed Header */}
          <div style={{ padding: 16, borderBottom: '1px solid #e5e7eb', flexShrink: 0 }}>
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
                    cursor: 'pointer',
                  }}
                  onClick={() => handleOpenModal(expandedNode)}
                  title="Click to open modal"
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

            {/* Go to Node button - zooms to the node */}
            <button
              onClick={() => {
                if (onNavigateToNode) {
                  onNavigateToNode(expandedNode);
                }
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
                marginBottom: 12,
              }}
            >
              Go to Node
            </button>

            {/* Chain Search Input */}
            <input
              type="text"
              value={chainSearch}
              onChange={(e) => setChainSearch(e.target.value)}
              placeholder="Filter chain..."
              style={{
                width: '100%',
                padding: '6px 10px',
                fontSize: 12,
                border: '1px solid #d0d7de',
                borderRadius: 6,
                backgroundColor: '#f6f8fa',
                outline: 'none',
                boxSizing: 'border-box',
              }}
            />
          </div>

          {/* Scrollable Content */}
          <div style={{ flex: 1, overflowY: 'auto', padding: 16 }}>
            {/* Ancestors Section */}
            <div style={{ marginBottom: 16 }}>
              <div style={{ fontSize: 11, color: '#6e7781', fontWeight: 600, marginBottom: 6 }}>
                UPSTREAM {chainSearch ? `(${chainData.ancestors.length}/${chainData.totalAncestors})` : `(${chainData.ancestors.length})`}
              </div>
              {chainData.ancestors.length === 0 ? (
                <div style={{ color: '#9ca3af', fontSize: 12, fontStyle: 'italic' }}>
                  {chainSearch && chainData.totalAncestors > 0 ? 'No matches' : 'Root node'}
                </div>
              ) : (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                  {chainData.ancestors.slice(0, chainSearch ? 15 : 5).map((node) => (
                    <div
                      key={node.id}
                      onClick={() => {
                        if (onNavigateToNode) {
                          onNavigateToNode(node);
                        }
                        setExpandedNodeId(node.id);
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
                          cursor: 'pointer',
                        }}
                        onClick={(e) => {
                          e.stopPropagation();
                          handleOpenModal(node);
                        }}
                        title="Click to open modal"
                      >
                        {node.node_type.slice(0, 3).toUpperCase()}
                      </span>
                      <span style={{ color: '#24292f', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                        {truncate(node.title, 30)}
                      </span>
                    </div>
                  ))}
                  {chainData.ancestors.length > (chainSearch ? 15 : 5) && (
                    <div style={{ fontSize: 11, color: '#6e7781' }}>+{chainData.ancestors.length - (chainSearch ? 15 : 5)} more</div>
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
                DOWNSTREAM {chainSearch ? `(${chainData.descendants.length}/${chainData.totalDescendants})` : `(${chainData.descendants.length})`}
              </div>
              {chainData.descendants.length === 0 ? (
                <div style={{ color: '#9ca3af', fontSize: 12, fontStyle: 'italic' }}>
                  {chainSearch && chainData.totalDescendants > 0 ? 'No matches' : 'Leaf node'}
                </div>
              ) : (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                  {chainData.descendants.slice(0, chainSearch ? 15 : 5).map((node) => (
                    <div
                      key={node.id}
                      onClick={() => {
                        // Zoom to node and update panel to show its info
                        if (onNavigateToNode) {
                          onNavigateToNode(node);
                        }
                        setExpandedNodeId(node.id);
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
                          cursor: 'pointer',
                        }}
                        onClick={(e) => {
                          e.stopPropagation();
                          handleOpenModal(node);
                        }}
                        title="Click to open modal"
                      >
                        {node.node_type.slice(0, 3).toUpperCase()}
                      </span>
                      <span style={{ color: '#24292f', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                        {truncate(node.title, 30)}
                      </span>
                    </div>
                  ))}
                  {chainData.descendants.length > (chainSearch ? 15 : 5) && (
                    <div style={{ fontSize: 11, color: '#6e7781' }}>+{chainData.descendants.length - (chainSearch ? 15 : 5)} more</div>
                  )}
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default CalloutLines;
