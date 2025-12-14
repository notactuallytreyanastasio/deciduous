/**
 * Graph Data Hook
 *
 * Fetches graph data and optionally subscribes to SSE for live updates.
 */

import { useState, useEffect, useCallback } from 'react';
import type { GraphData, GitCommit } from '../types/graph';
import type { RoadmapItem } from '../types/generated/schema';

interface UseGraphDataOptions {
  /** Path to graph-data.json (default: '/graph-data.json' or '/api/graph') */
  graphUrl?: string;
  /** Path to git-history.json (optional, for timeline view) */
  gitHistoryUrl?: string;
  /** Path to roadmap-items.json (optional, for roadmap view) */
  roadmapUrl?: string;
  /** Enable SSE live updates (requires deciduous serve) */
  enableSSE?: boolean;
  /** SSE endpoint (default: '/api/events') */
  sseUrl?: string;
  /** Polling interval in milliseconds (0 to disable, default: 0) */
  pollInterval?: number;
}

interface UseGraphDataResult {
  graphData: GraphData | null;
  gitHistory: GitCommit[];
  roadmapItems: RoadmapItem[];
  loading: boolean;
  error: string | null;
  lastUpdated: Date | null;
  refresh: () => Promise<void>;
}

/**
 * Hook for loading and managing graph data
 *
 * Usage:
 * ```tsx
 * const { graphData, loading, error, refresh } = useGraphData({
 *   enableSSE: true,  // Enable live updates
 * });
 * ```
 */
export function useGraphData(options: UseGraphDataOptions = {}): UseGraphDataResult {
  const {
    graphUrl = detectGraphUrl(),
    gitHistoryUrl,
    roadmapUrl,
    enableSSE = false,
    sseUrl = '/api/events',
    pollInterval = 0,
  } = options;

  const [graphData, setGraphData] = useState<GraphData | null>(null);
  const [gitHistory, setGitHistory] = useState<GitCommit[]>([]);
  const [roadmapItems, setRoadmapItems] = useState<RoadmapItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);

  /**
   * Fetch graph data from the server or static JSON
   */
  const fetchGraph = useCallback(async () => {
    try {
      const response = await fetch(graphUrl);
      if (!response.ok) {
        throw new Error(`Failed to fetch graph: ${response.status} ${response.statusText}`);
      }
      const json = await response.json();
      // Handle both API response {ok, data, error} and direct GraphData format
      const data: GraphData = json.data ?? json;
      if (json.ok === false && json.error) {
        throw new Error(json.error);
      }
      setGraphData(data);
      setLastUpdated(new Date());
      setError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to load graph data';
      setError(message);
      console.error('Graph fetch error:', err);
    }
  }, [graphUrl]);

  /**
   * Fetch git history (optional, for timeline view)
   */
  const fetchGitHistory = useCallback(async () => {
    if (!gitHistoryUrl) return;

    try {
      const response = await fetch(gitHistoryUrl);
      if (response.ok) {
        const data = await response.json() as GitCommit[];
        setGitHistory(data);
      }
      // Don't treat missing git history as an error
    } catch (err) {
      console.warn('Could not load git history:', err);
    }
  }, [gitHistoryUrl]);

  /**
   * Fetch roadmap items (optional, for roadmap view)
   */
  const fetchRoadmap = useCallback(async () => {
    if (!roadmapUrl) return;

    try {
      const response = await fetch(roadmapUrl);
      if (response.ok) {
        const json = await response.json();
        // Handle both API response {ok, data, error} and direct array format
        const data: RoadmapItem[] = json.data ?? json;
        setRoadmapItems(data);
      }
      // Don't treat missing roadmap as an error
    } catch (err) {
      console.warn('Could not load roadmap items:', err);
    }
  }, [roadmapUrl]);

  /**
   * Refresh data manually
   */
  const refresh = useCallback(async () => {
    setLoading(true);
    await Promise.all([fetchGraph(), fetchGitHistory(), fetchRoadmap()]);
    setLoading(false);
  }, [fetchGraph, fetchGitHistory, fetchRoadmap]);

  /**
   * Initial data load
   */
  useEffect(() => {
    let mounted = true;

    const load = async () => {
      setLoading(true);
      await Promise.all([fetchGraph(), fetchGitHistory(), fetchRoadmap()]);
      if (mounted) setLoading(false);
    };

    load();

    return () => {
      mounted = false;
    };
  }, [fetchGraph, fetchGitHistory, fetchRoadmap]);

  /**
   * SSE subscription for live updates
   */
  useEffect(() => {
    if (!enableSSE) return;

    let eventSource: EventSource | null = null;
    let reconnectTimeout: ReturnType<typeof setTimeout> | null = null;

    const connect = () => {
      try {
        eventSource = new EventSource(sseUrl);

        eventSource.onmessage = (event) => {
          if (event.data === 'refresh' || event.data === 'update') {
            // Re-fetch graph data when server signals a change
            fetchGraph();
          }
        };

        eventSource.onerror = () => {
          // Connection lost, try to reconnect
          eventSource?.close();
          reconnectTimeout = setTimeout(connect, 5000);
        };

        eventSource.onopen = () => {
          console.log('SSE connected for live updates');
        };
      } catch (err) {
        console.warn('SSE not available:', err);
      }
    };

    connect();

    return () => {
      eventSource?.close();
      if (reconnectTimeout) clearTimeout(reconnectTimeout);
    };
  }, [enableSSE, sseUrl, fetchGraph]);

  /**
   * Polling for auto-refresh (used when SSE is not available)
   */
  useEffect(() => {
    if (pollInterval <= 0) return;

    const intervalId = setInterval(() => {
      fetchGraph();
    }, pollInterval);

    return () => clearInterval(intervalId);
  }, [pollInterval, fetchGraph]);

  return {
    graphData,
    gitHistory,
    roadmapItems,
    loading,
    error,
    lastUpdated,
    refresh,
  };
}

/**
 * Detect the appropriate graph URL based on environment
 */
function detectGraphUrl(): string {
  // In development, use the static file from public folder
  // In production on GitHub Pages, use relative path
  return './graph-data.json';
}

/**
 * Hook variant that just returns loading state for suspense boundaries
 */
export function useGraphDataStatus(): { loading: boolean; error: string | null } {
  const { loading, error } = useGraphData();
  return { loading, error };
}
