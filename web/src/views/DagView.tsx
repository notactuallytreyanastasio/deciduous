/**
 * DAG View
 *
 * Port of docs/demo/visual-graph.html - Dagre hierarchical layout.
 * Uses D3.js + Dagre for organized DAG visualization.
 *
 * Default: Shows only the most recent goal chain for focus.
 * Use controls to expand and see more chains.
 */

import React, { useRef, useEffect, useState, useCallback, useMemo } from 'react';
import * as d3 from 'd3';
import dagre from 'dagre';
import type { DecisionNode, DecisionEdge, GraphData, Chain, GitCommit } from '../types/graph';
import { getConfidence, getCommit, truncate, shortCommit, githubCommitUrl } from '../types/graph';
import { TypeBadge, ConfidenceBadge, CommitBadge, EdgeBadge } from '../components/NodeBadge';
import { SearchBar } from '../components/SearchBar';
import { CalloutLines } from '../components/CalloutLines';
import { MiniMap } from '../components/MiniMap';
import { useNodeVisibility } from '../hooks/useNodeVisibility';
import { NODE_COLORS, getNodeColor, getEdgeColor } from '../utils/colors';

interface DagViewProps {
  graphData: GraphData;
  chains: Chain[];
  gitHistory?: GitCommit[];
}

// Look up commit info from gitHistory by hash
function getCommitInfo(hash: string | null, gitHistory: GitCommit[]): GitCommit | null {
  if (!hash || gitHistory.length === 0) return null;
  return gitHistory.find(c => c.hash === hash || c.short_hash === hash || c.hash.startsWith(hash)) ?? null;
}

// Dagre node data type
interface DagreNodeData {
  width: number;
  height: number;
  x: number;
  y: number;
  node: DecisionNode;
}

// Dagre edge data type
interface DagreEdgeData {
  points: { x: number; y: number }[];
  edge: DecisionEdge;
}

type ViewMode = 'recent' | 'all' | 'single';

// Default number of recent chains to show
const DEFAULT_RECENT_CHAINS = 8;

/**
 * Get the most recent update time for a chain (max of all node updated_at times)
 */
function getChainLastUpdated(chain: Chain): number {
  return Math.max(...chain.nodes.map(n => new Date(n.updated_at).getTime()));
}

/**
 * Sort chains by most recent activity (most recently updated nodes)
 */
function sortChainsByRecency(chains: Chain[]): Chain[] {
  return [...chains].sort((a, b) => getChainLastUpdated(b) - getChainLastUpdated(a));
}

