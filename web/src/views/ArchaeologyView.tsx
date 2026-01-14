/**
 * ArchaeologyView - Redesigned
 *
 * Narrative-focused exploration with:
 * - Left sidebar: Compact narrative list with keyboard nav (j/k)
 * - Main area: DAG visualization of selected narrative
 * - Right overlay: Stacked cards for viewing nodes
 * - Modal: Single comprehensive AI explanation (not chat)
 */

import React, { useState, useMemo, useCallback, useEffect, useRef } from 'react';
import type { GraphData } from '../types/graph';
import type { ArchaeologyFilters } from '../types/archaeology';
import { DEFAULT_ARCHAEOLOGY_FILTERS } from '../types/archaeology';
import {
  buildNarratives,
  filterNarratives,
  calculateArchaeologyStats,
  formatNarrativeContext,
} from '../utils/archaeologyProcessing';
import { NarrativeGraph } from '../components/NarrativeGraph';
import { CardStack } from '../components/CardStack';
import { ExplanationModal } from '../components/ExplanationModal';
import { getNodeColor } from '../utils/colors';
import { useLocalStorage } from '../hooks/useLocalStorage';
import { useIsMobile } from '../hooks/useMediaQuery';

// Request timeout in milliseconds
const REQUEST_TIMEOUT_MS = 60000;

interface ArchaeologyViewProps {
  graphData: GraphData;
}

