/**
 * Trace View
 *
 * Displays API trace sessions and spans captured by `deciduous proxy`.
 * Shows token usage, model info, and allows linking to decision nodes.
 *
 * Keyboard shortcuts:
 * - j/k or Arrow keys: Navigate sessions/spans
 * - Enter: Expand session to see spans / show span detail
 * - Escape: Go back to previous view
 * - l: Link session to most recent goal
 * - u: Unlink session
 * - r: Refresh traces
 */

import React, { useState, useEffect, useCallback } from 'react';
import {
  TraceSession,
  TraceSpan,
  TraceContent,
  formatTokens,
  formatDuration,
  formatRelativeTime,
  getModelShortName,
  getSessionDuration,
} from '../types/trace';

// =============================================================================
// Types
// =============================================================================

type ViewMode = 'sessions' | 'spans' | 'detail';
type DetailTab = 'thinking' | 'response' | 'tools';

// =============================================================================
// Styles
// =============================================================================

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: 'flex',
    height: '100%',
    overflow: 'hidden',
    backgroundColor: '#fafafa',
    color: '#111827',
  },
  sessionList: {
    flex: 1,
    overflow: 'auto',
    padding: '16px',
    borderRight: '1px solid #e5e7eb',
  },
  spanList: {
    flex: 1,
    overflow: 'auto',
    padding: '16px',
    borderRight: '1px solid #e5e7eb',
  },
  detailPanel: {
    width: '400px',
    overflow: 'auto',
    padding: '16px',
    backgroundColor: '#ffffff',
  },
  header: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: '16px',
  },
  title: {
    fontSize: '18px',
    fontWeight: 600,
    color: '#0969da',
    margin: 0,
  },
  backButton: {
    background: 'none',
    border: '1px solid #e5e7eb',
    color: '#6b7280',
    padding: '4px 12px',
    borderRadius: '6px',
    cursor: 'pointer',
    fontSize: '12px',
  },
  sessionCard: {
    padding: '12px',
    marginBottom: '8px',
    borderRadius: '8px',
    border: '1px solid #e5e7eb',
    backgroundColor: '#ffffff',
    cursor: 'pointer',
    transition: 'all 0.15s ease',
  },
  sessionCardSelected: {
    backgroundColor: '#eff6ff',
    borderColor: '#3b82f6',
  },
  sessionCardLinked: {
    borderColor: '#22c55e',
  },
  sessionId: {
    fontFamily: 'monospace',
    fontSize: '14px',
    color: '#0969da',
    marginBottom: '8px',
  },
  sessionMeta: {
    display: 'flex',
    gap: '16px',
    fontSize: '12px',
    color: '#6b7280',
    marginBottom: '8px',
  },
  tokenBar: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    fontSize: '12px',
  },
  tokenIn: {
    color: '#16a34a',
  },
  tokenOut: {
    color: '#ea580c',
  },
  linkedBadge: {
    backgroundColor: '#22c55e',
    color: '#fff',
    padding: '2px 8px',
    borderRadius: '4px',
    fontSize: '11px',
    marginLeft: '8px',
  },
  spanRow: {
    padding: '8px 12px',
    marginBottom: '4px',
    borderRadius: '6px',
    border: '1px solid #e5e7eb',
    backgroundColor: '#ffffff',
    cursor: 'pointer',
    display: 'flex',
    gap: '12px',
    alignItems: 'center',
    fontSize: '13px',
  },
  spanRowSelected: {
    backgroundColor: '#eff6ff',
    borderColor: '#3b82f6',
  },
  spanSeq: {
    color: '#6b7280',
    fontFamily: 'monospace',
    minWidth: '30px',
  },
  modelBadge: {
    padding: '2px 6px',
    borderRadius: '4px',
    fontSize: '11px',
    backgroundColor: '#f3f4f6',
    color: '#374151',
  },
  modelOpus: { backgroundColor: '#8b5cf6', color: '#fff' },
  modelSonnet: { backgroundColor: '#3b82f6', color: '#fff' },
  modelHaiku: { backgroundColor: '#22c55e', color: '#fff' },
  spanDuration: {
    color: '#6b7280',
    minWidth: '50px',
  },
  spanTokens: {
    display: 'flex',
    gap: '4px',
  },
  toolList: {
    color: '#ea580c',
    flex: 1,
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  },
  detailTabs: {
    display: 'flex',
    gap: '8px',
    marginBottom: '16px',
  },
  tab: {
    padding: '6px 12px',
    borderRadius: '6px',
    border: '1px solid #e5e7eb',
    background: '#ffffff',
    color: '#6b7280',
    cursor: 'pointer',
    fontSize: '13px',
  },
  tabActive: {
    backgroundColor: '#3b82f6',
    borderColor: '#3b82f6',
    color: '#fff',
  },
  contentArea: {
    backgroundColor: '#f9fafb',
    border: '1px solid #e5e7eb',
    padding: '12px',
    borderRadius: '6px',
    fontFamily: 'monospace',
    fontSize: '12px',
    lineHeight: '1.5',
    whiteSpace: 'pre-wrap',
    wordBreak: 'break-word',
    maxHeight: 'calc(100vh - 200px)',
    overflow: 'auto',
    color: '#374151',
  },
  emptyState: {
    textAlign: 'center' as const,
    padding: '48px',
    color: '#9ca3af',
  },
  previewSection: {
    marginBottom: '12px',
  },
  previewLabel: {
    color: '#0969da',
    fontSize: '12px',
    marginBottom: '4px',
  },
  previewText: {
    fontSize: '12px',
    color: '#6b7280',
    lineHeight: '1.4',
  },
};

