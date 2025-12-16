/**
 * URL State Hook
 *
 * Syncs view state with URL query parameters for deep linking and sharing.
 * Supports: node selection, search query, view mode, chain count, fullscreen.
 */

import { useState, useEffect, useCallback } from 'react';

export type ViewMode = 'recent' | 'all' | 'single';

export interface UrlState {
  selectedNodeId: number | null;
  searchQuery: string;
  viewMode: ViewMode;
  recentChainCount: number;
  focusChainIndex: number | null;
  isFullscreen: boolean;
}

const DEFAULT_CHAIN_COUNT = 8;

/**
 * Parse URL query parameters into state object
 */
function parseUrlParams(): UrlState {
  if (typeof window === 'undefined') {
    return getDefaultState();
  }

  const params = new URLSearchParams(window.location.search);

  const nodeParam = params.get('node');
  const searchParam = params.get('search') || params.get('q');
  const viewParam = params.get('view');
  const chainsParam = params.get('chains');
  const focusParam = params.get('focus');
  const fullscreenParam = params.get('fullscreen');

  return {
    selectedNodeId: nodeParam ? parseInt(nodeParam, 10) : null,
    searchQuery: searchParam || '',
    viewMode: (viewParam === 'all' || viewParam === 'single' || viewParam === 'recent')
      ? viewParam
      : 'recent',
    recentChainCount: chainsParam ? parseInt(chainsParam, 10) : DEFAULT_CHAIN_COUNT,
    focusChainIndex: focusParam ? parseInt(focusParam, 10) : null,
    isFullscreen: fullscreenParam === '1' || fullscreenParam === 'true',
  };
}

/**
 * Get default state values
 */
function getDefaultState(): UrlState {
  return {
    selectedNodeId: null,
    searchQuery: '',
    viewMode: 'recent',
    recentChainCount: DEFAULT_CHAIN_COUNT,
    focusChainIndex: null,
    isFullscreen: typeof window !== 'undefined' && window.innerWidth >= 768,
  };
}

/**
 * Serialize state to URL query string
 */
function stateToQueryString(state: UrlState): string {
  const params = new URLSearchParams();

  if (state.selectedNodeId !== null) {
    params.set('node', String(state.selectedNodeId));
  }
  if (state.searchQuery) {
    params.set('search', state.searchQuery);
  }
  if (state.viewMode !== 'recent') {
    params.set('view', state.viewMode);
  }
  if (state.recentChainCount !== DEFAULT_CHAIN_COUNT) {
    params.set('chains', String(state.recentChainCount));
  }
  if (state.focusChainIndex !== null) {
    params.set('focus', String(state.focusChainIndex));
  }
  if (state.isFullscreen) {
    params.set('fullscreen', '1');
  }

  return params.toString();
}

/**
 * Update URL without triggering navigation
 */
function updateUrl(state: UrlState) {
  if (typeof window === 'undefined') return;

  const queryString = stateToQueryString(state);
  const newUrl = queryString
    ? `${window.location.pathname}?${queryString}`
    : window.location.pathname;

  // Use replaceState to avoid polluting browser history
  window.history.replaceState({}, '', newUrl);
}

export interface UseUrlStateResult {
  state: UrlState;
  setSelectedNodeId: (id: number | null) => void;
  setSearchQuery: (query: string) => void;
  setViewMode: (mode: ViewMode) => void;
  setRecentChainCount: (count: number) => void;
  setFocusChainIndex: (index: number | null) => void;
  setIsFullscreen: (fullscreen: boolean) => void;
  copyLinkToClipboard: () => Promise<void>;
  getShareableUrl: () => string;
}

/**
 * Hook to sync view state with URL parameters
 *
 * Usage:
 * ```tsx
 * const {
 *   state,
 *   setSelectedNodeId,
 *   setSearchQuery,
 *   copyLinkToClipboard,
 * } = useUrlState();
 * ```
 */
export function useUrlState(): UseUrlStateResult {
  const [state, setState] = useState<UrlState>(getDefaultState);

  // Initialize from URL on mount
  useEffect(() => {
    const initialState = parseUrlParams();
    setState(initialState);
  }, []);

  // Update URL when state changes
  useEffect(() => {
    updateUrl(state);
  }, [state]);

  // Handle browser back/forward navigation
  useEffect(() => {
    const handlePopState = () => {
      setState(parseUrlParams());
    };

    window.addEventListener('popstate', handlePopState);
    return () => window.removeEventListener('popstate', handlePopState);
  }, []);

  const setSelectedNodeId = useCallback((id: number | null) => {
    setState(prev => ({ ...prev, selectedNodeId: id }));
  }, []);

  const setSearchQuery = useCallback((query: string) => {
    setState(prev => ({ ...prev, searchQuery: query }));
  }, []);

  const setViewMode = useCallback((mode: ViewMode) => {
    setState(prev => ({ ...prev, viewMode: mode }));
  }, []);

  const setRecentChainCount = useCallback((count: number) => {
    setState(prev => ({ ...prev, recentChainCount: count }));
  }, []);

  const setFocusChainIndex = useCallback((index: number | null) => {
    setState(prev => ({ ...prev, focusChainIndex: index }));
  }, []);

  const setIsFullscreen = useCallback((fullscreen: boolean) => {
    setState(prev => ({ ...prev, isFullscreen: fullscreen }));
  }, []);

  const getShareableUrl = useCallback((): string => {
    if (typeof window === 'undefined') return '';
    const queryString = stateToQueryString(state);
    return queryString
      ? `${window.location.origin}${window.location.pathname}?${queryString}`
      : `${window.location.origin}${window.location.pathname}`;
  }, [state]);

  const copyLinkToClipboard = useCallback(async () => {
    const url = getShareableUrl();
    try {
      await navigator.clipboard.writeText(url);
    } catch (err) {
      // Fallback for older browsers
      const textArea = document.createElement('textarea');
      textArea.value = url;
      document.body.appendChild(textArea);
      textArea.select();
      document.execCommand('copy');
      document.body.removeChild(textArea);
    }
  }, [getShareableUrl]);

  return {
    state,
    setSelectedNodeId,
    setSearchQuery,
    setViewMode,
    setRecentChainCount,
    setFocusChainIndex,
    setIsFullscreen,
    copyLinkToClipboard,
    getShareableUrl,
  };
}

export default useUrlState;
