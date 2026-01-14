/**
 * NarrativeGraph Component
 *
 * DAG visualization for a single narrative using D3.js + Dagre.
 * Shows nodes color-coded by type with edges showing relationships.
 *
 * Performance optimizations:
 * - Debounced resize handler
 * - Memoized layout calculation
 * - Node limit with warning for very large graphs
 */

import React, { useRef, useEffect, useCallback, useState, useMemo } from 'react';
import * as d3 from 'd3';
import dagre from 'dagre';
import type { DecisionNode, DecisionEdge } from '../types/graph';
import { getNodeColor } from '../utils/colors';
import { truncate } from '../types/graph';

interface NarrativeGraphProps {
  nodes: DecisionNode[];
  edges: DecisionEdge[];
  selectedNodeId: number | null;
  onNodeSelect: (nodeId: number) => void;
  onNodeHover?: (nodeId: number | null) => void;
}

// Node dimensions
const NODE_WIDTH = 180;
const NODE_HEIGHT = 60;
const NODE_MARGIN = 40;

// Performance limits
const SOFT_NODE_LIMIT = 100; // Show warning
const HARD_NODE_LIMIT = 500; // Force truncation

// Debounce helper
function debounce<T extends (...args: unknown[]) => void>(fn: T, delay: number): T {
  let timeoutId: ReturnType<typeof setTimeout>;
  return ((...args: Parameters<T>) => {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), delay);
  }) as T;
}

interface DagreNodeData {
  width: number;
  height: number;
  x: number;
  y: number;
  node: DecisionNode;
}

interface DagreEdgeData {
  points: { x: number; y: number }[];
  edge: DecisionEdge;
}

