/**
 * SearchBar Component
 *
 * Full-text search across graph nodes with dropdown results.
 * Searches: titles, descriptions, commit messages, prompts.
 */

import React, { useState, useEffect, useRef, useMemo, useCallback } from 'react';
import type { DecisionNode, GitCommit } from '../types/graph';
import { parseMetadata, truncate, shortCommit } from '../types/graph';
// TypeBadge and ConfidenceBadge available if needed for richer result display
// import { TypeBadge, ConfidenceBadge } from './NodeBadge';
import { getNodeColor } from '../utils/colors';

interface SearchResult {
  node: DecisionNode;
  matchType: 'title' | 'description' | 'commit' | 'prompt' | 'files';
  matchText: string;
  commitInfo?: GitCommit;
}

interface SearchBarProps {
  nodes: DecisionNode[];
  gitHistory?: GitCommit[];
  onSelectNode: (node: DecisionNode) => void;
  onHighlightNodes: (nodeIds: Set<number>) => void;
  placeholder?: string;
  /** Controlled query value (for URL state sync) */
  query?: string;
  /** Callback when query changes (for URL state sync) */
  onQueryChange?: (query: string) => void;
}

// Debounce hook
function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value);

  useEffect(() => {
    const handler = setTimeout(() => {
      setDebouncedValue(value);
    }, delay);

    return () => {
      clearTimeout(handler);
    };
  }, [value, delay]);

  return debouncedValue;
}

// Build a map from commit hash to GitCommit for fast lookup
function buildCommitMap(gitHistory: GitCommit[]): Map<string, GitCommit> {
  const map = new Map<string, GitCommit>();
  for (const commit of gitHistory) {
    map.set(commit.hash, commit);
    map.set(commit.short_hash, commit);
    // Also store by first 7 chars for partial matches
    if (commit.hash.length >= 7) {
      map.set(commit.hash.slice(0, 7), commit);
    }
  }
  return map;
}

