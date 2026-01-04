/**
 * Graph View
 *
 * Port of docs/spelunk-graph.html - D3.js force-directed graph.
 * Preserves the exact logic from the vanilla JS implementation.
 */

import React, { useRef, useEffect, useState, useCallback } from 'react';
import * as d3 from 'd3';
import type { DecisionNode, GraphData } from '../types/graph';
import { getConfidence, getCommit, truncate } from '../types/graph';
import { tracePath } from '../utils/graphProcessing';
import { TypeBadge, ConfidenceBadge, CommitBadge, EdgeBadge } from '../components/NodeBadge';
import { TypeFilters, FilterValue } from '../components/TypeFilters';
import { NODE_COLORS, getNodeColor } from '../utils/colors';

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
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [selectedNode, setSelectedNode] = useState<DecisionNode | null>(null);
  const [filter, setFilter] = useState<FilterValue>('all');
  const [searchTerm, setSearchTerm] = useState('');
  const simulationRef = useRef<d3.Simulation<SimNode, SimLink> | null>(null);

  // Handle node selection
  const handleSelectNode = useCallback((node: DecisionNode) => {
    setSelectedNode(node);
  }, []);

  const handleSelectNodeById = useCallback((id: number) => {
    const node = graphData.nodes.find(n => n.id === id);
    if (node) setSelectedNode(node);
  }, [graphData.nodes]);

  const handleCloseDetail = useCallback(() => {
    setSelectedNode(null);
  }, []);

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

  // Get path to root for selected node
  const pathToRoot = selectedNode ? tracePath(selectedNode.id, graphData) : [];

  return (
    <div style={styles.container}>
      {/* Controls */}
      <div style={styles.controls}>
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

      {/* SVG Container */}
      <div ref={containerRef} style={styles.svgContainer}>
        <svg ref={svgRef} style={styles.svg} />
      </div>

      {/* Detail Panel */}
      {selectedNode && (
        <div style={styles.detailPanel}>
          <button onClick={handleCloseDetail} style={styles.closeBtn}>×</button>

          <div style={styles.detailHeader}>
            <TypeBadge type={selectedNode.node_type} />
            <ConfidenceBadge confidence={getConfidence(selectedNode)} />
            <CommitBadge commit={getCommit(selectedNode)} />
          </div>

          <h3 style={styles.detailTitle}>{selectedNode.title}</h3>
          <p style={styles.detailMeta}>
            Node #{selectedNode.id} · {new Date(selectedNode.created_at).toLocaleString()}
          </p>

          {selectedNode.description && (
            <div style={styles.detailSection}>
              <h4 style={styles.sectionTitle}>Description</h4>
              <p style={styles.description}>{selectedNode.description}</p>
            </div>
          )}

          {pathToRoot.length > 1 && (
            <div style={styles.detailSection}>
              <h4 style={styles.sectionTitle}>Path to Root</h4>
              {pathToRoot.map(n => (
                <div
                  key={n.id}
                  onClick={() => handleSelectNodeById(n.id)}
                  style={styles.pathNode}
                >
                  <TypeBadge type={n.node_type} size="sm" />
                  <span>{truncate(n.title, 35)}</span>
                </div>
              ))}
            </div>
          )}

          {/* Connections */}
          <ConnectionsList
            node={selectedNode}
            graphData={graphData}
            onSelectNode={handleSelectNodeById}
          />
        </div>
      )}
    </div>
  );
};

// =============================================================================
// Connections List
// =============================================================================

interface ConnectionsListProps {
  node: DecisionNode;
  graphData: GraphData;
  onSelectNode: (id: number) => void;
}

const ConnectionsList: React.FC<ConnectionsListProps> = ({ node, graphData, onSelectNode }) => {
  const incoming = graphData.edges.filter(e => e.to_node_id === node.id);
  const outgoing = graphData.edges.filter(e => e.from_node_id === node.id);

  const getNode = (id: number) => graphData.nodes.find(n => n.id === id);

  return (
    <>
      {incoming.length > 0 && (
        <div style={styles.detailSection}>
          <h4 style={styles.sectionTitle}>Incoming ({incoming.length})</h4>
          {incoming.map(e => {
            const n = getNode(e.from_node_id);
            return (
              <div key={e.id} onClick={() => onSelectNode(e.from_node_id)} style={styles.connection}>
                <TypeBadge type={n?.node_type || 'observation'} size="sm" />
                <span>{truncate(n?.title || 'Unknown', 30)}</span>
              </div>
            );
          })}
        </div>
      )}

      {outgoing.length > 0 && (
        <div style={styles.detailSection}>
          <h4 style={styles.sectionTitle}>Outgoing ({outgoing.length})</h4>
          {outgoing.map(e => {
            const n = getNode(e.to_node_id);
            return (
              <div key={e.id} onClick={() => onSelectNode(e.to_node_id)} style={styles.connection}>
                <EdgeBadge type={e.edge_type} />
                <TypeBadge type={n?.node_type || 'observation'} size="sm" />
                <span>{truncate(n?.title || 'Unknown', 25)}</span>
                {e.rationale && <span style={styles.rationale}>{e.rationale}</span>}
              </div>
            );
          })}
        </div>
      )}
    </>
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
  controls: {
    position: 'absolute',
    top: '20px',
    left: '20px',
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    padding: '15px',
    borderRadius: '8px',
    zIndex: 10,
    maxWidth: '250px',
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
  },
  svg: {
    width: '100%',
    height: '100%',
  },
  detailPanel: {
    position: 'absolute',
    top: '20px',
    right: '20px',
    bottom: '20px',
    width: '350px',
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderRadius: '8px',
    padding: '20px',
    overflowY: 'auto',
    zIndex: 10,
  },
  closeBtn: {
    position: 'absolute',
    top: '10px',
    right: '10px',
    width: '28px',
    height: '28px',
    border: 'none',
    background: '#f6f8fa',
    color: '#57606a',
    borderRadius: '4px',
    fontSize: '18px',
    cursor: 'pointer',
  },
  detailHeader: {
    display: 'flex',
    gap: '8px',
    marginBottom: '10px',
    flexWrap: 'wrap',
  },
  detailTitle: {
    fontSize: '16px',
    margin: '0 0 8px 0',
    color: '#24292f',
  },
  detailMeta: {
    fontSize: '12px',
    color: '#6e7781',
    margin: 0,
  },
  detailSection: {
    marginTop: '20px',
  },
  sectionTitle: {
    fontSize: '12px',
    color: '#57606a',
    margin: '0 0 10px 0',
    textTransform: 'uppercase',
  },
  description: {
    fontSize: '13px',
    color: '#57606a',
    lineHeight: 1.5,
    margin: 0,
  },
  pathNode: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '8px',
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    borderRadius: '4px',
    marginBottom: '6px',
    cursor: 'pointer',
    fontSize: '12px',
    color: '#24292f',
  },
  connection: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '8px',
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    borderRadius: '4px',
    marginBottom: '6px',
    cursor: 'pointer',
    fontSize: '12px',
    color: '#24292f',
  },
  rationale: {
    color: '#6e7781',
    fontSize: '11px',
    marginLeft: 'auto',
  },
};
