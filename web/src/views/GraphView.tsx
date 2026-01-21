/**
 * Graph View
 *
 * Port of docs/spelunk-graph.html - D3.js force-directed graph.
 * Preserves the exact logic from the vanilla JS implementation.
 */

import React, { useRef, useEffect, useState, useCallback, useMemo } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import * as d3 from 'd3';
import type { DecisionNode, GraphData } from '../types/graph';
import { getConfidence, truncate } from '../types/graph';
import { TypeFilters, FilterValue } from '../components/TypeFilters';
import { NODE_COLORS, getNodeColor } from '../utils/colors';
import { CardStack } from '../components/CardStack';

interface GraphViewProps {
  graphData: GraphData;
}

// D3 simulation node type
interface SimNode extends DecisionNode {
  x?: number;
  y?: number;
  fx?: number | null;
  fy?: number | null;
}

// D3 simulation link type
interface SimLink {
  source: SimNode;
  target: SimNode;
  type: string;
  rationale: string | null;
}

export const GraphView: React.FC<GraphViewProps> = ({ graphData }) => {
  // URL-based node selection
  const { nodeId: urlNodeId } = useParams<{ nodeId?: string }>();
  const navigate = useNavigate();

  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [filter, setFilter] = useState<FilterValue>('all');
  const [searchTerm, setSearchTerm] = useState('');
  const simulationRef = useRef<d3.Simulation<SimNode, SimLink> | null>(null);

  // Stack-based node selection (like archaeology view)
  const [nodeStack, setNodeStack] = useState<DecisionNode[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [expandedIndex, setExpandedIndex] = useState<number | null>(null);
  const [showCardStack, setShowCardStack] = useState(false);

  // Node map for quick lookups
  const nodeMap = useMemo(() => new Map(graphData.nodes.map(n => [n.id, n])), [graphData.nodes]);

  // Current selected node (first in stack)
  const selectedNode = nodeStack.length > 0 ? nodeStack[selectedIndex] : null;

  // Sync URL param with state - initialize stack with the URL node
  useEffect(() => {
    if (urlNodeId && graphData.nodes.length > 0) {
      const nodeId = parseInt(urlNodeId, 10);
      const node = graphData.nodes.find(n => n.id === nodeId);
      if (node && (nodeStack.length === 0 || nodeStack[0].id !== nodeId)) {
        setNodeStack([node]);
        setSelectedIndex(0);
        setExpandedIndex(0);
        setShowCardStack(true);
      }
    }
  }, [urlNodeId, graphData.nodes]);

  // Update URL when selection changes
  useEffect(() => {
    if (selectedNode) {
      navigate(`/graph/${selectedNode.id}`, { replace: true });
    } else if (nodeStack.length === 0 && urlNodeId) {
      navigate('/graph', { replace: true });
    }
  }, [selectedNode, nodeStack.length, urlNodeId, navigate]);

  // Handle node selection from graph - starts a new stack
  const handleSelectNode = useCallback((node: DecisionNode) => {
    setNodeStack([node]);
    setSelectedIndex(0);
    setExpandedIndex(0);
    setShowCardStack(true);
  }, []);

  // Handle clicking on parent/child in CardStack - adds to stack
  const handleNodeClick = useCallback((nodeId: number) => {
    const node = nodeMap.get(nodeId);
    if (!node) return;

    // Check if node is already in stack
    const existingIndex = nodeStack.findIndex(n => n.id === nodeId);
    if (existingIndex >= 0) {
      // Just select it
      setSelectedIndex(existingIndex);
      setExpandedIndex(existingIndex);
    } else {
      // Add to stack after current selection
      const newStack = [...nodeStack.slice(0, selectedIndex + 1), node];
      setNodeStack(newStack);
      setSelectedIndex(newStack.length - 1);
      setExpandedIndex(newStack.length - 1);
    }
  }, [nodeMap, nodeStack, selectedIndex]);

  const handleCloseCardStack = useCallback(() => {
    setShowCardStack(false);
    setNodeStack([]);
    setSelectedIndex(0);
    setExpandedIndex(null);
    navigate('/graph', { replace: true });
  }, [navigate]);

  // Keyboard navigation for card stack
  useEffect(() => {
    if (!showCardStack || nodeStack.length === 0) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      switch (e.key) {
        case 'j':
        case 'ArrowDown':
          e.preventDefault();
          setSelectedIndex(prev => Math.min(prev + 1, nodeStack.length - 1));
          break;
        case 'k':
        case 'ArrowUp':
          e.preventDefault();
          setSelectedIndex(prev => Math.max(prev - 1, 0));
          break;
        case ' ':
        case 'Enter':
          e.preventDefault();
          setExpandedIndex(prev => prev === selectedIndex ? null : selectedIndex);
          break;
        case 'q':
        case 'Escape':
          e.preventDefault();
          handleCloseCardStack();
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [showCardStack, nodeStack.length, selectedIndex, handleCloseCardStack]);

  // Initialize D3 graph
  useEffect(() => {
    if (!svgRef.current || !containerRef.current) return;

    const svg = d3.select(svgRef.current);
    const container = containerRef.current;
    const width = container.clientWidth;
    const height = container.clientHeight;

    // Clear previous content
    svg.selectAll('*').remove();

    // Create container for zoom
    const g = svg.append('g');

    // Add zoom behavior
    const zoom = d3.zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.1, 4])
      .on('zoom', (event) => g.attr('transform', event.transform));
    svg.call(zoom);

    // Create nodes and links for simulation
    const nodes: SimNode[] = graphData.nodes.map(n => ({ ...n }));
    const nodeMap = new Map(nodes.map(n => [n.id, n]));

    const links: SimLink[] = graphData.edges
      .map(e => ({
        source: nodeMap.get(e.from_node_id)!,
        target: nodeMap.get(e.to_node_id)!,
        type: e.edge_type,
        rationale: e.rationale,
      }))
      .filter(l => l.source && l.target);

    // Create simulation
    const simulation = d3.forceSimulation<SimNode>(nodes)
      .force('link', d3.forceLink<SimNode, SimLink>(links)
        .id(d => d.id)
        .distance(80))
      .force('charge', d3.forceManyBody().strength(-200))
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collision', d3.forceCollide().radius(30));

    simulationRef.current = simulation;

    // Draw links
    const link = g.append('g')
      .selectAll<SVGLineElement, SimLink>('line')
      .data(links)
      .join('line')
      .attr('class', 'link')
      .attr('stroke', d => d.type === 'chosen' ? '#22c55e' : d.type === 'rejected' ? '#ef4444' : '#3b82f6')
      .attr('stroke-width', 1.5)
      .attr('stroke-opacity', 0.6)
      .attr('stroke-dasharray', d => d.type === 'rejected' ? '5,5' : null);

    // Draw nodes
    const node = g.append('g')
      .selectAll<SVGGElement, SimNode>('.node')
      .data(nodes)
      .join('g')
      .attr('class', 'node')
      .style('cursor', 'pointer')
      .call(d3.drag<SVGGElement, SimNode>()
        .on('start', (event, d) => {
          if (!event.active) simulation.alphaTarget(0.3).restart();
          d.fx = d.x;
          d.fy = d.y;
        })
        .on('drag', (event, d) => {
          d.fx = event.x;
          d.fy = event.y;
        })
        .on('end', (event, d) => {
          if (!event.active) simulation.alphaTarget(0);
          d.fx = null;
          d.fy = null;
        }));

    // Node circles
    node.append('circle')
      .attr('r', d => {
        if (d.node_type === 'goal') return 18;
        if (d.node_type === 'decision') return 15;
        return 12;
      })
      .attr('fill', d => getNodeColor(d.node_type))
      .attr('stroke', '#fff')
      .attr('stroke-width', 2);

    // Labels for larger nodes
    node.filter(d => d.node_type === 'goal' || d.node_type === 'decision')
      .append('text')
      .attr('dy', 30)
      .attr('text-anchor', 'middle')
      .attr('fill', '#57606a')
      .attr('font-size', '10px')
      .text(d => truncate(d.title, 20));

    // Click handler
    node.on('click', (_event, d) => {
      handleSelectNode(d);
    });

    // Tooltip
    node.append('title')
      .text(d => {
        const conf = getConfidence(d);
        return `${d.title}\n${d.node_type}${conf !== null ? ` · ${conf}%` : ''}`;
      });

    // Update positions on tick
    simulation.on('tick', () => {
      link
        .attr('x1', d => d.source.x!)
        .attr('y1', d => d.source.y!)
        .attr('x2', d => d.target.x!)
        .attr('y2', d => d.target.y!);

      node.attr('transform', d => `translate(${d.x},${d.y})`);
    });

    // Cleanup
    return () => {
      simulation.stop();
    };
  }, [graphData, handleSelectNode]);

  // Apply filter and search
  useEffect(() => {
    if (!svgRef.current) return;

    const svg = d3.select(svgRef.current);
    const searchLower = searchTerm.toLowerCase();

    svg.selectAll<SVGGElement, SimNode>('.node').style('opacity', d => {
      // Search filter
      if (searchTerm) {
        const match = d.title.toLowerCase().includes(searchLower) ||
          (d.description?.toLowerCase().includes(searchLower) ?? false);
        if (!match) return 0.15;
      }

      // Type filter
      if (filter !== 'all' && d.node_type !== filter) {
        return 0.15;
      }

      return 1;
    });
  }, [filter, searchTerm]);

  // Highlight selected node connections
  useEffect(() => {
    if (!svgRef.current) return;

    const svg = d3.select(svgRef.current);

    svg.selectAll<SVGGElement, SimNode>('.node')
      .classed('selected', d => d.id === selectedNode?.id);

    svg.selectAll<SVGLineElement, SimLink>('.link')
      .attr('stroke-width', d =>
        d.source.id === selectedNode?.id || d.target.id === selectedNode?.id ? 3 : 1.5
      )
      .attr('stroke-opacity', d =>
        d.source.id === selectedNode?.id || d.target.id === selectedNode?.id ? 1 : 0.6
      );
  }, [selectedNode]);

  return (
    <div style={styles.container}>
      {/* Controls - far left */}
      <div style={styles.controlsSidebar}>
        <h2 style={styles.title}>Graph Explorer</h2>
        <TypeFilters value={filter} onChange={setFilter} />
        <input
          type="text"
          placeholder="Search nodes..."
          value={searchTerm}
          onChange={e => setSearchTerm(e.target.value)}
          style={styles.search}
        />
        <div style={styles.legend}>
          {Object.entries(NODE_COLORS).map(([type, color]) => (
            <div key={type} style={styles.legendItem}>
              <div style={{ ...styles.legendDot, backgroundColor: color }} />
              <span>{type.charAt(0).toUpperCase() + type.slice(1)}</span>
            </div>
          ))}
        </div>
      </div>

      {/* SVG Container - fills remaining space */}
      <div ref={containerRef} style={styles.svgContainer}>
        <svg ref={svgRef} style={styles.svg} />
      </div>

      {/* CardStack - far right, shown when node selected */}
      {showCardStack && nodeStack.length > 0 && (
        <CardStack
          nodes={nodeStack}
          edges={graphData.edges}
          selectedIndex={selectedIndex}
          expandedIndex={expandedIndex}
          onSelectIndex={setSelectedIndex}
          onExpandIndex={setExpandedIndex}
          onNodeClick={handleNodeClick}
          onClose={handleCloseCardStack}
          allNodes={graphData.nodes}
        />
      )}
    </div>
  );
};

// =============================================================================
// Styles
// =============================================================================

const styles: Record<string, React.CSSProperties> = {
  container: {
    height: '100%',
    display: 'flex',
    position: 'relative',
    backgroundColor: '#ffffff',
  },
  controlsSidebar: {
    width: '220px',
    flexShrink: 0,
    backgroundColor: '#f6f8fa',
    borderRight: '1px solid #d0d7de',
    padding: '15px',
    overflowY: 'auto',
  },
  title: {
    fontSize: '16px',
    margin: '0 0 12px 0',
    color: '#24292f',
  },
  search: {
    width: '100%',
    padding: '8px 12px',
    marginTop: '12px',
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderRadius: '4px',
    color: '#24292f',
    fontSize: '13px',
    boxSizing: 'border-box',
  },
  legend: {
    marginTop: '15px',
    display: 'flex',
    flexDirection: 'column',
    gap: '6px',
  },
  legendItem: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    fontSize: '11px',
    color: '#57606a',
  },
  legendDot: {
    width: '10px',
    height: '10px',
    borderRadius: '50%',
  },
  svgContainer: {
    flex: 1,
    height: '100%',
    position: 'relative',
  },
  svg: {
    width: '100%',
    height: '100%',
  },
};