export const SearchBar: React.FC<SearchBarProps> = ({
  nodes,
  gitHistory = [],
  onSelectNode,
  onHighlightNodes,
  placeholder = 'Search nodes, commits, prompts...',
  query: controlledQuery,
  onQueryChange,
}) => {
  // Support both controlled and uncontrolled modes
  const [internalQuery, setInternalQuery] = useState('');
  const isControlled = controlledQuery !== undefined;
  const query = isControlled ? controlledQuery : internalQuery;
  const setQuery = isControlled ? (onQueryChange || (() => {})) : setInternalQuery;
  const [isFocused, setIsFocused] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const debouncedQuery = useDebounce(query, 150);

  // Build commit lookup map
  const commitMap = useMemo(() => buildCommitMap(gitHistory), [gitHistory]);

  // Search function
  const searchResults = useMemo((): SearchResult[] => {
    if (!debouncedQuery || debouncedQuery.length < 2) return [];

    const lowerQuery = debouncedQuery.toLowerCase();
    const results: SearchResult[] = [];
    const seenNodeIds = new Set<number>();

    for (const node of nodes) {
      const metadata = parseMetadata(node.metadata_json);

      // Search title
      if (node.title.toLowerCase().includes(lowerQuery)) {
        if (!seenNodeIds.has(node.id)) {
          results.push({ node, matchType: 'title', matchText: node.title });
          seenNodeIds.add(node.id);
        }
      }

      // Search description
      if (node.description?.toLowerCase().includes(lowerQuery)) {
        if (!seenNodeIds.has(node.id)) {
          results.push({ node, matchType: 'description', matchText: node.description });
          seenNodeIds.add(node.id);
        }
      }

      // Search commit hash and message
      if (metadata?.commit) {
        const commitInfo = commitMap.get(metadata.commit) || commitMap.get(metadata.commit.slice(0, 7));
        const commitMessage = commitInfo?.message?.toLowerCase() || '';
        const commitHash = metadata.commit.toLowerCase();

        if (commitHash.includes(lowerQuery) || commitMessage.includes(lowerQuery)) {
          if (!seenNodeIds.has(node.id)) {
            results.push({
              node,
              matchType: 'commit',
              matchText: commitInfo?.message || metadata.commit,
              commitInfo,
            });
            seenNodeIds.add(node.id);
          }
        }
      }

      // Search prompt
      if (metadata?.prompt?.toLowerCase().includes(lowerQuery)) {
        if (!seenNodeIds.has(node.id)) {
          results.push({ node, matchType: 'prompt', matchText: metadata.prompt });
          seenNodeIds.add(node.id);
        }
      }

      // Search files
      if (metadata?.files?.some(f => f.toLowerCase().includes(lowerQuery))) {
        if (!seenNodeIds.has(node.id)) {
          const matchedFile = metadata.files.find(f => f.toLowerCase().includes(lowerQuery)) || '';
          results.push({ node, matchType: 'files', matchText: matchedFile });
          seenNodeIds.add(node.id);
        }
      }
    }

    // Sort by relevance: title matches first, then by recency
    return results.sort((a, b) => {
      // Title matches are most relevant
      if (a.matchType === 'title' && b.matchType !== 'title') return -1;
      if (b.matchType === 'title' && a.matchType !== 'title') return 1;
      // Then by recency
      return new Date(b.node.updated_at).getTime() - new Date(a.node.updated_at).getTime();
    }).slice(0, 15); // Limit to 15 results
  }, [debouncedQuery, nodes, commitMap]);

  // Update highlighted nodes when search changes
  useEffect(() => {
    const highlightedIds = new Set(searchResults.map(r => r.node.id));
    onHighlightNodes(highlightedIds);
  }, [searchResults, onHighlightNodes]);

  // Reset selected index when results change
  useEffect(() => {
    setSelectedIndex(0);
  }, [searchResults]);

  // Keyboard navigation
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (!searchResults.length) return;

    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        setSelectedIndex(prev => Math.min(prev + 1, searchResults.length - 1));
        break;
      case 'ArrowUp':
        e.preventDefault();
        setSelectedIndex(prev => Math.max(prev - 1, 0));
        break;
      case 'Enter':
        e.preventDefault();
        if (searchResults[selectedIndex]) {
          onSelectNode(searchResults[selectedIndex].node);
          setQuery('');
          inputRef.current?.blur();
        }
        break;
      case 'Escape':
        e.preventDefault();
        setQuery('');
        inputRef.current?.blur();
        break;
    }
  }, [searchResults, selectedIndex, onSelectNode]);

  // Scroll selected item into view
  useEffect(() => {
    if (dropdownRef.current && searchResults.length > 0) {
      const selectedElement = dropdownRef.current.children[selectedIndex] as HTMLElement;
      selectedElement?.scrollIntoView({ block: 'nearest' });
    }
  }, [selectedIndex, searchResults.length]);

  const handleResultClick = useCallback((result: SearchResult) => {
    onSelectNode(result.node);
    setQuery('');
    inputRef.current?.blur();
  }, [onSelectNode]);

  const showDropdown = isFocused && searchResults.length > 0;

  // Get match type label
  const getMatchLabel = (matchType: SearchResult['matchType']) => {
    switch (matchType) {
      case 'title': return 'Title';
      case 'description': return 'Desc';
      case 'commit': return 'Commit';
      case 'prompt': return 'Prompt';
      case 'files': return 'File';
    }
  };

  return (
    <div style={styles.container}>
      <div style={styles.inputWrapper}>
        <svg style={styles.searchIcon} viewBox="0 0 16 16" fill="currentColor">
          <path d="M10.68 11.74a6 6 0 0 1-7.922-8.982 6 6 0 0 1 8.982 7.922l3.04 3.04a.749.749 0 0 1-.326 1.275.749.749 0 0 1-.734-.215ZM11.5 7a4.499 4.499 0 1 0-8.997 0A4.499 4.499 0 0 0 11.5 7Z" />
        </svg>
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={e => setQuery(e.target.value)}
          onFocus={() => setIsFocused(true)}
          onBlur={() => setTimeout(() => setIsFocused(false), 200)}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          style={styles.input}
          aria-label="Search nodes"
          aria-expanded={showDropdown}
          aria-controls="search-results"
          role="combobox"
        />
        {query && (
          <button
            onClick={() => setQuery('')}
            style={styles.clearBtn}
            aria-label="Clear search"
          >
            ×
          </button>
        )}
      </div>

      {showDropdown && (
        <div
          ref={dropdownRef}
          id="search-results"
          style={styles.dropdown}
          role="listbox"
        >
          {searchResults.map((result, index) => (
            <div
              key={`${result.node.id}-${result.matchType}`}
              onClick={() => handleResultClick(result)}
              style={{
                ...styles.resultItem,
                ...(index === selectedIndex ? styles.resultItemSelected : {}),
              }}
              role="option"
              aria-selected={index === selectedIndex}
            >
              <div style={styles.resultLeft}>
                <div
                  style={{
                    ...styles.nodeTypeDot,
                    backgroundColor: getNodeColor(result.node.node_type),
                  }}
                />
                <span style={styles.nodeId}>#{result.node.id}</span>
                <span style={styles.resultTitle}>{truncate(result.node.title, 35)}</span>
              </div>
              <div style={styles.resultRight}>
                <span style={styles.matchLabel}>{getMatchLabel(result.matchType)}</span>
                {result.matchType === 'commit' && result.commitInfo && (
                  <span style={styles.commitHash}>{shortCommit(result.commitInfo.hash)}</span>
                )}
              </div>
            </div>
          ))}
          <div style={styles.dropdownFooter}>
            <span style={styles.footerHint}>↑↓ navigate · Enter select · Esc close</span>
          </div>
        </div>
      )}

      {isFocused && query.length >= 2 && searchResults.length === 0 && (
        <div style={styles.dropdown}>
          <div style={styles.noResults}>No matching nodes found</div>
        </div>
      )}
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    position: 'relative',
    flex: 1,
    maxWidth: '400px',
    minWidth: '200px',
  },
  inputWrapper: {
    position: 'relative',
    display: 'flex',
    alignItems: 'center',
  },
  searchIcon: {
    position: 'absolute',
    left: '10px',
    width: '16px',
    height: '16px',
    color: '#57606a',
    pointerEvents: 'none',
  },
  input: {
    width: '100%',
    padding: '8px 32px 8px 34px',
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    fontSize: '13px',
    color: '#24292f',
    outline: 'none',
    transition: 'border-color 0.15s, background-color 0.15s',
  },
  clearBtn: {
    position: 'absolute',
    right: '8px',
    width: '20px',
    height: '20px',
    padding: 0,
    backgroundColor: 'transparent',
    border: 'none',
    borderRadius: '4px',
    color: '#57606a',
    fontSize: '16px',
    cursor: 'pointer',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
  },
  dropdown: {
    position: 'absolute',
    top: 'calc(100% + 4px)',
    left: 0,
    right: 0,
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderRadius: '8px',
    boxShadow: '0 8px 24px rgba(0, 0, 0, 0.12)',
    maxHeight: '400px',
    overflowY: 'auto',
    zIndex: 1000,
  },
  resultItem: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: '10px 12px',
    cursor: 'pointer',
    borderBottom: '1px solid #f0f0f0',
    transition: 'background-color 0.1s',
  },
  resultItemSelected: {
    backgroundColor: '#f6f8fa',
  },
  resultLeft: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    flex: 1,
    minWidth: 0,
  },
  nodeTypeDot: {
    width: '8px',
    height: '8px',
    borderRadius: '50%',
    flexShrink: 0,
  },
  nodeId: {
    fontSize: '11px',
    color: '#6e7781',
    fontFamily: 'monospace',
    flexShrink: 0,
  },
  resultTitle: {
    fontSize: '13px',
    color: '#24292f',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  },
  resultRight: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    flexShrink: 0,
    marginLeft: '8px',
  },
  matchLabel: {
    fontSize: '10px',
    color: '#57606a',
    backgroundColor: '#f6f8fa',
    padding: '2px 6px',
    borderRadius: '4px',
    textTransform: 'uppercase',
  },
  commitHash: {
    fontSize: '11px',
    color: '#0969da',
    fontFamily: 'monospace',
    backgroundColor: '#ddf4ff',
    padding: '2px 6px',
    borderRadius: '4px',
  },
  dropdownFooter: {
    padding: '8px 12px',
    backgroundColor: '#f6f8fa',
    borderTop: '1px solid #d0d7de',
  },
  footerHint: {
    fontSize: '11px',
    color: '#6e7781',
  },
  noResults: {
    padding: '16px',
    textAlign: 'center',
    color: '#57606a',
    fontSize: '13px',
  },
};

export default SearchBar;
