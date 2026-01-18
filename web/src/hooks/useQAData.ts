/**
 * Q&A Data Hook
 *
 * Fetches and manages Q&A interaction data with search support.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import type { QaInteraction, QaSearchResult, QaListResponse } from '../types/qa';

interface UseQADataOptions {
  /** Initial search query */
  searchQuery?: string;
  /** Pagination offset */
  offset?: number;
  /** Pagination limit */
  limit?: number;
  /** Debounce delay for search in ms */
  debounceMs?: number;
}

interface UseQADataResult {
  /** Q&A items (either search results or paginated list) */
  items: QaInteraction[];
  /** Search results with ranking (only when searching) */
  searchResults: QaSearchResult[] | null;
  /** Total count of Q&A interactions */
  total: number;
  /** Whether data is loading */
  loading: boolean;
  /** Error message if any */
  error: string | null;
  /** Whether we're in search mode */
  isSearchMode: boolean;
  /** Current search query */
  searchQuery: string;
  /** Set search query (triggers debounced search) */
  setSearchQuery: (query: string) => void;
  /** Pagination offset */
  offset: number;
  /** Set pagination offset */
  setOffset: (offset: number) => void;
  /** Refresh data */
  refresh: () => Promise<void>;
  /** Soft delete a Q&A interaction */
  deleteItem: (id: number) => Promise<boolean>;
}

/**
 * Hook for loading and managing Q&A data with search
 */
export function useQAData(options: UseQADataOptions = {}): UseQADataResult {
  const {
    searchQuery: initialQuery = '',
    offset: initialOffset = 0,
    limit = 20,
    debounceMs = 300,
  } = options;

  const [items, setItems] = useState<QaInteraction[]>([]);
  const [searchResults, setSearchResults] = useState<QaSearchResult[] | null>(null);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQueryState] = useState(initialQuery);
  const [debouncedQuery, setDebouncedQuery] = useState(initialQuery);
  const [offset, setOffset] = useState(initialOffset);

  const abortControllerRef = useRef<AbortController | null>(null);

  // Debounce search query
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedQuery(searchQuery);
      // Reset offset when search changes
      if (searchQuery !== debouncedQuery) {
        setOffset(0);
      }
    }, debounceMs);

    return () => clearTimeout(timer);
  }, [searchQuery, debounceMs, debouncedQuery]);

  const setSearchQuery = useCallback((query: string) => {
    setSearchQueryState(query);
  }, []);

  const isSearchMode = debouncedQuery.trim().length > 0;

  /**
   * Fetch Q&A list (browse mode)
   */
  const fetchList = useCallback(async (signal: AbortSignal) => {
    const url = `/api/qa?offset=${offset}&limit=${limit}`;
    const response = await fetch(url, { signal });
    if (!response.ok) {
      throw new Error(`Failed to fetch Q&A list: ${response.status}`);
    }
    const json = await response.json();
    if (json.ok === false && json.error) {
      throw new Error(json.error);
    }
    const data: QaListResponse = json.data ?? json;
    return data;
  }, [offset, limit]);

  /**
   * Search Q&A interactions (search mode)
   */
  const fetchSearch = useCallback(async (query: string, signal: AbortSignal) => {
    const url = `/api/qa/search?q=${encodeURIComponent(query)}&limit=${limit}`;
    const response = await fetch(url, { signal });
    if (!response.ok) {
      throw new Error(`Failed to search Q&A: ${response.status}`);
    }
    const json = await response.json();
    if (json.ok === false && json.error) {
      throw new Error(json.error);
    }
    const results: QaSearchResult[] = json.data ?? json;
    return results;
  }, [limit]);

  /**
   * Fetch data based on current mode
   */
  const fetchData = useCallback(async () => {
    // Cancel any in-flight request
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    abortControllerRef.current = new AbortController();
    const { signal } = abortControllerRef.current;

    setLoading(true);
    setError(null);

    try {
      if (isSearchMode) {
        const results = await fetchSearch(debouncedQuery, signal);
        setSearchResults(results);
        setItems(results.map(r => r.interaction));
        setTotal(results.length);
      } else {
        const data = await fetchList(signal);
        setSearchResults(null);
        setItems(data.items);
        setTotal(data.total);
      }
    } catch (err) {
      if (err instanceof Error && err.name === 'AbortError') {
        return; // Ignore aborted requests
      }
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, [isSearchMode, debouncedQuery, fetchSearch, fetchList]);

  // Fetch data when dependencies change
  useEffect(() => {
    fetchData();
  }, [fetchData]);

  /**
   * Refresh data
   */
  const refresh = useCallback(async () => {
    await fetchData();
  }, [fetchData]);

  /**
   * Soft delete a Q&A interaction
   */
  const deleteItem = useCallback(async (id: number): Promise<boolean> => {
    try {
      const response = await fetch(`/api/qa/${id}`, { method: 'DELETE' });
      if (!response.ok) {
        const json = await response.json();
        throw new Error(json.error || `Failed to delete: ${response.status}`);
      }
      // Refresh the list after deletion
      await refresh();
      return true;
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete');
      return false;
    }
  }, [refresh]);

  return {
    items,
    searchResults,
    total,
    loading,
    error,
    isSearchMode,
    searchQuery,
    setSearchQuery,
    offset,
    setOffset,
    refresh,
    deleteItem,
  };
}