export const DagView: React.FC<DagViewProps> = ({ graphData, chains, gitHistory = [] }) => {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [selectedNode, setSelectedNode] = useState<DecisionNode | null>(null);
  const [focusChainIndex, setFocusChainIndex] = useState<number | null>(null);
  const [zoom, setZoom] = useState(1);

  // New state for recency filtering
  const [viewMode, setViewMode] = useState<ViewMode>('recent');
  const [recentChainCount, setRecentChainCount] = useState(DEFAULT_RECENT_CHAINS);
  // Detect mobile for responsive defaults
  const isMobile = typeof window !== 'undefined' && window.innerWidth < 768;
  const [isFullscreen, setIsFullscreen] = useState(!isMobile); // Fullscreen by default on desktop only
  const [isControlsCollapsed, setIsControlsCollapsed] = useState(isMobile); // Collapsed by default on mobile

  // Search state - highlighted node IDs from search
  const [highlightedNodeIds, setHighlightedNodeIds] = useState<Set<number>>(new Set());

  // Track node positions for visibility detection and callouts
  const [nodePositions, setNodePositions] = useState<Map<number, { x: number; y: number; width: number; height: number }>>(new Map());
  const [transform, setTransform] = useState({ x: 0, y: 0 });
  const [containerDimensions, setContainerDimensions] = useState({ width: 0, height: 0 });

  // Graph bounds for minimap
  const [graphBounds, setGraphBounds] = useState({ minX: 0, maxX: 1000, minY: 0, maxY: 1000 });

  // Node visibility tracking
  const { visibilityMap } = useNodeVisibility(
    svgRef,
    nodePositions,
    zoom,
    transform
  );

  // Store zoom behavior for programmatic control
  const zoomBehaviorRef = useRef<d3.ZoomBehavior<SVGSVGElement, unknown> | null>(null);

  // Sort chains by recency for display
  const sortedChains = useMemo(() => sortChainsByRecency(chains), [chains]);

  // Get only goal chains (for the dropdown and recent filtering)
  const goalChains = useMemo(() =>
    sortedChains.filter(c => c.root.node_type === 'goal'),
    [sortedChains]
  );

  // Determine which chains to show based on view mode
  const visibleChains = useMemo(() => {
    if (viewMode === 'single' && focusChainIndex !== null) {
      return [chains[focusChainIndex]].filter(Boolean);
    }
    if (viewMode === 'recent') {
      return goalChains.slice(0, recentChainCount);
    }
    return sortedChains; // 'all' mode
  }, [viewMode, focusChainIndex, chains, goalChains, sortedChains, recentChainCount]);

  // Get all visible node IDs from visible chains
  const visibleNodeIds = useMemo(() => {
    const ids = new Set<number>();
    visibleChains.forEach(chain => {
      chain.nodes.forEach(n => ids.add(n.id));
    });
    return ids;
  }, [visibleChains]);

  // Calculate how many chains are hidden
  const hiddenChainCount = goalChains.length - (viewMode === 'recent' ? recentChainCount : 0);

  const handleSelectNode = useCallback((node: DecisionNode) => {
    setSelectedNode(node);
  }, []);

  const handleSelectNodeById = useCallback((id: number) => {
    const node = graphData.nodes.find(n => n.id === id);
    if (node) setSelectedNode(node);
  }, [graphData.nodes]);

  // State for custom expand input
  const [expandInputVisible, setExpandInputVisible] = useState(false);
  const [expandInputValue, setExpandInputValue] = useState('');

  const handleShowMore = useCallback((count: number = 1) => {
    setRecentChainCount(prev => Math.min(prev + count, goalChains.length));
    setExpandInputVisible(false);
    setExpandInputValue('');
  }, [goalChains.length]);

  const handleShowLess = useCallback((count: number = 1) => {
    setRecentChainCount(prev => Math.max(prev - count, 1));
  }, []);

  const handleExpandSubmit = useCallback(() => {
    const num = parseInt(expandInputValue, 10);
    if (num > 0) {
      handleShowMore(num);
    }
  }, [expandInputValue, handleShowMore]);

  const handleShowAll = useCallback(() => {
    setViewMode('all');
  }, []);

  const handleShowRecent = useCallback(() => {
    setViewMode('recent');
    setRecentChainCount(DEFAULT_RECENT_CHAINS);
    setFocusChainIndex(null);
  }, []);

  const toggleFullscreen = useCallback(() => {
    setIsFullscreen(prev => !prev);
  }, []);

  const toggleControls = useCallback(() => {
    setIsControlsCollapsed(prev => !prev);
  }, []);

  // Handle Escape key to exit fullscreen
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isFullscreen) {
        setIsFullscreen(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isFullscreen]);

  const handleFocusChain = useCallback((index: number | null) => {
    if (index === null) {
      setViewMode('recent');
      setFocusChainIndex(null);
    } else {
      setViewMode('single');
      setFocusChainIndex(index);
    }
  }, []);

  // Navigate to a specific node (pan/zoom to bring it into view)
  const handleNavigateToNode = useCallback((node: DecisionNode) => {
    if (!svgRef.current || !containerRef.current || !zoomBehaviorRef.current) return;

    const svg = d3.select(svgRef.current);
    const container = containerRef.current;
    const width = container.clientWidth;
    const height = container.clientHeight;

    const pos = nodePositions.get(node.id);
    if (!pos) return;

    // Calculate target transform to center the node
    const targetScale = 1.2; // Zoom in a bit
    const targetX = width / 2 - pos.x * targetScale;
    const targetY = height / 2 - pos.y * targetScale;

    // Animate to the node
    svg.transition()
      .duration(500)
      .call(
        zoomBehaviorRef.current.transform,
        d3.zoomIdentity.translate(targetX, targetY).scale(targetScale)
      );

    // Also open the node modal
    setSelectedNode(node);
  }, [nodePositions]);

  // Build and render DAG
  useEffect(() => {
    if (!svgRef.current || !containerRef.current) return;

    const svg = d3.select(svgRef.current);
    const container = containerRef.current;
    const width = container.clientWidth;
    const height = container.clientHeight;

    svg.selectAll('*').remove();

    // Filter nodes based on visibility
    const visibleNodes = graphData.nodes.filter(n => visibleNodeIds.has(n.id));
    const visibleEdges = graphData.edges.filter(
      e => visibleNodeIds.has(e.from_node_id) && visibleNodeIds.has(e.to_node_id)
    );

    if (visibleNodes.length === 0) return;

    // Create Dagre graph
    const g = new dagre.graphlib.Graph();
    g.setGraph({
      rankdir: 'TB',
      nodesep: 80,
      ranksep: 100,
      marginx: 50,
      marginy: 50,
    });
    g.setDefaultEdgeLabel(() => ({}));

    // Add nodes
    visibleNodes.forEach(node => {
      g.setNode(String(node.id), {
        width: 150,
        height: 60,
        node,
      });
    });

    // Add edges
    visibleEdges.forEach(edge => {
      g.setEdge(String(edge.from_node_id), String(edge.to_node_id), { edge });
    });

    // Run layout
    dagre.layout(g);

    // Store node positions for visibility tracking and callouts
    const newPositions = new Map<number, { x: number; y: number; width: number; height: number }>();
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    g.nodes().forEach(nodeId => {
      const nodeData = g.node(nodeId) as DagreNodeData;
      newPositions.set(parseInt(nodeId), {
        x: nodeData.x,
        y: nodeData.y,
        width: nodeData.width,
        height: nodeData.height,
      });
      // Track bounds
      minX = Math.min(minX, nodeData.x - nodeData.width / 2);
      maxX = Math.max(maxX, nodeData.x + nodeData.width / 2);
      minY = Math.min(minY, nodeData.y - nodeData.height / 2);
      maxY = Math.max(maxY, nodeData.y + nodeData.height / 2);
    });
    setNodePositions(newPositions);
    setGraphBounds({ minX, maxX, minY, maxY });
    setContainerDimensions({ width, height });

    // Get graph dimensions
    const graphWidth = g.graph().width || width;
    const graphHeight = g.graph().height || height;

    // Create main group first (before zoom behavior references it)
    const mainGroup = svg.append('g');

    // Create container group with zoom
    const zoomBehavior = d3.zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.1, 3])
      .on('zoom', (event) => {
        mainGroup.attr('transform', event.transform);
        setZoom(event.transform.k);
        setTransform({ x: event.transform.x, y: event.transform.y });
      });

    // Store zoom behavior ref for programmatic control
    zoomBehaviorRef.current = zoomBehavior;

    svg.call(zoomBehavior);

    // Center the graph initially
    const initialScale = Math.min(
      (width - 100) / graphWidth,
      (height - 100) / graphHeight,
      1
    );
    const tx = (width - graphWidth * initialScale) / 2;
    const ty = (height - graphHeight * initialScale) / 2;

    svg.call(
      zoomBehavior.transform,
      d3.zoomIdentity.translate(tx, ty).scale(initialScale)
    );

    // Draw edges
    const edges = mainGroup.append('g')
      .selectAll('.edge')
      .data(g.edges())
      .join('g')
      .attr('class', 'edge');

    edges.each(function (e) {
      const edge = g.edge(e) as DagreEdgeData;
      const edgeData = edge.edge;

      const line = d3.line<{ x: number; y: number }>()
        .x(d => d.x)
        .y(d => d.y)
        .curve(d3.curveBasis);

      d3.select(this)
        .append('path')
        .attr('d', line(edge.points))
        .attr('fill', 'none')
        .attr('stroke', getEdgeColor(edgeData.edge_type))
        .attr('stroke-width', 2)
        .attr('stroke-opacity', 0.6)
        .attr('stroke-dasharray', edgeData.edge_type === 'rejected' ? '5,5' : null)
        .attr('marker-end', 'url(#arrowhead)');
    });

    // Defs for markers and filters
    const defs = svg.append('defs');

    // Arrow marker
    defs.append('marker')
      .attr('id', 'arrowhead')
      .attr('viewBox', '-5 -5 10 10')
      .attr('refX', 8)
      .attr('refY', 0)
      .attr('markerWidth', 6)
      .attr('markerHeight', 6)
      .attr('orient', 'auto')
      .append('path')
      .attr('d', 'M-5,-5L5,0L-5,5Z')
      .attr('fill', '#666');

    // Glow filter for search highlights
    const glowFilter = defs.append('filter')
      .attr('id', 'search-glow')
      .attr('x', '-50%')
      .attr('y', '-50%')
      .attr('width', '200%')
      .attr('height', '200%');
    glowFilter.append('feGaussianBlur')
      .attr('stdDeviation', '4')
      .attr('result', 'coloredBlur');
    const feMerge = glowFilter.append('feMerge');
    feMerge.append('feMergeNode').attr('in', 'coloredBlur');
    feMerge.append('feMergeNode').attr('in', 'SourceGraphic');

    // Draw nodes
    const nodes = mainGroup.append('g')
      .selectAll('.node')
      .data(g.nodes())
      .join('g')
      .attr('class', 'node')
      .attr('transform', d => {
        const node = g.node(d) as DagreNodeData;
        return `translate(${node.x - node.width / 2},${node.y - node.height / 2})`;
      })
      .style('cursor', 'pointer')
      .on('click', (_event, d) => {
        const nodeData = (g.node(d) as DagreNodeData).node;
        handleSelectNode(nodeData);
      });

    // Node rectangles
    nodes.append('rect')
      .attr('width', d => (g.node(d) as DagreNodeData).width)
      .attr('height', d => (g.node(d) as DagreNodeData).height)
      .attr('rx', 8)
      .attr('fill', d => {
        const nodeData = (g.node(d) as DagreNodeData).node;
        return getNodeColor(nodeData.node_type);
      })
      .attr('fill-opacity', d => {
        const nodeData = (g.node(d) as DagreNodeData).node;
        // Highlight search matches with higher opacity
        return highlightedNodeIds.has(nodeData.id) ? 0.5 : 0.2;
      })
      .attr('stroke', d => {
        const nodeData = (g.node(d) as DagreNodeData).node;
        // Use yellow/gold stroke for search highlights
        return highlightedNodeIds.has(nodeData.id) ? '#f59e0b' : getNodeColor(nodeData.node_type);
      })
      .attr('stroke-width', d => {
        const nodeData = (g.node(d) as DagreNodeData).node;
        // Thicker stroke for highlighted nodes
        return highlightedNodeIds.has(nodeData.id) ? 4 : 2;
      })
      .attr('filter', d => {
        const nodeData = (g.node(d) as DagreNodeData).node;
        // Add glow effect for highlighted nodes
        return highlightedNodeIds.has(nodeData.id) ? 'url(#search-glow)' : null;
      });

    // Node ID
    nodes.append('text')
      .attr('x', 10)
      .attr('y', 18)
      .attr('fill', '#666')
      .attr('font-size', '10px')
      .text(d => `#${d}`);

    // Node title
    nodes.append('text')
      .attr('x', d => (g.node(d) as DagreNodeData).width / 2)
      .attr('y', 38)
      .attr('text-anchor', 'middle')
      .attr('fill', '#24292f')
      .attr('font-size', '12px')
      .text(d => {
        const nodeData = (g.node(d) as DagreNodeData).node;
        return truncate(nodeData.title, 20);
      });

    // Cleanup
    return () => {
      svg.on('.zoom', null);
    };
  }, [graphData, visibleNodeIds, handleSelectNode, highlightedNodeIds]);

  return (
    <div style={{
      ...styles.container,
      ...(isFullscreen ? styles.fullscreenContainer : {}),
    }}>
      {/* Top Bar - Recency Filter */}
      <div style={{
        ...styles.topBar,
        ...(isFullscreen ? styles.fullscreenTopBar : {}),
      }}>
        <div style={styles.topBarLeft}>
          <SearchBar
            nodes={graphData.nodes}
            gitHistory={gitHistory}
            onSelectNode={handleSelectNode}
            onHighlightNodes={setHighlightedNodeIds}
            placeholder="Search nodes, commits..."
          />
        </div>

        <div style={styles.topBarCenter}>
          {viewMode === 'recent' && (
            <>
              {/* -1 button - disabled when only 1 chain shown */}
              <button
                onClick={() => handleShowLess(1)}
                style={{
                  ...styles.topBarBtnDanger,
                  ...(recentChainCount <= 1 ? styles.topBarBtnDisabled : {}),
                }}
                disabled={recentChainCount <= 1}
                title={recentChainCount <= 1 ? "Already showing minimum" : "Show one fewer goal chain"}
              >
                −1 Chain
              </button>

              {/* +1 button - disabled when all chains shown */}
              <button
                onClick={() => handleShowMore(1)}
                style={{
                  ...styles.topBarBtn,
                  ...(hiddenChainCount <= 0 ? styles.topBarBtnDisabled : {}),
                }}
                disabled={hiddenChainCount <= 0}
                title={hiddenChainCount <= 0 ? "All chains shown" : "Show one more goal chain"}
              >
                +1 Chain
              </button>

              {/* +N button - disabled when all chains shown */}
              {!expandInputVisible ? (
                <button
                  onClick={() => setExpandInputVisible(true)}
                  style={{
                    ...styles.topBarBtn,
                    ...(hiddenChainCount <= 0 ? styles.topBarBtnDisabled : {}),
                  }}
                  disabled={hiddenChainCount <= 0}
                  title={hiddenChainCount <= 0 ? "All chains shown" : "Add a specific number of chains"}
                >
                  +N...
                </button>
              ) : (
                <div style={styles.expandInputRow}>
                  <input
                    type="number"
                    min="1"
                    max={hiddenChainCount}
                    value={expandInputValue}
                    onChange={e => setExpandInputValue(e.target.value)}
                    onKeyDown={e => e.key === 'Enter' && handleExpandSubmit()}
                    placeholder={String(hiddenChainCount)}
                    style={styles.topBarInput}
                    autoFocus
                  />
                  <button onClick={handleExpandSubmit} style={styles.topBarBtn}>
                    Add
                  </button>
                </div>
              )}

              {/* Show All button - disabled when all chains shown */}
              <button
                onClick={handleShowAll}
                style={{
                  ...styles.topBarBtnSecondary,
                  ...(hiddenChainCount <= 0 ? styles.topBarBtnDisabled : {}),
                }}
                disabled={hiddenChainCount <= 0}
                title={hiddenChainCount <= 0 ? "All chains shown" : "Show all goal chains in the graph"}
              >
                Show All ({goalChains.length})
              </button>

              {/* Reset button - only when expanded beyond default */}
              {recentChainCount > DEFAULT_RECENT_CHAINS && (
                <button onClick={handleShowRecent} style={styles.topBarBtnSecondary}>
                  Reset to {DEFAULT_RECENT_CHAINS}
                </button>
              )}
            </>
          )}
          {viewMode === 'all' && (
            <button onClick={handleShowRecent} style={styles.topBarBtn}>
              Show Recent Only
            </button>
          )}
          {viewMode === 'single' && (
            <button onClick={handleShowRecent} style={styles.topBarBtn}>
              Back to Recent
            </button>
          )}
        </div>

        <div style={styles.topBarRight}>
          {highlightedNodeIds.size > 0 && (
            <>
              <span style={styles.matchCount}>{highlightedNodeIds.size} matches</span>
              <span style={styles.topBarStatDivider}>·</span>
            </>
          )}
          <span style={styles.topBarStat}>{visibleNodeIds.size} nodes</span>
          <span style={styles.topBarStatDivider}>·</span>
          <span style={styles.topBarStat}>{visibleChains.length} chains</span>
          <button
            onClick={toggleFullscreen}
            style={styles.fullscreenBtn}
            title={isFullscreen ? 'Exit fullscreen (Esc)' : 'Enter fullscreen'}
          >
            {isFullscreen ? '⤓' : '⤢'}
          </button>
        </div>
      </div>

      {/* Hidden chains indicator */}
      {viewMode === 'recent' && hiddenChainCount > 0 && (
        <div style={styles.hiddenIndicator}>
          <span style={styles.hiddenIndicatorText}>
            + {hiddenChainCount} older goal chain{hiddenChainCount !== 1 ? 's' : ''} not shown
          </span>
          <button onClick={handleShowAll} style={styles.hiddenIndicatorBtn}>
            Show all
          </button>
        </div>
      )}

      {/* Side Controls */}
      <div style={{
        ...styles.controls,
        ...(isControlsCollapsed ? styles.controlsCollapsed : {}),
        ...(isFullscreen ? styles.controlsFullscreen : {}),
      }}>
        <button
          onClick={toggleControls}
          style={styles.collapseBtn}
          title={isControlsCollapsed ? 'Show controls' : 'Hide controls'}
        >
          {isControlsCollapsed ? '☰' : '✕'}
        </button>

        {!isControlsCollapsed && (
          <>
            <div style={styles.section}>
              <label style={styles.label}>Jump to Chain</label>
              <select
                value={focusChainIndex ?? ''}
                onChange={e => handleFocusChain(e.target.value ? Number(e.target.value) : null)}
                style={styles.select}
              >
                <option value="">Recent Chains</option>
                {goalChains.map((chain) => (
                  <option key={chain.root.id} value={chains.indexOf(chain)}>
                    {truncate(chain.root.title, 30)}
                  </option>
                ))}
              </select>
            </div>

            <div style={styles.legend}>
              <div style={styles.legendTitle}>Node Types</div>
              {Object.entries(NODE_COLORS).map(([type, color]) => (
                <div key={type} style={styles.legendItem}>
                  <div style={{ ...styles.legendDot, backgroundColor: color }} />
                  <span>{type}</span>
                </div>
              ))}
            </div>

            <div style={styles.zoomInfo}>
              Zoom: {Math.round(zoom * 100)}%
            </div>
          </>
        )}
      </div>

      {/* SVG Container */}
      <div ref={containerRef} style={styles.svgContainer}>
        <svg ref={svgRef} style={styles.svg} />

        {/* Callout Lines for too-small nodes */}
        {highlightedNodeIds.size > 0 && (
          <CalloutLines
            nodes={graphData.nodes}
            highlightedNodeIds={highlightedNodeIds}
            visibilityMap={visibilityMap}
            containerWidth={containerDimensions.width}
            containerHeight={containerDimensions.height}
            onSelectNode={handleSelectNode}
          />
        )}

        {/* MiniMap for off-screen nodes */}
        {highlightedNodeIds.size > 0 && (
          <MiniMap
            nodes={graphData.nodes}
            highlightedNodeIds={highlightedNodeIds}
            visibilityMap={visibilityMap}
            nodePositions={nodePositions}
            graphBounds={graphBounds}
            viewportBounds={{ x: transform.x, y: transform.y, width: containerDimensions.width, height: containerDimensions.height }}
            zoom={zoom}
            onNavigateToNode={handleNavigateToNode}
          />
        )}
      </div>

      {/* Detail Modal */}
      {selectedNode && (
        <div style={styles.modalBackdrop} onClick={() => setSelectedNode(null)}>
          <div style={styles.modal} onClick={e => e.stopPropagation()}>
            <div style={styles.modalHeader}>
              <div style={styles.modalHeaderLeft}>
                <TypeBadge type={selectedNode.node_type} />
                <ConfidenceBadge confidence={getConfidence(selectedNode)} />
                <CommitBadge commit={getCommit(selectedNode)} />
              </div>
              <button onClick={() => setSelectedNode(null)} style={styles.modalCloseBtn}>×</button>
            </div>

            <h2 style={styles.modalTitle}>{selectedNode.title}</h2>
            <p style={styles.modalMeta}>
              Node #{selectedNode.id} · Created {new Date(selectedNode.created_at).toLocaleString()}
            </p>

            {/* Commit Message Section */}
            {(() => {
              const commitHash = getCommit(selectedNode);
              const commitInfo = getCommitInfo(commitHash, gitHistory);
              if (!commitHash) return null;
              return (
                <div style={styles.commitSection}>
                  <a
                    href={githubCommitUrl(commitHash, 'notactuallytreyanastasio/deciduous')}
                    target="_blank"
                    rel="noopener noreferrer"
                    style={styles.commitLink}
                  >
                    {shortCommit(commitHash)}
                  </a>
                  {commitInfo ? (
                    <>
                      <div style={styles.commitMessage}>{commitInfo.message}</div>
                      <div style={styles.commitMeta}>
                        by {commitInfo.author} · {new Date(commitInfo.date).toLocaleDateString()}
                        {commitInfo.files_changed && ` · ${commitInfo.files_changed} files changed`}
                      </div>
                    </>
                  ) : (
                    <div style={styles.commitMeta}>Commit details not available</div>
                  )}
                </div>
              );
            })()}

            {selectedNode.description && (
              <div style={styles.modalSection}>
                <p style={styles.modalDescription}>{selectedNode.description}</p>
              </div>
            )}

            {/* Connections - clickable to navigate */}
            <ConnectionsList
              node={selectedNode}
              graphData={graphData}
              onSelectNode={handleSelectNodeById}
            />

            <div style={styles.modalFooter}>
              <span style={styles.modalHint}>Click connected nodes to navigate · Click outside or × to close</span>
            </div>
          </div>
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
                <span>{truncate(n?.title || 'Unknown', 25)}</span>
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
                <span>{truncate(n?.title || 'Unknown', 20)}</span>
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
    flexDirection: 'column',
    position: 'relative',
    backgroundColor: '#ffffff',
  },
  fullscreenContainer: {
    position: 'fixed',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    zIndex: 1000,
    height: '100vh',
  },
  // Top Bar - Prominent recency filter controls
  topBar: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: '12px 20px',
    backgroundColor: '#f6f8fa',
    borderBottom: '1px solid #d0d7de',
    zIndex: 20,
    flexShrink: 0,
    flexWrap: 'wrap',
    gap: '8px',
  },
  topBarLeft: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    flex: 1,
    minWidth: 0,
    maxWidth: '400px',
  },
  topBarTitle: {
    fontSize: '14px',
    fontWeight: 600,
    color: '#0969da',
  },
  topBarSubtitle: {
    fontSize: '13px',
    color: '#57606a',
    cursor: 'help',
  },
  topBarCenter: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
  },
  topBarBtn: {
    padding: '6px 12px',
    backgroundColor: '#2da44e',
    border: 'none',
    borderRadius: '6px',
    color: '#fff',
    fontSize: '12px',
    fontWeight: 500,
    cursor: 'pointer',
    transition: 'background-color 0.15s',
  },
  topBarBtnSecondary: {
    padding: '6px 12px',
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    color: '#24292f',
    fontSize: '12px',
    fontWeight: 500,
    cursor: 'pointer',
    transition: 'background-color 0.15s',
  },
  topBarBtnDanger: {
    padding: '6px 12px',
    backgroundColor: '#ffebe9',
    border: '1px solid #ff8182',
    borderRadius: '6px',
    color: '#cf222e',
    fontSize: '12px',
    fontWeight: 500,
    cursor: 'pointer',
    transition: 'background-color 0.15s',
  },
  topBarBtnDisabled: {
    opacity: 0.5,
    cursor: 'not-allowed',
  },
  topBarInput: {
    width: '50px',
    padding: '5px 8px',
    backgroundColor: '#ffffff',
    border: '1px solid #2da44e',
    borderRadius: '6px',
    color: '#24292f',
    fontSize: '12px',
    textAlign: 'center' as const,
  },
  topBarRight: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
  },
  topBarStat: {
    fontSize: '12px',
    color: '#57606a',
  },
  matchCount: {
    fontSize: '12px',
    fontWeight: 600,
    color: '#f59e0b',
    backgroundColor: '#fef3c7',
    padding: '2px 8px',
    borderRadius: '10px',
  },
  topBarStatDivider: {
    color: '#d0d7de',
  },
  fullscreenBtn: {
    marginLeft: '12px',
    padding: '6px 10px',
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    color: '#24292f',
    fontSize: '14px',
    cursor: 'pointer',
    transition: 'background-color 0.15s',
  },
  fullscreenTopBar: {
    padding: '8px 20px',
  },
  // Hidden chains indicator - visual hint of more content
  hiddenIndicator: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '12px',
    padding: '8px 20px',
    backgroundColor: '#fff8c5',
    borderBottom: '1px solid #d4a72c',
    flexShrink: 0,
  },
  hiddenIndicatorText: {
    fontSize: '12px',
    color: '#9a6700',
    fontStyle: 'italic',
  },
  hiddenIndicatorBtn: {
    padding: '4px 10px',
    backgroundColor: 'transparent',
    border: '1px solid #9a6700',
    borderRadius: '4px',
    color: '#9a6700',
    fontSize: '11px',
    cursor: 'pointer',
  },
  // Side controls (simplified)
  controls: {
    position: 'absolute',
    top: '70px',
    left: '20px',
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    padding: '15px',
    borderRadius: '8px',
    zIndex: 10,
    width: '180px',
    boxShadow: '0 1px 3px rgba(0,0,0,0.08)',
    transition: 'width 0.2s, padding 0.2s',
  },
  controlsCollapsed: {
    width: '40px',
    padding: '8px',
    overflow: 'hidden',
  },
  controlsFullscreen: {
    top: '60px',
  },
  collapseBtn: {
    width: '24px',
    height: '24px',
    padding: 0,
    backgroundColor: 'transparent',
    border: 'none',
    borderRadius: '4px',
    color: '#57606a',
    fontSize: '14px',
    cursor: 'pointer',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    marginBottom: '10px',
  },
  expandInputRow: {
    display: 'flex',
    gap: '4px',
    alignItems: 'center',
  },
  section: {
    marginBottom: '15px',
  },
  label: {
    display: 'block',
    fontSize: '11px',
    color: '#57606a',
    marginBottom: '6px',
    textTransform: 'uppercase',
  },
  select: {
    width: '100%',
    padding: '8px',
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderRadius: '4px',
    color: '#24292f',
    fontSize: '12px',
  },
  legend: {
    marginTop: '20px',
  },
  legendTitle: {
    fontSize: '11px',
    color: '#57606a',
    marginBottom: '8px',
    textTransform: 'uppercase',
  },
  legendItem: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    fontSize: '11px',
    color: '#57606a',
    marginBottom: '4px',
  },
  legendDot: {
    width: '10px',
    height: '10px',
    borderRadius: '50%',
  },
  zoomInfo: {
    marginTop: '15px',
    fontSize: '11px',
    color: '#6e7781',
  },
  svgContainer: {
    flex: 1,
    position: 'relative',
    minHeight: 0,
    backgroundColor: '#f6f8fa',
  },
  svg: {
    width: '100%',
    height: '100%',
  },
  // Modal styles
  modalBackdrop: {
    position: 'fixed',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    backgroundColor: 'rgba(0, 0, 0, 0.5)',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    zIndex: 100,
  },
  modal: {
    backgroundColor: '#ffffff',
    borderRadius: '12px',
    padding: '24px',
    width: '90%',
    maxWidth: '600px',
    maxHeight: '80vh',
    overflowY: 'auto',
    border: '1px solid #d0d7de',
    boxShadow: '0 8px 32px rgba(0, 0, 0, 0.15)',
  },
  modalHeader: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'flex-start',
    marginBottom: '16px',
  },
  modalHeaderLeft: {
    display: 'flex',
    gap: '8px',
    flexWrap: 'wrap',
  },
  modalCloseBtn: {
    width: '32px',
    height: '32px',
    border: 'none',
    background: '#f6f8fa',
    color: '#57606a',
    borderRadius: '6px',
    fontSize: '20px',
    cursor: 'pointer',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    transition: 'background-color 0.15s',
  },
  modalTitle: {
    fontSize: '20px',
    fontWeight: 600,
    margin: '0 0 8px 0',
    color: '#24292f',
  },
  modalMeta: {
    fontSize: '13px',
    color: '#57606a',
    margin: '0 0 16px 0',
  },
  modalSection: {
    marginBottom: '20px',
    padding: '12px',
    backgroundColor: '#f6f8fa',
    borderRadius: '8px',
  },
  modalDescription: {
    fontSize: '14px',
    color: '#24292f',
    lineHeight: 1.6,
    margin: 0,
  },
  commitSection: {
    backgroundColor: '#f6f8fa',
    padding: '12px 16px',
    borderRadius: '8px',
    borderLeft: '3px solid #0969da',
    marginBottom: '16px',
  },
  commitLink: {
    fontFamily: 'monospace',
    fontSize: '13px',
    color: '#0969da',
    textDecoration: 'none',
    backgroundColor: '#ddf4ff',
    padding: '2px 8px',
    borderRadius: '4px',
  },
  commitMessage: {
    fontSize: '15px',
    color: '#24292f',
    marginTop: '10px',
    lineHeight: 1.5,
    fontWeight: 500,
    whiteSpace: 'pre-wrap',
  },
  commitMeta: {
    fontSize: '12px',
    color: '#57606a',
    marginTop: '6px',
  },
  modalFooter: {
    marginTop: '20px',
    paddingTop: '16px',
    borderTop: '1px solid #d0d7de',
  },
  modalHint: {
    fontSize: '12px',
    color: '#6e7781',
    fontStyle: 'italic',
  },
  // Connection styles (used inside modal)
  detailSection: {
    marginTop: '16px',
  },
  sectionTitle: {
    fontSize: '12px',
    color: '#57606a',
    margin: '0 0 10px 0',
    textTransform: 'uppercase',
    fontWeight: 600,
  },
  connection: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '10px 12px',
    backgroundColor: '#f6f8fa',
    borderRadius: '6px',
    marginBottom: '6px',
    cursor: 'pointer',
    fontSize: '13px',
    color: '#24292f',
    transition: 'background-color 0.15s',
    border: '1px solid #d0d7de',
  },
};