// =============================================================================
// Components
// =============================================================================

const TraceView: React.FC = () => {
  const [sessions, setSessions] = useState<TraceSession[]>([]);
  const [spans, setSpans] = useState<TraceSpan[]>([]);
  const [content, setContent] = useState<TraceContent[]>([]);
  const [selectedSessionIdx, setSelectedSessionIdx] = useState(0);
  const [selectedSpanIdx, setSelectedSpanIdx] = useState(0);
  const [viewMode, setViewMode] = useState<ViewMode>('sessions');
  const [detailTab, setDetailTab] = useState<DetailTab>('thinking');
  const [_loading, setLoading] = useState(false);

  // Fetch sessions on mount
  useEffect(() => {
    fetchSessions();
  }, []);

  const fetchSessions = async () => {
    setLoading(true);
    try {
      const res = await fetch('/api/traces');
      const data = await res.json();
      if (data.ok && data.data) {
        setSessions(data.data);
      }
    } catch (e) {
      console.error('Failed to fetch traces:', e);
    } finally {
      setLoading(false);
    }
  };

  const fetchSpans = async (sessionId: string) => {
    setLoading(true);
    try {
      const res = await fetch(`/api/traces/${sessionId}`);
      const data = await res.json();
      if (data.ok && data.data) {
        setSpans(data.data);
        setSelectedSpanIdx(0);
      }
    } catch (e) {
      console.error('Failed to fetch spans:', e);
    } finally {
      setLoading(false);
    }
  };

  const fetchContent = async (sessionId: string, spanId: number) => {
    setLoading(true);
    try {
      const res = await fetch(`/api/traces/${sessionId}/spans/${spanId}`);
      const data = await res.json();
      if (data.ok && data.data) {
        setContent(data.data);
      }
    } catch (e) {
      console.error('Failed to fetch content:', e);
    } finally {
      setLoading(false);
    }
  };

  // Keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't handle if in input
      if (e.target instanceof HTMLInputElement) return;

      switch (e.key) {
        case 'j':
        case 'ArrowDown':
          e.preventDefault();
          if (viewMode === 'sessions') {
            setSelectedSessionIdx(i => Math.min(i + 1, sessions.length - 1));
          } else if (viewMode === 'spans') {
            setSelectedSpanIdx(i => Math.min(i + 1, spans.length - 1));
          }
          break;
        case 'k':
        case 'ArrowUp':
          e.preventDefault();
          if (viewMode === 'sessions') {
            setSelectedSessionIdx(i => Math.max(i - 1, 0));
          } else if (viewMode === 'spans') {
            setSelectedSpanIdx(i => Math.max(i - 1, 0));
          }
          break;
        case 'Enter':
          e.preventDefault();
          if (viewMode === 'sessions' && sessions[selectedSessionIdx]) {
            const session = sessions[selectedSessionIdx];
            fetchSpans(session.session_id);
            setViewMode('spans');
          } else if (viewMode === 'spans' && spans[selectedSpanIdx]) {
            const session = sessions[selectedSessionIdx];
            const span = spans[selectedSpanIdx];
            fetchContent(session.session_id, span.id);
            setViewMode('detail');
            setDetailTab('thinking');
          }
          break;
        case 'Escape':
          e.preventDefault();
          if (viewMode === 'detail') {
            setViewMode('spans');
            setContent([]);
          } else if (viewMode === 'spans') {
            setViewMode('sessions');
            setSpans([]);
          }
          break;
        case 'r':
          e.preventDefault();
          fetchSessions();
          break;
        case 'Tab':
          if (viewMode === 'detail') {
            e.preventDefault();
            setDetailTab(t => {
              if (t === 'thinking') return 'response';
              if (t === 'response') return 'tools';
              return 'thinking';
            });
          }
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [viewMode, sessions, spans, selectedSessionIdx, selectedSpanIdx]);

  // Get content for current tab
  const getTabContent = useCallback(() => {
    if (detailTab === 'thinking') {
      return content.filter(c => c.content_type === 'thinking').map(c => c.content).join('\n');
    } else if (detailTab === 'response') {
      return content.filter(c => c.content_type === 'response').map(c => c.content).join('\n');
    } else {
      return content
        .filter(c => c.content_type === 'tool_input' || c.content_type === 'tool_output')
        .map(c => `=== ${c.tool_name || 'Tool'} (${c.content_type}) ===\n${c.content}`)
        .join('\n\n');
    }
  }, [content, detailTab]);

  const getModelStyle = (model: string | null): React.CSSProperties => {
    const name = getModelShortName(model);
    if (name === 'opus') return { ...styles.modelBadge, ...styles.modelOpus };
    if (name === 'sonnet') return { ...styles.modelBadge, ...styles.modelSonnet };
    if (name === 'haiku') return { ...styles.modelBadge, ...styles.modelHaiku };
    return styles.modelBadge;
  };

  // Session list view
  const renderSessions = () => (
    <div style={styles.sessionList}>
      <div style={styles.header}>
        <h2 style={styles.title}>Trace Sessions</h2>
        <button style={styles.backButton} onClick={fetchSessions}>
          Refresh
        </button>
      </div>
      {sessions.length === 0 ? (
        <div style={styles.emptyState}>
          <p>No trace sessions found.</p>
          <p style={{ fontSize: '12px', marginTop: '8px' }}>
            Run <code>deciduous proxy -- claude</code> to capture API traffic.
          </p>
        </div>
      ) : (
        sessions.map((session, idx) => (
          <div
            key={session.session_id}
            style={{
              ...styles.sessionCard,
              ...(idx === selectedSessionIdx ? styles.sessionCardSelected : {}),
              ...(session.linked_node_id ? styles.sessionCardLinked : {}),
            }}
            onClick={() => {
              setSelectedSessionIdx(idx);
              fetchSpans(session.session_id);
              setViewMode('spans');
            }}
          >
            <div style={styles.sessionId}>
              {session.session_id.slice(0, 8)}
              {session.linked_node_id && (
                <span style={styles.linkedBadge}>→ #{session.linked_node_id}</span>
              )}
            </div>
            <div style={styles.sessionMeta}>
              <span>{formatRelativeTime(session.started_at)}</span>
              <span>{getSessionDuration(session)}</span>
              {session.git_branch && <span>🌿 {session.git_branch}</span>}
            </div>
            <div style={styles.tokenBar}>
              <span style={styles.tokenIn}>↓ {formatTokens(session.total_input_tokens)}</span>
              <span style={styles.tokenOut}>↑ {formatTokens(session.total_output_tokens)}</span>
              {session.total_cache_read > 0 && (
                <span style={{ color: '#8b949e' }}>📖 {formatTokens(session.total_cache_read)}</span>
              )}
            </div>
          </div>
        ))
      )}
    </div>
  );

  // Spans list view
  const renderSpans = () => {
    const session = sessions[selectedSessionIdx];
    return (
      <div style={styles.spanList}>
        <div style={styles.header}>
          <h2 style={styles.title}>
            Spans: {session?.session_id.slice(0, 8)}
          </h2>
          <button
            style={styles.backButton}
            onClick={() => {
              setViewMode('sessions');
              setSpans([]);
            }}
          >
            ← Back
          </button>
        </div>
        {spans.length === 0 ? (
          <div style={styles.emptyState}>No spans recorded for this session.</div>
        ) : (
          spans.map((span, idx) => (
            <div
              key={span.id}
              style={{
                ...styles.spanRow,
                ...(idx === selectedSpanIdx ? styles.spanRowSelected : {}),
              }}
              onClick={() => {
                setSelectedSpanIdx(idx);
                fetchContent(session.session_id, span.id);
                setViewMode('detail');
                setDetailTab('thinking');
              }}
            >
              <span style={styles.spanSeq}>#{span.sequence_num}</span>
              <span style={getModelStyle(span.model)}>{getModelShortName(span.model)}</span>
              <span style={styles.spanDuration}>{formatDuration(span.duration_ms)}</span>
              <div style={styles.spanTokens}>
                <span style={styles.tokenIn}>{span.input_tokens ? formatTokens(span.input_tokens) : '-'}↓</span>
                <span style={styles.tokenOut}>{span.output_tokens ? formatTokens(span.output_tokens) : '-'}↑</span>
              </div>
              {span.tool_names && (
                <span style={styles.toolList}>{span.tool_names}</span>
              )}
            </div>
          ))
        )}
      </div>
    );
  };

  // Detail panel with preview
  const renderDetail = () => {
    const span = spans[selectedSpanIdx];
    const tabContent = getTabContent();

    return (
      <div style={styles.detailPanel}>
        <div style={styles.header}>
          <h3 style={{ ...styles.title, fontSize: '14px' }}>Span Detail</h3>
          <button
            style={styles.backButton}
            onClick={() => {
              setViewMode('spans');
              setContent([]);
            }}
          >
            ← Back
          </button>
        </div>

        {span && (
          <>
            {span.user_preview && (
              <div style={styles.previewSection}>
                <div style={styles.previewLabel}>User:</div>
                <div style={styles.previewText}>{span.user_preview.slice(0, 200)}...</div>
              </div>
            )}
          </>
        )}

        <div style={styles.detailTabs}>
          {(['thinking', 'response', 'tools'] as DetailTab[]).map(tab => (
            <button
              key={tab}
              style={{
                ...styles.tab,
                ...(detailTab === tab ? styles.tabActive : {}),
              }}
              onClick={() => setDetailTab(tab)}
            >
              {tab.charAt(0).toUpperCase() + tab.slice(1)}
            </button>
          ))}
        </div>

        <div style={styles.contentArea}>
          {tabContent || `No ${detailTab} content`}
        </div>
      </div>
    );
  };

  // Main render
  return (
    <div style={styles.container}>
      {viewMode === 'sessions' && renderSessions()}
      {viewMode === 'spans' && (
        <>
          {renderSpans()}
          {spans[selectedSpanIdx] && (
            <div style={{ ...styles.detailPanel, width: '300px' }}>
              <h3 style={{ ...styles.title, fontSize: '14px', marginBottom: '12px' }}>Preview</h3>
              {spans[selectedSpanIdx].thinking_preview && (
                <div style={styles.previewSection}>
                  <div style={styles.previewLabel}>Thinking:</div>
                  <div style={styles.previewText}>
                    {spans[selectedSpanIdx].thinking_preview?.slice(0, 300)}...
                  </div>
                </div>
              )}
              {spans[selectedSpanIdx].response_preview && (
                <div style={styles.previewSection}>
                  <div style={styles.previewLabel}>Response:</div>
                  <div style={styles.previewText}>
                    {spans[selectedSpanIdx].response_preview?.slice(0, 300)}...
                  </div>
                </div>
              )}
            </div>
          )}
        </>
      )}
      {viewMode === 'detail' && (
        <>
          {renderSpans()}
          {renderDetail()}
        </>
      )}
    </div>
  );
};

export default TraceView;
