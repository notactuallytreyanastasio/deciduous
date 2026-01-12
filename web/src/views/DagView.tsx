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
import { getConfidence, getCommit, getPrompt, truncate } from '../types/graph';
import { TypeBadge, ConfidenceBadge, CommitBadge, EdgeBadge } from '../components/NodeBadge';
import { SearchBar } from '../components/SearchBar';
import { CalloutLines } from '../components/CalloutLines';
import { MiniMap } from '../components/MiniMap';
import { AskResponseModal } from '../components/AskResponseModal';
import { useNodeVisibility } from '../hooks/useNodeVisibility';
import { useUrlState } from '../hooks/useUrlState';
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


// Default number of recent chains to show (increased from 8 for larger graphs)
const DEFAULT_RECENT_CHAINS = 1000;

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
  const [zoom, setZoom] = useState(1);

  // URL-synced state for deep linking and sharing
  const {
    state: urlState,
    setSelectedNodeId,
    setSearchQuery,
    setSearchSort,
    setViewMode,
    setRecentChainCount,
    setFocusChainIndex,
    copyLinkToClipboard,
  } = useUrlState();

  // Always fullscreen - no mini view
  const isFullscreen = true;

  // Visual card stack - shows parent/child relationships spatially
  // Each card has: nodeId, level (negative=parent, 0=root, positive=child), and source relationship
  interface StackCard {
    nodeId: number;
    level: number;  // -N for parents, 0 for root, +N for children
    relation: 'root' | 'parent' | 'child';
    edgeType?: string;
    isNew?: boolean;  // For slide-in animation
  }
  const [cardStack, setCardStack] = useState<StackCard[]>([]);

  // Clear "isNew" flag after animation completes
  useEffect(() => {
    if (cardStack.some(c => c.isNew)) {
      const timer = setTimeout(() => {
        setCardStack(prev => prev.map(c => ({ ...c, isNew: false })));
      }, 300); // Match animation duration
      return () => clearTimeout(timer);
    }
  }, [cardStack]);

  // Get node by ID helper
  const getNodeById = useCallback((id: number) =>
    graphData.nodes.find(n => n.id === id) ?? null
  , [graphData.nodes]);

  // Add a parent card (appears above with slide-from-top animation)
  const addParentCard = useCallback((nodeId: number) => {
    setCardStack(prev => {
      // Find the minimum level (most "parent" position)
      const minLevel = prev.length > 0 ? Math.min(...prev.map(c => c.level)) : 0;
      // Check if this node is already in the stack
      if (prev.some(c => c.nodeId === nodeId)) return prev;
      return [...prev, { nodeId, level: minLevel - 1, relation: 'parent' as const, isNew: true }];
    });
  }, []);

  // Add a child card (appears below with slide-from-bottom animation)
  const addChildCard = useCallback((nodeId: number, edgeType?: string) => {
    setCardStack(prev => {
      // Find the maximum level (most "child" position)
      const maxLevel = prev.length > 0 ? Math.max(...prev.map(c => c.level)) : 0;
      // Check if this node is already in the stack
      if (prev.some(c => c.nodeId === nodeId)) return prev;
      return [...prev, { nodeId, level: maxLevel + 1, relation: 'child' as const, edgeType, isNew: true }];
    });
  }, []);

  // Remove a specific card from the stack
  const removeCard = useCallback((nodeId: number) => {
    setCardStack(prev => prev.filter(c => c.nodeId !== nodeId));
  }, []);

  // Close all cards
  const closeAllCards = useCallback(() => {
    setCardStack([]);
    setSelectedNodeId(null);
  }, [setSelectedNodeId]);

  // Unified callback for adding to card stack (used by CalloutLines)
  const handleAddToCardStack = useCallback((nodeId: number, relation: 'root' | 'parent' | 'child', edgeType?: string) => {
    if (relation === 'root') {
      // Start a new card stack with this node as root
      setCardStack([{ nodeId, level: 0, relation: 'root', isNew: true }]);
      setSelectedNodeId(nodeId);
    } else if (relation === 'parent') {
      addParentCard(nodeId);
    } else if (relation === 'child') {
      addChildCard(nodeId, edgeType);
    }
  }, [addParentCard, addChildCard, setSelectedNodeId]);

  // Legacy: derive selectedNode for compatibility with other code
  const selectedNode = useMemo(() => {
    if (cardStack.length > 0) {
      // Return the root card's node (level 0) or the first card
      const rootCard = cardStack.find(c => c.level === 0) || cardStack[0];
      return getNodeById(rootCard.nodeId);
    }
    if (urlState.selectedNodeId === null) return null;
    return graphData.nodes.find(n => n.id === urlState.selectedNodeId) ?? null;
  }, [urlState.selectedNodeId, graphData.nodes, cardStack, getNodeById]);

  // Local UI state (not URL-synced)
  const isMobile = typeof window !== 'undefined' && window.innerWidth < 768;
  const [isControlsCollapsed, setIsControlsCollapsed] = useState(isMobile);

  // Search state - highlighted node IDs from search
  const [highlightedNodeIds, setHighlightedNodeIds] = useState<Set<number>>(new Set());

  // Git-Log Modal state
  const [showGitLogModal, setShowGitLogModal] = useState(false);
  const [gitLogFilter, setGitLogFilter] = useState<'all' | 'linked'>('all');

  // Commit Correlation View state
  const [showCorrelationModal, setShowCorrelationModal] = useState(false);
  const [hoveredLink, setHoveredLink] = useState<{ commit?: string; node?: number } | null>(null);

  // Time Slider / Playback state
  const [showTimeSlider, setShowTimeSlider] = useState(false);
  const [timeSliderValue, setTimeSliderValue] = useState(100);
  const [isPlaying, setIsPlaying] = useState(false);
  const [playbackSpeed, setPlaybackSpeed] = useState(1);

  // Ask Claude state
  const [askModalOpen, setAskModalOpen] = useState(false);
  const [askQuestion, setAskQuestion] = useState('');
  const [askResponse, setAskResponse] = useState<string | null>(null);
  const [askLoading, setAskLoading] = useState(false);
  const [askInputVisible, setAskInputVisible] = useState(false);

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
    if (urlState.viewMode === 'single' && urlState.focusChainIndex !== null) {
      return [chains[urlState.focusChainIndex]].filter(Boolean);
    }
    if (urlState.viewMode === 'recent') {
      return goalChains.slice(0, urlState.recentChainCount);
    }
    return sortedChains; // 'all' mode
  }, [urlState.viewMode, urlState.focusChainIndex, chains, goalChains, sortedChains, urlState.recentChainCount]);

  // Get all visible node IDs from visible chains
  const visibleNodeIds = useMemo(() => {
    const ids = new Set<number>();
    visibleChains.forEach(chain => {
      chain.nodes.forEach(n => ids.add(n.id));
    });
    return ids;
  }, [visibleChains]);

  // Calculate how many chains are hidden
  const hiddenChainCount = goalChains.length - (urlState.viewMode === 'recent' ? urlState.recentChainCount : 0);

  // Time slider: calculate time bounds from all nodes
  const timeBounds = useMemo(() => {
    if (graphData.nodes.length === 0) return { min: Date.now(), max: Date.now() };
    const timestamps = graphData.nodes.map(n => new Date(n.created_at).getTime());
    return { min: Math.min(...timestamps), max: Math.max(...timestamps) };
  }, [graphData.nodes]);

  // Current time based on slider position
  const currentTime = useMemo(() => {
    const { min, max } = timeBounds;
    return min + (timeSliderValue / 100) * (max - min);
  }, [timeBounds, timeSliderValue]);

  // Nodes visible at current time (null = show all)
  const timeFilteredNodeIds = useMemo(() => {
    if (!showTimeSlider) return null;
    return new Set(
      graphData.nodes
        .filter(n => new Date(n.created_at).getTime() <= currentTime)
        .map(n => n.id)
    );
  }, [graphData.nodes, currentTime, showTimeSlider]);

  // Playback effect
  useEffect(() => {
    if (!isPlaying) return;
    const interval = setInterval(() => {
      setTimeSliderValue(prev => {
        const next = prev + (0.5 * playbackSpeed);
        if (next >= 100) {
          setIsPlaying(false);
          return 100;
        }
        return next;
      });
    }, 50);
    return () => clearInterval(interval);
  }, [isPlaying, playbackSpeed]);

  // When clicking a node in the graph, start a fresh card stack with just that node
  const handleSelectNode = useCallback((node: DecisionNode) => {
    setCardStack([{ nodeId: node.id, level: 0, relation: 'root', isNew: true }]);
    setSelectedNodeId(node.id);
  }, [setSelectedNodeId]);

  // When clicking an incoming (parent) node in a card
  const handleSelectParent = useCallback((nodeId: number) => {
    addParentCard(nodeId);
  }, [addParentCard]);

  // When clicking an outgoing (child) node in a card
  const handleSelectChild = useCallback((nodeId: number, edgeType?: string) => {
    addChildCard(nodeId, edgeType);
  }, [addChildCard]);

  // State for custom expand input
  const [expandInputVisible, setExpandInputVisible] = useState(false);
  const [expandInputValue, setExpandInputValue] = useState('');

  const handleShowMore = useCallback((count: number = 1) => {
    setRecentChainCount(Math.min(urlState.recentChainCount + count, goalChains.length));
    setExpandInputVisible(false);
    setExpandInputValue('');
  }, [urlState.recentChainCount, goalChains.length, setRecentChainCount]);

  const handleShowLess = useCallback((count: number = 1) => {
    setRecentChainCount(Math.max(urlState.recentChainCount - count, 1));
  }, [urlState.recentChainCount, setRecentChainCount]);

  const handleExpandSubmit = useCallback(() => {
    const num = parseInt(expandInputValue, 10);
    if (num > 0) {
      handleShowMore(num);
    }
  }, [expandInputValue, handleShowMore]);

  const handleShowAll = useCallback(() => {
    setViewMode('all');
  }, [setViewMode]);

  const handleShowRecent = useCallback(() => {
    setViewMode('recent');
    setRecentChainCount(DEFAULT_RECENT_CHAINS);
    setFocusChainIndex(null);
  }, [setViewMode, setRecentChainCount, setFocusChainIndex]);

  // Fullscreen toggle removed - always fullscreen

  const toggleControls = useCallback(() => {
    setIsControlsCollapsed(prev => !prev);
  }, []);

  // Submit question to Claude
  const submitAskQuestion = useCallback(async () => {
    if (!askQuestion.trim() || askLoading) return;
    setAskLoading(true);
    setAskModalOpen(true);
    setAskResponse(null);

    try {
      const res = await fetch('/api/ask', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          question: askQuestion,
          context: {
            selected_node_id: urlState.selectedNodeId,
            visible_node_ids: Array.from(visibleNodeIds),
            current_branch: selectedNode?.metadata_json
              ? JSON.parse(selectedNode.metadata_json)?.branch
              : null,
          },
        }),
      });
      const json = await res.json();
      if (json.ok) {
        setAskResponse(json.data.answer);
      } else {
        setAskResponse(`Error: ${json.error || 'Unknown error'}`);
      }
    } catch (e) {
      setAskResponse(`Error: ${e instanceof Error ? e.message : 'Failed to connect'}`);
    } finally {
      setAskLoading(false);
      setAskInputVisible(false);
    }
  }, [askQuestion, askLoading, urlState.selectedNodeId, visibleNodeIds, selectedNode]);

  // Handle ask input key events
  const handleAskKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      submitAskQuestion();
    } else if (e.key === 'Escape') {
      setAskInputVisible(false);
      setAskQuestion('');
    }
  }, [submitAskQuestion]);

  // Escape key closes selected node modal
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && selectedNode) {
        setSelectedNodeId(null);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [selectedNode, setSelectedNodeId]);

  // Prevent browser zoom on wheel events (trackpad pinch sends wheel with ctrlKey)
  // This allows D3 zoom to handle all zoom gestures on the graph
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const preventBrowserZoom = (e: WheelEvent) => {
      // Trackpad pinch zoom sends wheel events with ctrlKey=true
      // Prevent browser from zooming the page
      if (e.ctrlKey) {
        e.preventDefault();
      }
    };

    // Must use passive: false to be able to preventDefault
    container.addEventListener('wheel', preventBrowserZoom, { passive: false });
    return () => container.removeEventListener('wheel', preventBrowserZoom);
  }, []);

  const handleFocusChain = useCallback((index: number | null) => {
    if (index === null) {
      setViewMode('recent');
      setFocusChainIndex(null);
    } else {
      setViewMode('single');
      setFocusChainIndex(index);
    }
  }, [setViewMode, setFocusChainIndex]);

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
    // Don't open modal here - let user click on panel or node to open it
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
            query={urlState.searchQuery}
            onQueryChange={setSearchQuery}
            sortOrder={urlState.searchSort}
            onSortOrderChange={setSearchSort}
          />
        </div>

        <div style={styles.topBarCenter}>
          {urlState.viewMode === 'recent' && (
            <>
              {/* -1 button - disabled when only 1 chain shown */}
              <button
                onClick={() => handleShowLess(1)}
                style={{
                  ...styles.topBarBtnDanger,
                  ...(urlState.recentChainCount <= 1 ? styles.topBarBtnDisabled : {}),
                }}
                disabled={urlState.recentChainCount <= 1}
                title={urlState.recentChainCount <= 1 ? "Already showing minimum" : "Show one fewer goal chain"}
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
              {urlState.recentChainCount > DEFAULT_RECENT_CHAINS && (
                <button onClick={handleShowRecent} style={styles.topBarBtnSecondary}>
                  Reset to {DEFAULT_RECENT_CHAINS}
                </button>
              )}
            </>
          )}
          {urlState.viewMode === 'all' && (
            <button onClick={handleShowRecent} style={styles.topBarBtn}>
              Show Recent Only
            </button>
          )}
          {urlState.viewMode === 'single' && (
            <button onClick={handleShowRecent} style={styles.topBarBtn}>
              Back to Recent
            </button>
          )}

          {/* Separator */}
          <span style={styles.topBarStatDivider}>|</span>

          {/* Git History Tools */}
          <button
            onClick={() => setShowGitLogModal(true)}
            style={{
              ...styles.topBarBtnSecondary,
              ...(gitHistory.length === 0 ? styles.topBarBtnDisabled : {}),
            }}
            disabled={gitHistory.length === 0}
            title="View git commit history with linked decisions"
          >
            Git Log
          </button>
          <button
            onClick={() => setShowCorrelationModal(true)}
            style={{
              ...styles.topBarBtnSecondary,
              ...(gitHistory.length === 0 ? styles.topBarBtnDisabled : {}),
            }}
            disabled={gitHistory.length === 0}
            title="View commit-decision correlation"
          >
            Correlation
          </button>
          <button
            onClick={() => setShowTimeSlider(prev => !prev)}
            style={{
              ...styles.topBarBtnSecondary,
              ...(showTimeSlider ? { backgroundColor: '#0969da', color: '#fff', borderColor: '#0969da' } : {}),
            }}
            title="Toggle time slider for temporal navigation"
          >
            Timeline
          </button>
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

          {/* Ask Claude Button */}
          {!askInputVisible ? (
            <button
              onClick={() => setAskInputVisible(true)}
              style={styles.askButton}
              title="Ask Claude about the codebase"
            >
              Ask about the code
            </button>
          ) : (
            <div style={styles.askInputContainer}>
              <input
                type="text"
                value={askQuestion}
                onChange={e => setAskQuestion(e.target.value)}
                onKeyDown={handleAskKeyDown}
                placeholder="Ask a question about the code..."
                style={styles.askInput}
                autoFocus
                disabled={askLoading}
              />
              <button
                onClick={submitAskQuestion}
                style={styles.askSubmitBtn}
                disabled={askLoading || !askQuestion.trim()}
              >
                {askLoading ? '...' : 'Ask'}
              </button>
              <button
                onClick={() => { setAskInputVisible(false); setAskQuestion(''); }}
                style={styles.askCancelBtn}
                title="Cancel"
              >
                ×
              </button>
            </div>
          )}

          <button
            onClick={async () => {
              await copyLinkToClipboard();
              // Brief visual feedback could be added via state if desired
            }}
            style={styles.copyLinkBtn}
            title="Copy shareable link to clipboard"
          >
            🔗 Copy Link
          </button>
        </div>
      </div>

      {/* Hidden chains indicator */}
      {urlState.viewMode === 'recent' && hiddenChainCount > 0 && (
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
                value={urlState.focusChainIndex ?? ''}
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
            edges={graphData.edges}
            highlightedNodeIds={highlightedNodeIds}
            visibilityMap={visibilityMap}
            containerWidth={containerDimensions.width}
            containerHeight={containerDimensions.height}
            onSelectNode={handleSelectNode}
            onNavigateToNode={handleNavigateToNode}
            onAddToCardStack={handleAddToCardStack}
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

      {/* Visual Card Stack - shows parent/child relationships spatially */}
      {cardStack.length > 0 && (
        <div style={styles.cardStackBackdrop} onClick={closeAllCards}>
          {/* CSS Keyframes for slide animations */}
          <style>{`
            @keyframes slideFromTop {
              from {
                opacity: 0;
                transform: translateY(-30px);
              }
              to {
                opacity: 1;
                transform: translateY(0);
              }
            }
            @keyframes slideFromBottom {
              from {
                opacity: 0;
                transform: translateY(30px);
              }
              to {
                opacity: 1;
                transform: translateY(0);
              }
            }
            @keyframes slideFromCenter {
              from {
                opacity: 0;
                transform: scale(0.95);
              }
              to {
                opacity: 1;
                transform: scale(1);
              }
            }
            .stack-card-btn:hover:not(:disabled) {
              background-color: #f0f6fc !important;
              border-color: #0969da !important;
              transform: translateX(4px);
            }
            .stack-card-close:hover {
              background-color: #ffebe9 !important;
              color: #cf222e !important;
            }
          `}</style>
          <div style={styles.cardStackContainer} onClick={e => e.stopPropagation()}>
            {/* Sort cards by level: parents (negative) at top, children (positive) at bottom */}
            {[...cardStack]
              .sort((a, b) => a.level - b.level)
              .map((card, cardIndex) => {
                const node = getNodeById(card.nodeId);
                if (!node) return null;

                const incoming = graphData.edges.filter(e => e.to_node_id === node.id);
                const outgoing = graphData.edges.filter(e => e.from_node_id === node.id);

                // Calculate visual offset based on position in sorted list
                const offset = cardIndex * 8;

                // Determine animation based on relation and whether card is new
                const animationName = card.isNew
                  ? card.relation === 'parent' ? 'slideFromTop'
                    : card.relation === 'child' ? 'slideFromBottom'
                    : 'slideFromCenter'
                  : 'none';

                return (
                  <div
                    key={card.nodeId}
                    style={{
                      ...styles.stackCard,
                      marginLeft: `${offset}px`,
                      zIndex: 100 + cardIndex,
                      borderLeft: card.relation === 'parent' ? '4px solid #8250df' :
                                  card.relation === 'child' ? '4px solid #1a7f37' :
                                  '4px solid #0969da',
                      animation: card.isNew ? `${animationName} 0.25s ease-out forwards` : 'none',
                    }}
                  >
                    {/* Card header with relation indicator */}
                    <div style={styles.stackCardHeader}>
                      <div style={styles.stackCardHeaderLeft}>
                        {card.relation === 'parent' && <span style={styles.relationBadgeParent}>↑ PARENT</span>}
                        {card.relation === 'child' && <span style={styles.relationBadgeChild}>↓ CHILD {card.edgeType && `(${card.edgeType})`}</span>}
                        {card.relation === 'root' && <span style={styles.relationBadgeRoot}>● ROOT</span>}
                        <TypeBadge type={node.node_type} />
                        <ConfidenceBadge confidence={getConfidence(node)} />
                      </div>
                      <button onClick={() => removeCard(card.nodeId)} style={styles.stackCardClose} className="stack-card-close">×</button>
                    </div>

                    <h3 style={styles.stackCardTitle}>{node.title}</h3>
                    <p style={styles.stackCardMeta}>
                      Node #{node.id} · {new Date(node.created_at).toLocaleDateString()}
                    </p>

                    {/* Description */}
                    {node.description && (
                      <p style={styles.stackCardDescription}>{node.description}</p>
                    )}

                    {/* Prompt - show full content, no truncation */}
                    {(() => {
                      const prompt = getPrompt(node);
                      if (!prompt) return null;
                      return (
                        <div style={styles.stackCardPrompt}>
                          <strong>Prompt:</strong> {prompt}
                        </div>
                      );
                    })()}

                    {/* Commit */}
                    {(() => {
                      const commitHash = getCommit(node);
                      if (!commitHash) return null;
                      const commitInfo = getCommitInfo(commitHash, gitHistory);
                      return (
                        <div style={styles.stackCardCommit}>
                          <CommitBadge commit={commitHash} />
                          {commitInfo && <span style={styles.stackCardCommitMsg}>{truncate(commitInfo.message, 60)}</span>}
                        </div>
                      );
                    })()}

                    {/* Parent connections - click to add parent card above */}
                    {incoming.length > 0 && (
                      <div style={styles.stackCardConnections}>
                        <span style={styles.stackCardConnectionLabel}>↑ Parents ({incoming.length})</span>
                        <div style={styles.stackCardConnectionList}>
                          {incoming.map(e => {
                            const parentNode = getNodeById(e.from_node_id);
                            const isInStack = cardStack.some(c => c.nodeId === e.from_node_id);
                            return (
                              <button
                                key={e.id}
                                onClick={() => !isInStack && handleSelectParent(e.from_node_id)}
                                style={{
                                  ...styles.stackCardConnectionBtn,
                                  ...(isInStack ? styles.stackCardConnectionBtnActive : {}),
                                }}
                                disabled={isInStack}
                                className="stack-card-btn"
                              >
                                <TypeBadge type={parentNode?.node_type || 'observation'} size="sm" />
                                {truncate(parentNode?.title || 'Unknown', 40)}
                                {isInStack && ' ✓'}
                              </button>
                            );
                          })}
                        </div>
                      </div>
                    )}

                    {/* Child connections - click to add child card below */}
                    {outgoing.length > 0 && (
                      <div style={styles.stackCardConnections}>
                        <span style={styles.stackCardConnectionLabel}>↓ Children ({outgoing.length})</span>
                        <div style={styles.stackCardConnectionList}>
                          {outgoing.map(e => {
                            const childNode = getNodeById(e.to_node_id);
                            const isInStack = cardStack.some(c => c.nodeId === e.to_node_id);
                            return (
                              <button
                                key={e.id}
                                onClick={() => !isInStack && handleSelectChild(e.to_node_id, e.edge_type)}
                                style={{
                                  ...styles.stackCardConnectionBtn,
                                  ...(isInStack ? styles.stackCardConnectionBtnActive : {}),
                                }}
                                disabled={isInStack}
                                className="stack-card-btn"
                              >
                                <EdgeBadge type={e.edge_type} />
                                <TypeBadge type={childNode?.node_type || 'observation'} size="sm" />
                                {truncate(childNode?.title || 'Unknown', 35)}
                                {isInStack && ' ✓'}
                              </button>
                            );
                          })}
                        </div>
                      </div>
                    )}
                  </div>
                );
              })}

            {/* Stack summary footer */}
            <div style={styles.stackFooter}>
              <span>{cardStack.length} card{cardStack.length !== 1 ? 's' : ''} in stack</span>
              <button onClick={closeAllCards} style={styles.stackClearBtn}>Clear All</button>
            </div>
          </div>
        </div>
      )}

      {/* Git-Log Modal */}
      {showGitLogModal && (
        <div style={styles.modalBackdrop} onClick={() => setShowGitLogModal(false)}>
          <div style={{ ...styles.modal, maxWidth: '700px' }} onClick={e => e.stopPropagation()}>
            <div style={styles.modalHeader}>
              <h3 style={{ margin: 0, fontSize: '18px' }}>Git History</h3>
              <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                <button
                  onClick={() => setGitLogFilter('all')}
                  style={{
                    ...styles.gitLogFilterBtn,
                    ...(gitLogFilter === 'all' ? styles.gitLogFilterBtnActive : {}),
                  }}
                >
                  All
                </button>
                <button
                  onClick={() => setGitLogFilter('linked')}
                  style={{
                    ...styles.gitLogFilterBtn,
                    ...(gitLogFilter === 'linked' ? styles.gitLogFilterBtnActive : {}),
                  }}
                >
                  Linked Only
                </button>
                <button onClick={() => setShowGitLogModal(false)} style={styles.modalCloseBtn}>×</button>
              </div>
            </div>
            <div style={{ maxHeight: '60vh', overflowY: 'auto', padding: '16px 0' }}>
              {gitHistory
                .filter(commit => {
                  if (gitLogFilter === 'all') return true;
                  return graphData.nodes.some(n => {
                    const nodeCommit = getCommit(n);
                    return nodeCommit === commit.hash || nodeCommit === commit.short_hash || commit.hash.startsWith(nodeCommit || '');
                  });
                })
                .map((commit, index, arr) => {
                  const linkedNodes = graphData.nodes.filter(n => {
                    const nodeCommit = getCommit(n);
                    return nodeCommit === commit.hash || nodeCommit === commit.short_hash || commit.hash.startsWith(nodeCommit || '');
                  });
                  const isLast = index === arr.length - 1;

                  return (
                    <div key={commit.hash} style={styles.gitLogItem}>
                      {!isLast && <div style={styles.gitLogLine} />}
                      <div style={styles.gitLogDot} />
                      <div>
                        <div style={styles.gitLogCommit}>
                          <span style={{ fontFamily: 'monospace', color: '#0969da' }}>{commit.short_hash}</span>
                          {index === 0 && <span style={styles.gitLogHeadBadge}>HEAD</span>}
                          <span style={{ marginLeft: '8px' }}>{truncate(commit.message.split('\n')[0], 50)}</span>
                        </div>
                        <div style={styles.gitLogMeta}>
                          {commit.author} · {new Date(commit.date).toLocaleDateString()}
                          {commit.files_changed && ` · ${commit.files_changed} files`}
                        </div>
                        {linkedNodes.length > 0 && (
                          <div style={{ marginTop: '8px' }}>
                            {linkedNodes.map(node => (
                              <div
                                key={node.id}
                                onClick={() => {
                                  setShowGitLogModal(false);
                                  setSelectedNodeId(node.id);
                                }}
                                style={styles.gitLogLinkedNode}
                              >
                                <TypeBadge type={node.node_type} size="sm" />
                                <span>#{node.id}: {truncate(node.title, 40)}</span>
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    </div>
                  );
                })}
              {gitHistory.length === 0 && (
                <div style={{ textAlign: 'center', color: '#57606a', padding: '40px' }}>
                  No git history available. Run <code>deciduous sync</code> to generate.
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Commit Correlation Modal */}
      {showCorrelationModal && (
        <div style={styles.modalBackdrop} onClick={() => setShowCorrelationModal(false)}>
          <div style={{ ...styles.modal, maxWidth: '1000px', width: '95%' }} onClick={e => e.stopPropagation()}>
            <div style={styles.modalHeader}>
              <h3 style={{ margin: 0, fontSize: '18px' }}>Commit-Decision Correlation</h3>
              <button onClick={() => setShowCorrelationModal(false)} style={styles.modalCloseBtn}>×</button>
            </div>
            <div style={styles.correlationContainer}>
              {/* Left column: Commits */}
              <div style={styles.correlationColumn}>
                <h4 style={styles.correlationColumnTitle}>Git Commits</h4>
                {gitHistory.map(commit => {
                  const isHovered = hoveredLink?.commit === commit.hash;
                  const hasLinks = graphData.nodes.some(n => {
                    const nodeCommit = getCommit(n);
                    return nodeCommit === commit.hash || nodeCommit === commit.short_hash;
                  });
                  return (
                    <div
                      key={commit.hash}
                      data-commit={commit.hash}
                      onMouseEnter={() => hasLinks && setHoveredLink({ commit: commit.hash })}
                      onMouseLeave={() => setHoveredLink(null)}
                      style={{
                        ...styles.correlationItem,
                        ...(isHovered ? styles.correlationItemHighlight : {}),
                        ...(hasLinks ? { borderLeft: '3px solid #0969da' } : {}),
                      }}
                    >
                      <div style={{ fontFamily: 'monospace', fontSize: '12px', color: '#0969da' }}>
                        {commit.short_hash}
                      </div>
                      <div style={{ fontSize: '13px', fontWeight: 500 }}>
                        {truncate(commit.message.split('\n')[0], 35)}
                      </div>
                      <div style={{ fontSize: '11px', color: '#57606a' }}>
                        {commit.author} · {new Date(commit.date).toLocaleDateString()}
                      </div>
                    </div>
                  );
                })}
              </div>

              {/* Middle: Connection indicator */}
              <div style={styles.correlationDivider}>
                <div style={styles.correlationDividerLine} />
              </div>

              {/* Right column: Nodes with commits */}
              <div style={styles.correlationColumn}>
                <h4 style={styles.correlationColumnTitle}>Linked Decisions</h4>
                {graphData.nodes
                  .filter(n => getCommit(n))
                  .sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime())
                  .map(node => {
                    const nodeCommit = getCommit(node);
                    const isHovered = hoveredLink?.commit && (
                      nodeCommit === hoveredLink.commit ||
                      gitHistory.find(c => c.hash === hoveredLink.commit)?.short_hash === nodeCommit
                    );
                    return (
                      <div
                        key={node.id}
                        data-node={node.id}
                        onMouseEnter={() => setHoveredLink({ node: node.id, commit: nodeCommit || undefined })}
                        onMouseLeave={() => setHoveredLink(null)}
                        onClick={() => {
                          setShowCorrelationModal(false);
                          setSelectedNodeId(node.id);
                        }}
                        style={{
                          ...styles.correlationItem,
                          ...(isHovered ? styles.correlationItemHighlight : {}),
                          cursor: 'pointer',
                        }}
                      >
                        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                          <TypeBadge type={node.node_type} size="sm" />
                          <span style={{ fontFamily: 'monospace', fontSize: '11px', color: '#57606a' }}>
                            #{node.id}
                          </span>
                        </div>
                        <div style={{ fontSize: '13px', fontWeight: 500 }}>
                          {truncate(node.title, 35)}
                        </div>
                        <div style={{ fontSize: '11px', color: '#57606a' }}>
                          <CommitBadge commit={nodeCommit} />
                        </div>
                      </div>
                    );
                  })}
                {graphData.nodes.filter(n => getCommit(n)).length === 0 && (
                  <div style={{ textAlign: 'center', color: '#57606a', padding: '40px' }}>
                    No nodes have linked commits. Use <code>--commit HEAD</code> when adding nodes.
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Time Slider */}
      {showTimeSlider && (
        <div style={styles.timeSliderContainer}>
          <button
            onClick={() => {
              if (isPlaying) {
                setIsPlaying(false);
              } else {
                if (timeSliderValue >= 100) setTimeSliderValue(0);
                setIsPlaying(true);
              }
            }}
            style={styles.playButton}
            title={isPlaying ? 'Pause' : 'Play'}
          >
            {isPlaying ? '⏸' : '▶'}
          </button>

          <div style={styles.speedButtons}>
            {[0.5, 1, 2].map(speed => (
              <button
                key={speed}
                onClick={() => setPlaybackSpeed(speed)}
                style={{
                  ...styles.speedButton,
                  ...(playbackSpeed === speed ? styles.speedButtonActive : {}),
                }}
              >
                {speed}x
              </button>
            ))}
          </div>

          <input
            type="range"
            min="0"
            max="100"
            value={timeSliderValue}
            onChange={e => setTimeSliderValue(Number(e.target.value))}
            style={styles.timeSlider}
          />

          <div style={styles.timeLabel}>
            {new Date(currentTime).toLocaleDateString('en-US', {
              month: 'short',
              day: 'numeric',
              year: 'numeric',
            })}
          </div>

          <div style={styles.timeNodeCount}>
            {timeFilteredNodeIds?.size ?? graphData.nodes.length} / {graphData.nodes.length} nodes
          </div>
        </div>
      )}

      {/* Ask Claude Modal */}
      <AskResponseModal
        isOpen={askModalOpen}
        content={askResponse || ''}
        question={askQuestion}
        onClose={() => {
          setAskModalOpen(false);
          setAskQuestion('');
        }}
        isLoading={askLoading}
      />
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
  copyLinkBtn: {
    marginLeft: '12px',
    padding: '6px 12px',
    backgroundColor: '#ddf4ff',
    border: '1px solid #54aeff',
    borderRadius: '6px',
    color: '#0969da',
    fontSize: '12px',
    fontWeight: 500,
    cursor: 'pointer',
    transition: 'background-color 0.15s',
  },
  // Ask Claude button styles
  askButton: {
    marginLeft: '12px',
    padding: '8px 16px',
    backgroundColor: '#0969da',
    border: 'none',
    borderRadius: '6px',
    color: '#ffffff',
    fontSize: '13px',
    fontWeight: 600,
    cursor: 'pointer',
    transition: 'background-color 0.15s',
    boxShadow: '0 1px 3px rgba(9, 105, 218, 0.3)',
  },
  askInputContainer: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    marginLeft: '12px',
  },
  askInput: {
    width: '280px',
    padding: '8px 12px',
    border: '2px solid #0969da',
    borderRadius: '6px',
    fontSize: '13px',
    color: '#24292f',
    outline: 'none',
    backgroundColor: '#ffffff',
  },
  askSubmitBtn: {
    padding: '8px 14px',
    backgroundColor: '#0969da',
    border: 'none',
    borderRadius: '6px',
    color: '#ffffff',
    fontSize: '13px',
    fontWeight: 600,
    cursor: 'pointer',
  },
  askCancelBtn: {
    padding: '6px 10px',
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    color: '#57606a',
    fontSize: '16px',
    cursor: 'pointer',
    lineHeight: 1,
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
    // Prevent browser zoom gestures from interfering with graph zoom
    touchAction: 'none',
    overflow: 'hidden',
  },
  svg: {
    width: '100%',
    height: '100%',
    // Prevent touch events from triggering page zoom
    touchAction: 'none',
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
  modalBackBtn: {
    padding: '4px 10px',
    border: 'none',
    background: '#0969da',
    color: '#ffffff',
    borderRadius: '6px',
    fontSize: '12px',
    fontWeight: 500,
    cursor: 'pointer',
    transition: 'background-color 0.15s',
    marginRight: '8px',
  },
  modalBreadcrumb: {
    padding: '8px 0 12px 0',
    fontSize: '12px',
    color: '#57606a',
    borderBottom: '1px solid #d0d7de',
    marginBottom: '12px',
  },
  breadcrumbItem: {
    display: 'inline',
  },
  breadcrumbLink: {
    color: '#0969da',
    cursor: 'pointer',
    textDecoration: 'none',
  },
  breadcrumbCurrent: {
    color: '#24292f',
    fontWeight: 500,
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
    fontSize: '13px',
    color: '#24292f',
    transition: 'background-color 0.15s',
    border: '1px solid #d0d7de',
  },
  connectionWrapper: {
    marginBottom: '6px',
  },
  expandArrow: {
    background: 'none',
    border: 'none',
    cursor: 'pointer',
    padding: '2px 6px',
    fontSize: '10px',
    color: '#57606a',
    borderRadius: '3px',
    transition: 'background-color 0.15s',
    flexShrink: 0,
  },
  expandArrowDisabled: {
    color: '#d0d7de',
    cursor: 'default',
  },
  connectionTitle: {
    cursor: 'pointer',
    flex: 1,
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap' as const,
  },
  expandedDescription: {
    padding: '10px 12px 10px 36px',
    backgroundColor: '#ffffff',
    borderLeft: '3px solid #d0d7de',
    marginLeft: '12px',
    marginTop: '-1px',
    marginBottom: '0',
    fontSize: '12px',
    color: '#57606a',
    lineHeight: '1.5',
    whiteSpace: 'pre-wrap' as const,
    borderBottomLeftRadius: '6px',
    borderBottomRightRadius: '6px',
    border: '1px solid #d0d7de',
    borderTop: 'none',
  },
  // Prompt section styles
  promptSection: {
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    padding: '16px',
    borderRadius: '8px',
    marginBottom: '16px',
    borderLeft: '3px solid #0969da',
  },
  promptTitle: {
    fontSize: '12px',
    color: '#57606a',
    margin: '0 0 10px 0',
    textTransform: 'uppercase',
    fontWeight: 600,
  },
  promptText: {
    fontSize: '14px',
    color: '#24292f',
    lineHeight: 1.6,
    whiteSpace: 'pre-wrap',
    fontStyle: 'italic',
  },
  // Files section styles
  filesSection: {
    marginBottom: '16px',
  },
  filesList: {
    display: 'flex',
    flexWrap: 'wrap',
    gap: '8px',
  },
  fileTag: {
    backgroundColor: '#ddf4ff',
    padding: '4px 10px',
    borderRadius: '4px',
    fontSize: '12px',
    color: '#0969da',
    fontFamily: 'monospace',
  },
  // Branch section styles
  branchSection: {
    marginBottom: '16px',
  },
  branchTag: {
    backgroundColor: '#dafbe1',
    color: '#1a7f37',
    padding: '4px 10px',
    borderRadius: '4px',
    fontSize: '12px',
    fontFamily: 'monospace',
  },
  // Git Log Modal styles
  gitLogFilterBtn: {
    padding: '4px 12px',
    fontSize: '12px',
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    borderRadius: '4px',
    cursor: 'pointer',
    color: '#24292f',
  },
  gitLogFilterBtnActive: {
    backgroundColor: '#0969da',
    color: '#fff',
    borderColor: '#0969da',
  },
  gitLogItem: {
    position: 'relative',
    paddingLeft: '28px',
    marginBottom: '16px',
  },
  gitLogLine: {
    position: 'absolute',
    left: '6px',
    top: '18px',
    bottom: '-16px',
    width: '2px',
    backgroundColor: '#d0d7de',
  },
  gitLogDot: {
    position: 'absolute',
    left: '0',
    top: '4px',
    width: '14px',
    height: '14px',
    borderRadius: '50%',
    backgroundColor: '#3b82f6',
    border: '2px solid #fff',
    boxShadow: '0 0 0 1px #d0d7de',
  },
  gitLogCommit: {
    fontSize: '14px',
    fontWeight: 500,
    color: '#24292f',
    display: 'flex',
    alignItems: 'center',
    flexWrap: 'wrap',
    gap: '4px',
  },
  gitLogHeadBadge: {
    backgroundColor: '#ddf4ff',
    color: '#0969da',
    padding: '2px 6px',
    borderRadius: '4px',
    fontSize: '10px',
    fontWeight: 600,
    marginLeft: '6px',
  },
  gitLogMeta: {
    fontSize: '12px',
    color: '#57606a',
    marginTop: '4px',
  },
  gitLogLinkedNode: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '8px 12px',
    marginTop: '6px',
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    cursor: 'pointer',
    fontSize: '13px',
    transition: 'background-color 0.15s, border-color 0.15s',
  },
  // Correlation View styles
  correlationContainer: {
    display: 'flex',
    height: '500px',
    gap: '0',
  },
  correlationColumn: {
    flex: 1,
    overflowY: 'auto',
    padding: '16px',
    borderRight: '1px solid #d0d7de',
  },
  correlationColumnTitle: {
    fontSize: '12px',
    color: '#57606a',
    textTransform: 'uppercase',
    fontWeight: 600,
    marginBottom: '12px',
    marginTop: 0,
  },
  correlationDivider: {
    width: '60px',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    flexShrink: 0,
  },
  correlationDividerLine: {
    width: '2px',
    height: '100%',
    backgroundColor: '#d0d7de',
    backgroundImage: 'repeating-linear-gradient(180deg, #0969da 0, #0969da 4px, transparent 4px, transparent 8px)',
  },
  correlationItem: {
    padding: '12px',
    marginBottom: '8px',
    backgroundColor: '#f6f8fa',
    borderRadius: '6px',
    border: '1px solid #d0d7de',
    transition: 'background-color 0.15s, border-color 0.15s',
  },
  correlationItemHighlight: {
    backgroundColor: '#ddf4ff',
    borderColor: '#0969da',
  },
  // Time Slider styles
  timeSliderContainer: {
    position: 'absolute',
    bottom: 0,
    left: 0,
    right: 0,
    padding: '12px 20px',
    backgroundColor: 'rgba(246, 248, 250, 0.95)',
    borderTop: '1px solid #d0d7de',
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    zIndex: 15,
    backdropFilter: 'blur(8px)',
  },
  playButton: {
    width: '36px',
    height: '36px',
    borderRadius: '50%',
    border: 'none',
    backgroundColor: '#2da44e',
    color: '#fff',
    fontSize: '14px',
    cursor: 'pointer',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    flexShrink: 0,
  },
  speedButtons: {
    display: 'flex',
    gap: '4px',
  },
  speedButton: {
    padding: '4px 8px',
    fontSize: '11px',
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    borderRadius: '4px',
    cursor: 'pointer',
    color: '#24292f',
  },
  speedButtonActive: {
    backgroundColor: '#0969da',
    color: '#fff',
    borderColor: '#0969da',
  },
  timeSlider: {
    flex: 1,
    height: '6px',
    cursor: 'pointer',
    accentColor: '#0969da',
  },
  timeLabel: {
    fontSize: '12px',
    color: '#24292f',
    fontWeight: 500,
    minWidth: '100px',
    textAlign: 'right',
  },
  timeNodeCount: {
    fontSize: '11px',
    color: '#57606a',
    minWidth: '80px',
    textAlign: 'right',
  },
  // Card Stack styles - visual parent/child navigation
  cardStackBackdrop: {
    position: 'fixed',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    backgroundColor: 'rgba(0, 0, 0, 0.4)',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    zIndex: 200,
    backdropFilter: 'blur(2px)',
  },
  cardStackContainer: {
    display: 'flex',
    flexDirection: 'column',
    gap: '0',
    maxHeight: '85vh',
    overflowY: 'auto',
    padding: '20px',
    width: '95%',
    maxWidth: '800px',
  },
  stackCard: {
    backgroundColor: '#ffffff',
    borderRadius: '12px',
    padding: '16px 20px',
    marginBottom: '4px',
    boxShadow: '0 4px 20px rgba(0, 0, 0, 0.15)',
    border: '1px solid #d0d7de',
    position: 'relative',
    // Animation applied via style prop based on relation
    transition: 'transform 0.25s ease-out, opacity 0.25s ease-out, box-shadow 0.2s ease',
  },
  stackCardHeader: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: '12px',
  },
  stackCardHeaderLeft: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    flexWrap: 'wrap',
  },
  stackCardClose: {
    width: '28px',
    height: '28px',
    border: 'none',
    background: '#f6f8fa',
    color: '#57606a',
    borderRadius: '6px',
    fontSize: '18px',
    cursor: 'pointer',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    transition: 'background-color 0.15s, color 0.15s',
    flexShrink: 0,
  },
  relationBadgeParent: {
    backgroundColor: '#f5f0ff',
    color: '#8250df',
    padding: '3px 8px',
    borderRadius: '4px',
    fontSize: '10px',
    fontWeight: 700,
    textTransform: 'uppercase',
    letterSpacing: '0.5px',
  },
  relationBadgeChild: {
    backgroundColor: '#dafbe1',
    color: '#1a7f37',
    padding: '3px 8px',
    borderRadius: '4px',
    fontSize: '10px',
    fontWeight: 700,
    textTransform: 'uppercase',
    letterSpacing: '0.5px',
  },
  relationBadgeRoot: {
    backgroundColor: '#ddf4ff',
    color: '#0969da',
    padding: '3px 8px',
    borderRadius: '4px',
    fontSize: '10px',
    fontWeight: 700,
    textTransform: 'uppercase',
    letterSpacing: '0.5px',
  },
  stackCardTitle: {
    fontSize: '18px',
    fontWeight: 600,
    margin: '0 0 6px 0',
    color: '#24292f',
    lineHeight: 1.3,
  },
  stackCardMeta: {
    fontSize: '12px',
    color: '#57606a',
    margin: '0 0 12px 0',
  },
  stackCardDescription: {
    fontSize: '14px',
    color: '#24292f',
    lineHeight: 1.6,
    margin: '0 0 12px 0',
    padding: '10px 12px',
    backgroundColor: '#f6f8fa',
    borderRadius: '6px',
    borderLeft: '3px solid #d0d7de',
    whiteSpace: 'pre-wrap',
    wordBreak: 'break-word',
  },
  stackCardPrompt: {
    fontSize: '13px',
    color: '#57606a',
    fontStyle: 'italic',
    padding: '12px 14px',
    backgroundColor: '#fffbeb',
    borderRadius: '6px',
    borderLeft: '3px solid #f59e0b',
    marginBottom: '12px',
    lineHeight: 1.6,
    whiteSpace: 'pre-wrap',
    wordBreak: 'break-word',
    maxHeight: '400px',
    overflowY: 'auto',
  },
  stackCardCommit: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    padding: '10px 12px',
    backgroundColor: '#f6f8fa',
    borderRadius: '6px',
    marginBottom: '12px',
    flexWrap: 'wrap',
  },
  stackCardCommitMsg: {
    fontSize: '12px',
    color: '#57606a',
    fontStyle: 'italic',
  },
  stackCardConnections: {
    marginTop: '12px',
    padding: '12px',
    backgroundColor: '#f6f8fa',
    borderRadius: '8px',
    border: '1px solid #e1e4e8',
  },
  stackCardConnectionLabel: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#57606a',
    textTransform: 'uppercase',
    marginBottom: '8px',
    display: 'block',
    letterSpacing: '0.5px',
  },
  stackCardConnectionList: {
    display: 'flex',
    flexDirection: 'column',
    gap: '6px',
    maxHeight: '200px',
    overflowY: 'auto',
    paddingRight: '4px',
  },
  stackCardConnectionBtn: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '8px 12px',
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    cursor: 'pointer',
    fontSize: '13px',
    color: '#24292f',
    textAlign: 'left',
    transition: 'background-color 0.15s, border-color 0.15s, transform 0.1s',
    width: '100%',
  },
  stackCardConnectionBtnActive: {
    backgroundColor: '#ddf4ff',
    borderColor: '#0969da',
    color: '#0969da',
    cursor: 'default',
  },
  stackCardMore: {
    fontSize: '12px',
    color: '#57606a',
    fontStyle: 'italic',
    padding: '4px 0',
  },
  stackFooter: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: '12px 16px',
    backgroundColor: '#f6f8fa',
    borderRadius: '8px',
    marginTop: '8px',
    fontSize: '12px',
    color: '#57606a',
  },
  stackClearBtn: {
    padding: '6px 12px',
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    color: '#cf222e',
    fontSize: '12px',
    cursor: 'pointer',
    transition: 'background-color 0.15s, border-color 0.15s',
  },
};