export const NarrativeGraph: React.FC<NarrativeGraphProps> = ({
  nodes,
  edges,
  selectedNodeId,
  onNodeSelect,
  onNodeHover,
}) => {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [zoom, setZoom] = useState(1);
  const [showAllNodes, setShowAllNodes] = useState(false);

  // Handle large node counts
  const { displayNodes, displayEdges, isTruncated, totalCount } = useMemo(() => {
    const total = nodes.length;

    // If under soft limit or user chose to show all, use all nodes
    if (total <= SOFT_NODE_LIMIT || showAllNodes) {
      // Hard limit still applies
      if (total > HARD_NODE_LIMIT) {
        const truncatedNodes = nodes.slice(0, HARD_NODE_LIMIT);
        const nodeIds = new Set(truncatedNodes.map(n => n.id));
        const truncatedEdges = edges.filter(
          e => nodeIds.has(e.from_node_id) && nodeIds.has(e.to_node_id)
        );
        return {
          displayNodes: truncatedNodes,
          displayEdges: truncatedEdges,
          isTruncated: true,
          totalCount: total,
        };
      }
      return {
        displayNodes: nodes,
        displayEdges: edges,
        isTruncated: false,
        totalCount: total,
      };
    }

    // Truncate to soft limit
    const truncatedNodes = nodes.slice(0, SOFT_NODE_LIMIT);
    const nodeIds = new Set(truncatedNodes.map(n => n.id));
    const truncatedEdges = edges.filter(
      e => nodeIds.has(e.from_node_id) && nodeIds.has(e.to_node_id)
    );
    return {
      displayNodes: truncatedNodes,
      displayEdges: truncatedEdges,
      isTruncated: true,
      totalCount: total,
    };
  }, [nodes, edges, showAllNodes]);

  // Calculate dagre layout - memoized for performance
  const layout = useMemo(() => {
    if (displayNodes.length === 0) return null;

    const g = new dagre.graphlib.Graph();
    g.setGraph({
      rankdir: 'TB',
      nodesep: NODE_MARGIN,
      ranksep: NODE_MARGIN * 1.5,
      marginx: 40,
      marginy: 40,
    });
    g.setDefaultEdgeLabel(() => ({}));

    // Add nodes
    const nodeMap = new Map(displayNodes.map(n => [n.id, n]));
    for (const node of displayNodes) {
      g.setNode(String(node.id), {
        width: NODE_WIDTH,
        height: NODE_HEIGHT,
        node,
      });
    }

    // Add edges
    for (const edge of displayEdges) {
      if (nodeMap.has(edge.from_node_id) && nodeMap.has(edge.to_node_id)) {
        g.setEdge(String(edge.from_node_id), String(edge.to_node_id), { edge });
      }
    }

    // Calculate layout
    dagre.layout(g);

    return {
      graph: g,
      width: (g.graph().width ?? 400) + 80,
      height: (g.graph().height ?? 300) + 80,
    };
  }, [displayNodes, displayEdges]);

  // Build and render the graph
  const renderGraph = useCallback(() => {
    const svg = d3.select(svgRef.current);
    const container = containerRef.current;
    if (!svg || !container) return;

    // Clear previous render
    svg.selectAll('*').remove();

    if (!layout || displayNodes.length === 0) {
      // Show empty state
      svg.append('text')
        .attr('x', container.clientWidth / 2)
        .attr('y', container.clientHeight / 2)
        .attr('text-anchor', 'middle')
        .attr('fill', '#8c959f')
        .text('Select a narrative to view its graph');
      return;
    }

    // Use memoized layout
    const { graph: g, width: graphWidth, height: graphHeight } = layout;

    // Set SVG size
    svg.attr('width', container.clientWidth)
       .attr('height', container.clientHeight);

    // Create main group with zoom/pan
    const mainGroup = svg.append('g').attr('class', 'main-group');

    // Setup zoom behavior
    const zoomBehavior = d3.zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.2, 3])
      .on('zoom', (event) => {
        mainGroup.attr('transform', event.transform);
        setZoom(event.transform.k);
      });

    // Apply zoom to svg (with null check cast)
    (svg as d3.Selection<SVGSVGElement, unknown, null, undefined>).call(zoomBehavior);

    // Initial fit
    const scale = Math.min(
      container.clientWidth / graphWidth,
      container.clientHeight / graphHeight,
      1.2
    );
    const initialX = (container.clientWidth - graphWidth * scale) / 2;
    const initialY = (container.clientHeight - graphHeight * scale) / 2 + 20;

    (svg as d3.Selection<SVGSVGElement, unknown, null, undefined>).call(
      zoomBehavior.transform,
      d3.zoomIdentity.translate(initialX, initialY).scale(scale)
    );

    // Draw edges
    const edgesGroup = mainGroup.append('g').attr('class', 'edges');

    g.edges().forEach(e => {
      const edgeData = g.edge(e) as DagreEdgeData;
      if (!edgeData?.points) return;

      const line = d3.line<{ x: number; y: number }>()
        .x(d => d.x)
        .y(d => d.y)
        .curve(d3.curveBasis);

      edgesGroup.append('path')
        .attr('d', line(edgeData.points))
        .attr('fill', 'none')
        .attr('stroke', '#b1bac4')
        .attr('stroke-width', 2)
        .attr('marker-end', 'url(#arrowhead)');
    });

    // Add arrowhead marker
    svg.append('defs')
      .append('marker')
      .attr('id', 'arrowhead')
      .attr('viewBox', '0 -5 10 10')
      .attr('refX', 8)
      .attr('refY', 0)
      .attr('markerWidth', 6)
      .attr('markerHeight', 6)
      .attr('orient', 'auto')
      .append('path')
      .attr('d', 'M0,-5L10,0L0,5')
      .attr('fill', '#b1bac4');

    // Draw nodes
    const nodesGroup = mainGroup.append('g').attr('class', 'nodes');

    g.nodes().forEach(nodeId => {
      const nodeData = g.node(nodeId) as DagreNodeData;
      if (!nodeData?.node) return;

      const { x, y, node } = nodeData;
      const isSelected = node.id === selectedNodeId;
      const color = getNodeColor(node.node_type);

      const nodeGroup = nodesGroup.append('g')
        .attr('class', 'node')
        .attr('transform', `translate(${x - NODE_WIDTH / 2}, ${y - NODE_HEIGHT / 2})`)
        .style('cursor', 'pointer')
        .on('click', () => onNodeSelect(node.id))
        .on('mouseenter', () => onNodeHover?.(node.id))
        .on('mouseleave', () => onNodeHover?.(null));

      // Node background
      nodeGroup.append('rect')
        .attr('width', NODE_WIDTH)
        .attr('height', NODE_HEIGHT)
        .attr('rx', 8)
        .attr('fill', '#ffffff')
        .attr('stroke', isSelected ? '#0969da' : color)
        .attr('stroke-width', isSelected ? 3 : 2)
        .attr('filter', isSelected ? 'drop-shadow(0 4px 8px rgba(0,0,0,0.2))' : 'none');

      // Node type badge
      nodeGroup.append('rect')
        .attr('x', 8)
        .attr('y', 8)
        .attr('width', 50)
        .attr('height', 16)
        .attr('rx', 3)
        .attr('fill', color + '22');

      nodeGroup.append('text')
        .attr('x', 33)
        .attr('y', 19)
        .attr('text-anchor', 'middle')
        .attr('font-size', '9px')
        .attr('font-weight', 600)
        .attr('fill', color)
        .text(node.node_type.toUpperCase().slice(0, 6));

      // Node ID
      nodeGroup.append('text')
        .attr('x', NODE_WIDTH - 8)
        .attr('y', 19)
        .attr('text-anchor', 'end')
        .attr('font-size', '10px')
        .attr('fill', '#8c959f')
        .text(`#${node.id}`);

      // Node title
      nodeGroup.append('text')
        .attr('x', 8)
        .attr('y', 40)
        .attr('font-size', '12px')
        .attr('font-weight', 500)
        .attr('fill', '#24292f')
        .text(truncate(node.title, 25));

      // Second line if needed
      if (node.title.length > 25) {
        nodeGroup.append('text')
          .attr('x', 8)
          .attr('y', 52)
          .attr('font-size', '11px')
          .attr('fill', '#57606a')
          .text(truncate(node.title.slice(25), 22));
      }
    });

  }, [layout, displayNodes, selectedNodeId, onNodeSelect, onNodeHover]);

  // Re-render on data changes
  useEffect(() => {
    renderGraph();
  }, [renderGraph]);

  // Handle resize with debouncing
  useEffect(() => {
    const debouncedResize = debounce(() => renderGraph(), 150);
    window.addEventListener('resize', debouncedResize);
    return () => window.removeEventListener('resize', debouncedResize);
  }, [renderGraph]);

  return (
    <div ref={containerRef} style={styles.container}>
      <svg ref={svgRef} style={styles.svg} />

      {/* Truncation warning */}
      {isTruncated && (
        <div style={styles.truncationWarning}>
          <span>
            Showing {displayNodes.length} of {totalCount} nodes
          </span>
          {totalCount <= HARD_NODE_LIMIT && !showAllNodes && (
            <button
              style={styles.showAllButton}
              onClick={() => setShowAllNodes(true)}
            >
              Show all
            </button>
          )}
          {totalCount > HARD_NODE_LIMIT && (
            <span style={styles.hardLimitNote}>
              (max {HARD_NODE_LIMIT})
            </span>
          )}
        </div>
      )}

      {/* Zoom indicator */}
      <div style={styles.zoomIndicator}>
        {Math.round(zoom * 100)}%
      </div>
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    flex: 1,
    position: 'relative',
    backgroundColor: '#fafbfc',
    overflow: 'hidden',
    minHeight: 0, // Critical for flex child to shrink properly
  },
  svg: {
    width: '100%',
    height: '100%',
    display: 'block',
  },
  zoomIndicator: {
    position: 'absolute',
    bottom: '12px',
    left: '12px',
    backgroundColor: 'rgba(255, 255, 255, 0.9)',
    padding: '4px 8px',
    borderRadius: '4px',
    fontSize: '11px',
    color: '#57606a',
    border: '1px solid #e1e4e8',
  },
  truncationWarning: {
    position: 'absolute',
    top: '12px',
    left: '50%',
    transform: 'translateX(-50%)',
    backgroundColor: '#fff8c5',
    border: '1px solid #d4a72c',
    borderRadius: '6px',
    padding: '8px 16px',
    fontSize: '12px',
    color: '#6f5e02',
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    zIndex: 10,
  },
  showAllButton: {
    padding: '4px 10px',
    fontSize: '11px',
    fontWeight: 500,
    backgroundColor: '#ffffff',
    color: '#6f5e02',
    border: '1px solid #d4a72c',
    borderRadius: '4px',
    cursor: 'pointer',
  },
  hardLimitNote: {
    fontSize: '11px',
    color: '#8a7e3b',
  },
};