export const ArchaeologyView: React.FC<ArchaeologyViewProps> = ({ graphData }) => {
  // Persisted state
  const [selectedNarrativeId, setSelectedNarrativeId] = useLocalStorage<string | null>(
    'selected_narrative_id',
    null
  );
  const [filters, setFilters] = useLocalStorage<ArchaeologyFilters>(
    'filters',
    DEFAULT_ARCHAEOLOGY_FILTERS
  );

  // Responsive state
  const isMobile = useIsMobile();
  const [sidebarOpen, setSidebarOpen] = useState(!isMobile);

  // Transient state
  const [showCardStack, setShowCardStack] = useState(false);
  const [cardStackSelectedIndex, setCardStackSelectedIndex] = useState(0);
  const [cardStackExpandedIndex, setCardStackExpandedIndex] = useState<number | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<number | null>(null);

  // Modal state
  const [modalOpen, setModalOpen] = useState(false);
  const [modalTitle, setModalTitle] = useState('');
  const [modalContent, setModalContent] = useState<string | null>(null);
  const [modalLoading, setModalLoading] = useState(false);
  const [modalError, setModalError] = useState<string | null>(null);

  // Question input state
  const [question, setQuestion] = useState('');
  const abortControllerRef = useRef<AbortController | null>(null);

  // Build narratives from graph data
  const narratives = useMemo(() => buildNarratives(graphData), [graphData]);

  // Apply filters
  const filteredNarratives = useMemo(
    () => filterNarratives(narratives, filters),
    [narratives, filters]
  );

  // Compute selected index from persisted ID
  const selectedNarrativeIndex = useMemo(() => {
    if (!selectedNarrativeId) return 0;
    const idx = filteredNarratives.findIndex(n => n.id === selectedNarrativeId);
    return idx >= 0 ? idx : 0;
  }, [filteredNarratives, selectedNarrativeId]);

  // Helper to update selection
  const setSelectedNarrativeIndex = useCallback((indexOrFn: number | ((prev: number) => number)) => {
    const newIndex = typeof indexOrFn === 'function' ? indexOrFn(selectedNarrativeIndex) : indexOrFn;
    const narrative = filteredNarratives[newIndex];
    if (narrative) {
      setSelectedNarrativeId(narrative.id);
    }
  }, [filteredNarratives, selectedNarrativeIndex, setSelectedNarrativeId]);

  // Currently selected narrative
  const selectedNarrative = useMemo(
    () => filteredNarratives[selectedNarrativeIndex] ?? null,
    [filteredNarratives, selectedNarrativeIndex]
  );

  // Statistics
  const stats = useMemo(
    () => calculateArchaeologyStats(filteredNarratives),
    [filteredNarratives]
  );

  // Keyboard navigation for narrative list
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't handle if card stack is showing (it has its own handlers)
      if (showCardStack) return;
      // Don't handle if modal is open
      if (modalOpen) return;
      // Don't handle if typing in input or textarea
      if (document.activeElement?.tagName === 'INPUT' || document.activeElement?.tagName === 'TEXTAREA') return;

      switch (e.key) {
        case 'j':
        case 'ArrowDown':
          e.preventDefault();
          setSelectedNarrativeIndex(prev =>
            Math.min(prev + 1, filteredNarratives.length - 1)
          );
          break;
        case 'k':
        case 'ArrowUp':
          e.preventDefault();
          setSelectedNarrativeIndex(prev => Math.max(prev - 1, 0));
          break;
        case 'Enter':
          e.preventDefault();
          if (selectedNarrative) {
            setShowCardStack(true);
            setCardStackSelectedIndex(0);
            setCardStackExpandedIndex(null);
          }
          break;
        // Jump to first/last narrative
        case 'Home':
        case 'g':
          e.preventDefault();
          setSelectedNarrativeIndex(0);
          break;
        case 'End':
        case 'G':
          e.preventDefault();
          setSelectedNarrativeIndex(filteredNarratives.length - 1);
          break;
        // Page through narratives
        case 'PageDown':
          e.preventDefault();
          setSelectedNarrativeIndex(prev =>
            Math.min(prev + 10, filteredNarratives.length - 1)
          );
          break;
        case 'PageUp':
          e.preventDefault();
          setSelectedNarrativeIndex(prev => Math.max(prev - 10, 0));
          break;
        // Focus search with /
        case '/':
          e.preventDefault();
          const searchInput = document.querySelector('input[placeholder*="Search"]') as HTMLInputElement;
          if (searchInput) {
            searchInput.focus();
          }
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [showCardStack, modalOpen, filteredNarratives.length, selectedNarrative]);

  // Handle node selection in graph
  const handleNodeSelect = useCallback((nodeId: number) => {
    setSelectedNodeId(nodeId);
    // Always show card stack when a node is clicked
    setShowCardStack(true);
    if (selectedNarrative) {
      const nodeIndex = selectedNarrative.nodes.findIndex(n => n.id === nodeId);
      if (nodeIndex >= 0) {
        setCardStackSelectedIndex(nodeIndex);
        setCardStackExpandedIndex(nodeIndex);
      }
    }
  }, [selectedNarrative]);

  // Cleanup abort controller on unmount
  useEffect(() => {
    return () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, []);

  // Handle asking a question
  const handleAskQuestion = useCallback(async () => {
    if (!question.trim() || !selectedNarrative) return;

    // Cancel any existing request
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    abortControllerRef.current = new AbortController();
    const { signal } = abortControllerRef.current;

    setModalTitle(question);
    setModalOpen(true);
    setModalLoading(true);
    setModalError(null);
    setModalContent(null);

    try {
      // Build context from narrative (matching backend AskContext format)
      const narrativeContext = formatNarrativeContext(selectedNarrative);
      const context = {
        narrative: narrativeContext,
        visible_node_ids: selectedNarrative.nodes.map(n => n.id),
      };

      // Create timeout promise
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error('Request timed out')), REQUEST_TIMEOUT_MS);
      });

      // Race fetch against timeout
      const response = await Promise.race([
        fetch('/api/ask', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            question: question,
            context,
          }),
          signal,
        }),
        timeoutPromise,
      ]);

      if (!response.ok) {
        throw new Error(`API error: ${response.status}`);
      }

      const data = await response.json();
      // Response format: {"ok":true,"data":{"answer":"..."}}
      const answer = data.data?.answer || data.response || data.answer || 'No response received.';
      setModalContent(answer);
    } catch (err) {
      if (err instanceof Error && err.name === 'AbortError') {
        // Request was cancelled - close modal
        setModalOpen(false);
        return;
      }
      setModalError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setModalLoading(false);
      abortControllerRef.current = null;
    }
  }, [question, selectedNarrative]);

  // Cancel the current request
  const handleCancelRequest = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
    setModalOpen(false);
    setModalLoading(false);
  }, []);

  // Handle search
  const handleFilterChange = (key: keyof ArchaeologyFilters, value: unknown) => {
    setFilters(prev => ({ ...prev, [key]: value }));
    setSelectedNarrativeIndex(0);
  };

  return (
    <div style={styles.container}>
      {/* Mobile sidebar toggle */}
      {isMobile && (
        <button
          style={{
            ...styles.sidebarToggle,
            ...(sidebarOpen ? styles.sidebarToggleOpen : {}),
          }}
          onClick={() => setSidebarOpen(!sidebarOpen)}
          aria-label={sidebarOpen ? 'Close sidebar' : 'Open sidebar'}
        >
          {sidebarOpen ? '\u2715' : '\u2630'}
        </button>
      )}

      {/* Left Sidebar - Narrative List */}
      <div
        style={{
          ...styles.sidebar,
          ...(isMobile ? styles.sidebarMobile : {}),
          ...(isMobile && !sidebarOpen ? styles.sidebarHidden : {}),
        }}
      >
        {/* Search */}
        <div style={styles.searchBox}>
          <input
            type="text"
            placeholder="Search narratives..."
            value={filters.searchQuery}
            onChange={e => handleFilterChange('searchQuery', e.target.value)}
            style={styles.searchInput}
          />
        </div>

        {/* Stats */}
        <div style={styles.statsBar}>
          <span>{stats.narrativeCount} narratives</span>
          <span>{stats.totalPivots} pivots</span>
        </div>

        {/* Narrative List */}
        <div style={styles.narrativeList}>
          {filteredNarratives.length === 0 ? (
            <div style={styles.emptyState}>
              <p>No narratives found.</p>
            </div>
          ) : (
            filteredNarratives.map((narrative, index) => (
              <div
                key={narrative.id}
                style={{
                  ...styles.narrativeItem,
                  ...(index === selectedNarrativeIndex ? styles.narrativeItemSelected : {}),
                  borderLeftColor: getNodeColor(narrative.root.node_type),
                }}
                onClick={() => {
                  setSelectedNarrativeIndex(index);
                  if (isMobile) setSidebarOpen(false);
                }}
              >
                <div style={styles.narrativeHeader}>
                  <span
                    style={{
                      ...styles.typeBadge,
                      backgroundColor: getNodeColor(narrative.root.node_type) + '22',
                      color: getNodeColor(narrative.root.node_type),
                    }}
                  >
                    {narrative.root.node_type}
                  </span>
                  <span style={styles.nodeCount}>{narrative.nodes.length}</span>
                </div>
                <div style={styles.narrativeTitle}>{narrative.name}</div>
                {narrative.pivots.length > 0 && (
                  <span style={styles.pivotBadge}>
                    {narrative.pivots.length} pivot{narrative.pivots.length > 1 ? 's' : ''}
                  </span>
                )}
              </div>
            ))
          )}
        </div>

        {/* Keyboard hints */}
        <div style={styles.keyboardHints}>
          <span>j/k nav</span>
          <span>/ search</span>
          <span>Enter cards</span>
        </div>
      </div>

      {/* Main Area - Graph + Question Input */}
      <div style={styles.main}>
        {/* Graph visualization */}
        <NarrativeGraph
          nodes={selectedNarrative?.nodes ?? []}
          edges={selectedNarrative?.edges ?? []}
          selectedNodeId={selectedNodeId}
          onNodeSelect={handleNodeSelect}
        />

        {/* Card Stack Overlay */}
        {showCardStack && selectedNarrative && (
          <CardStack
            nodes={selectedNarrative.nodes}
            selectedIndex={cardStackSelectedIndex}
            expandedIndex={cardStackExpandedIndex}
            onSelectIndex={setCardStackSelectedIndex}
            onExpandIndex={setCardStackExpandedIndex}
            onClose={() => {
              setShowCardStack(false);
              setSelectedNodeId(null);
            }}
          />
        )}

        {/* Question Input Bar */}
        <div style={styles.questionBar}>
          <textarea
            placeholder={selectedNarrative
              ? `Ask about "${selectedNarrative.name}"...\n(Shift+Enter for new line, Enter to send)`
              : 'Select a narrative first...'
            }
            value={question}
            onChange={e => setQuestion(e.target.value)}
            onKeyDown={e => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                handleAskQuestion();
              }
            }}
            style={styles.questionInput}
            disabled={!selectedNarrative}
            rows={3}
          />
          <button
            style={{
              ...styles.askButton,
              ...((!selectedNarrative || !question.trim()) ? styles.askButtonDisabled : {}),
            }}
            onClick={handleAskQuestion}
            disabled={!selectedNarrative || !question.trim()}
          >
            Ask Claude
          </button>
        </div>
      </div>

      {/* Explanation Modal */}
      <ExplanationModal
        isOpen={modalOpen}
        title={modalTitle}
        content={modalContent}
        isLoading={modalLoading}
        error={modalError}
        onClose={() => setModalOpen(false)}
        onCancel={modalLoading ? handleCancelRequest : undefined}
        onRetry={modalError ? handleAskQuestion : undefined}
        timestamp={new Date()}
      />
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: 'flex',
    height: '100%',
    maxHeight: '100%',
    backgroundColor: '#ffffff',
    overflow: 'hidden',
  },

  // Sidebar
  sidebar: {
    width: '320px',
    minWidth: '280px',
    display: 'flex',
    flexDirection: 'column',
    borderRight: '1px solid #d0d7de',
    backgroundColor: '#fafbfc',
    minHeight: 0, // Critical for flex child to shrink properly
    overflow: 'hidden',
  },
  searchBox: {
    padding: '12px',
    borderBottom: '1px solid #e1e4e8',
  },
  searchInput: {
    width: '100%',
    padding: '8px 12px',
    fontSize: '13px',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    outline: 'none',
    backgroundColor: '#ffffff',
  },
  statsBar: {
    display: 'flex',
    gap: '12px',
    padding: '8px 12px',
    fontSize: '11px',
    color: '#57606a',
    borderBottom: '1px solid #e1e4e8',
    backgroundColor: '#f6f8fa',
  },
  narrativeList: {
    flex: 1,
    overflowY: 'auto',
    padding: '8px',
  },
  emptyState: {
    padding: '24px',
    textAlign: 'center',
    color: '#57606a',
    fontSize: '13px',
  },
  narrativeItem: {
    padding: '10px 12px',
    marginBottom: '4px',
    backgroundColor: '#ffffff',
    border: '1px solid #e1e4e8',
    borderLeft: '3px solid',
    borderRadius: '6px',
    cursor: 'pointer',
    transition: 'all 0.15s ease',
  },
  narrativeItemSelected: {
    backgroundColor: '#ddf4ff',
    borderColor: '#0969da',
    boxShadow: '0 2px 8px rgba(0,0,0,0.1)',
  },
  narrativeHeader: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    marginBottom: '4px',
  },
  typeBadge: {
    fontSize: '9px',
    fontWeight: 600,
    textTransform: 'uppercase',
    padding: '2px 6px',
    borderRadius: '3px',
  },
  nodeCount: {
    fontSize: '11px',
    color: '#8c959f',
  },
  narrativeTitle: {
    fontSize: '13px',
    fontWeight: 500,
    color: '#24292f',
    lineHeight: 1.3,
    marginBottom: '4px',
  },
  pivotBadge: {
    fontSize: '10px',
    color: '#fb8500',
    fontWeight: 500,
  },
  keyboardHints: {
    display: 'flex',
    gap: '16px',
    padding: '10px 12px',
    fontSize: '11px',
    color: '#8c959f',
    borderTop: '1px solid #e1e4e8',
    backgroundColor: '#f6f8fa',
  },

  // Main area
  main: {
    flex: 1,
    display: 'flex',
    flexDirection: 'column',
    position: 'relative',
    overflow: 'hidden',
    minHeight: 0, // Critical for flex child to shrink properly
  },
  questionBar: {
    display: 'flex',
    gap: '12px',
    padding: '16px',
    borderTop: '1px solid #d0d7de',
    backgroundColor: '#ffffff',
    flexShrink: 0,
    alignItems: 'flex-end',
  },
  questionInput: {
    flex: 1,
    padding: '12px 14px',
    fontSize: '14px',
    border: '1px solid #d0d7de',
    borderRadius: '8px',
    outline: 'none',
    backgroundColor: '#ffffff',
    resize: 'none',
    fontFamily: 'inherit',
    lineHeight: 1.5,
    minHeight: '80px',
  },
  askButton: {
    padding: '14px 24px',
    fontSize: '14px',
    fontWeight: 500,
    backgroundColor: '#6741d9',
    color: '#ffffff',
    border: 'none',
    borderRadius: '8px',
    cursor: 'pointer',
    transition: 'background-color 0.15s ease',
    alignSelf: 'flex-end',
    height: 'fit-content',
  },
  askButtonDisabled: {
    backgroundColor: '#d0d7de',
    cursor: 'not-allowed',
  },

  // Mobile responsive styles
  sidebarToggle: {
    position: 'fixed',
    top: '70px',
    left: '12px',
    zIndex: 1100,
    width: '44px',
    height: '44px',
    borderRadius: '8px',
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    boxShadow: '0 2px 8px rgba(0,0,0,0.15)',
    cursor: 'pointer',
    fontSize: '20px',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: '#57606a',
    transition: 'all 0.2s ease',
  },
  sidebarToggleOpen: {
    left: '280px',
    backgroundColor: '#f6f8fa',
  },
  sidebarMobile: {
    position: 'fixed',
    top: '60px',
    left: 0,
    bottom: 0,
    width: '280px',
    zIndex: 1000,
    transition: 'transform 0.3s ease',
    boxShadow: '4px 0 16px rgba(0,0,0,0.15)',
  },
  sidebarHidden: {
    transform: 'translateX(-100%)',
  },
};
