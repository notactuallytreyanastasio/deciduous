/**
 * Main Application Component
 *
 * Sets up routing and data loading for the unified graph viewer.
 */

import React, { useState, useMemo } from 'react';
import { HashRouter, Routes, Route, Navigate } from 'react-router-dom';
import { useGraphData } from './hooks/useGraphData';
import { useChains } from './hooks/useChains';
import { Layout } from './components/Layout';
import { ChainsView } from './views/ChainsView';
import { TimelineView } from './views/TimelineView';
import { GraphView } from './views/GraphView';
import { DagView } from './views/DagView';
import { RoadmapView } from './views/RoadmapView';
import { getUniqueBranches, getBranch, type GraphData } from './types/graph';

// Detect if running from deciduous serve (localhost) vs static file (GitHub Pages)
const isLocalServer = typeof window !== 'undefined' &&
  (window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1');

export const App: React.FC = () => {
  // Load graph data with optional SSE for live updates
  // Use /api/graph when running from deciduous serve, static file for GitHub Pages
  // Enable 30-second auto-refresh when running locally (deciduous serve)
  const {
    graphData,
    gitHistory,
    roadmapItems,
    loading,
    error,
    lastUpdated,
  } = useGraphData({
    graphUrl: isLocalServer ? '/api/graph' : './graph-data.json',
    gitHistoryUrl: './git-history.json',
    roadmapUrl: isLocalServer ? '/api/roadmap' : './roadmap-items.json',
    enableSSE: false, // Disable SSE until deciduous serve is implemented
    pollInterval: isLocalServer ? 30000 : 0, // 30-second refresh for local server only
  });

  // Branch filter state
  const [selectedBranch, setSelectedBranch] = useState<string | null>(null);

  // Get unique branches from all nodes
  const branches = useMemo(() => {
    if (!graphData) return [];
    return getUniqueBranches(graphData.nodes);
  }, [graphData]);

  // Filter graph data by selected branch
  const filteredGraphData = useMemo((): GraphData | null => {
    if (!graphData) return null;
    if (!selectedBranch) return graphData;

    const filteredNodes = graphData.nodes.filter(node => getBranch(node) === selectedBranch);
    const nodeIds = new Set(filteredNodes.map(n => n.id));
    const filteredEdges = graphData.edges.filter(
      edge => nodeIds.has(edge.from_node_id) && nodeIds.has(edge.to_node_id)
    );

    return { nodes: filteredNodes, edges: filteredEdges };
  }, [graphData, selectedBranch]);

  // Compute chains and sessions from filtered data
  const { chains, sessions, stats } = useChains(filteredGraphData);

  // Loading state
  if (loading) {
    return (
      <div style={styles.loading}>
        <div style={styles.spinner} />
        <p>Loading decision graph...</p>
      </div>
    );
  }

  // Error state
  if (error) {
    return (
      <div style={styles.error}>
        <h2>Error Loading Graph</h2>
        <p>{error}</p>
        <p style={styles.hint}>
          Make sure graph-data.json exists, or run <code>deciduous serve</code> for live data.
        </p>
      </div>
    );
  }

  // No data
  if (!graphData || graphData.nodes.length === 0) {
    return (
      <div style={styles.empty}>
        <h2>No Decision Data</h2>
        <p>The graph is empty. Start adding decisions!</p>
        <pre style={styles.code}>
          deciduous add goal "My first goal" -c 90
        </pre>
      </div>
    );
  }

  return (
    <HashRouter>
      <Layout
        stats={stats}
        lastUpdated={lastUpdated}
        branches={branches}
        selectedBranch={selectedBranch}
        onBranchChange={setSelectedBranch}
      >
        <Routes>
          <Route
            path="/"
            element={
              <DagView
                graphData={filteredGraphData!}
                chains={chains}
                gitHistory={gitHistory}
              />
            }
          />
          <Route
            path="/chains"
            element={
              <ChainsView
                graphData={filteredGraphData!}
                chains={chains}
                sessions={sessions}
                gitHistory={gitHistory}
              />
            }
          />
          <Route
            path="/timeline"
            element={
              <TimelineView
                graphData={filteredGraphData!}
                gitHistory={gitHistory}
              />
            }
          />
          <Route
            path="/graph"
            element={
              <GraphView graphData={filteredGraphData!} />
            }
          />
          <Route
            path="/roadmap"
            element={
              <RoadmapView graphData={filteredGraphData!} roadmapItems={roadmapItems} />
            }
          />
          {/* Fallback redirect */}
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </Layout>
    </HashRouter>
  );
};

// =============================================================================
// Styles
// =============================================================================

const styles: Record<string, React.CSSProperties> = {
  loading: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    height: '100vh',
    backgroundColor: '#ffffff',
    color: '#57606a',
  },
  spinner: {
    width: '40px',
    height: '40px',
    border: '3px solid #d0d7de',
    borderTopColor: '#0969da',
    borderRadius: '50%',
    animation: 'spin 1s linear infinite',
    marginBottom: '20px',
  },
  error: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    height: '100vh',
    backgroundColor: '#ffffff',
    color: '#24292f',
    textAlign: 'center',
    padding: '20px',
  },
  hint: {
    color: '#57606a',
    fontSize: '14px',
  },
  empty: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    height: '100vh',
    backgroundColor: '#ffffff',
    color: '#24292f',
    textAlign: 'center',
    padding: '20px',
  },
  code: {
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    padding: '15px 20px',
    borderRadius: '8px',
    fontFamily: 'monospace',
    fontSize: '14px',
    color: '#0969da',
    marginTop: '10px',
  },
};

export default App;
