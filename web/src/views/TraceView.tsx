/**
 * Trace View
 *
 * Displays API trace sessions and spans captured by `deciduous proxy`.
 * Redesigned with:
 * - Multiple expandable sessions and spans
 * - Better hierarchy visualization
 * - Clearer content display
 */

import React, { useState, useEffect, useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';
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
// Component
// =============================================================================

const TraceView: React.FC = () => {
  const [searchParams, setSearchParams] = useSearchParams();
  const [sessions, setSessions] = useState<TraceSession[]>([]);
  const [expandedSessions, setExpandedSessions] = useState<Set<string>>(new Set());
  const [sessionSpans, setSessionSpans] = useState<Record<string, TraceSpan[]>>({});
  const [expandedSpans, setExpandedSpans] = useState<Set<number>>(new Set());
  const [spanContent, setSpanContent] = useState<Record<number, TraceContent[]>>({});
  const [initialNavDone, setInitialNavDone] = useState(false);

  // Fetch sessions on mount
  useEffect(() => {
    fetchSessions();
  }, []);

  // Handle URL params for deep linking (after sessions load)
  useEffect(() => {
    if (sessions.length === 0 || initialNavDone) return;

    const sessionParam = searchParams.get('session');
    const spanParam = searchParams.get('span');

    if (sessionParam) {
      const matchingSession = sessions.find(s => s.session_id.startsWith(sessionParam));
      if (matchingSession) {
        setExpandedSessions(new Set([matchingSession.session_id]));
        fetchSpans(matchingSession.session_id).then(() => {
          if (spanParam) {
            const spanId = parseInt(spanParam, 10);
            if (!isNaN(spanId)) {
              setExpandedSpans(new Set([spanId]));
              fetchContent(matchingSession.session_id, spanId);
            }
          }
        });
      }
      setSearchParams({}, { replace: true });
    }
    setInitialNavDone(true);
  }, [sessions, searchParams, initialNavDone]);

  const fetchSessions = async () => {
    try {
      const res = await fetch('/api/traces');
      const data = await res.json();
      if (data.ok && data.data) {
        setSessions(data.data);
      }
    } catch (e) {
      console.error('Failed to fetch traces:', e);
    }
  };

  const fetchSpans = async (sessionId: string): Promise<TraceSpan[]> => {
    if (sessionSpans[sessionId]) return sessionSpans[sessionId];
    try {
      const res = await fetch(`/api/traces/${sessionId}`);
      const data = await res.json();
      if (data.ok && data.data) {
        setSessionSpans(prev => ({ ...prev, [sessionId]: data.data }));
        return data.data;
      }
    } catch (e) {
      console.error('Failed to fetch spans:', e);
    }
    return [];
  };

  const fetchContent = async (sessionId: string, spanId: number) => {
    if (spanContent[spanId]) return;
    try {
      const res = await fetch(`/api/traces/${sessionId}/spans/${spanId}`);
      const data = await res.json();
      if (data.ok && data.data) {
        setSpanContent(prev => ({ ...prev, [spanId]: data.data }));
      }
    } catch (e) {
      console.error('Failed to fetch content:', e);
    }
  };

  const toggleSession = (sessionId: string) => {
    setExpandedSessions(prev => {
      const next = new Set(prev);
      if (next.has(sessionId)) {
        next.delete(sessionId);
      } else {
        next.add(sessionId);
        fetchSpans(sessionId);
      }
      return next;
    });
  };

  const toggleSpan = (sessionId: string, spanId: number) => {
    setExpandedSpans(prev => {
      const next = new Set(prev);
      if (next.has(spanId)) {
        next.delete(spanId);
      } else {
        next.add(spanId);
        fetchContent(sessionId, spanId);
      }
      return next;
    });
  };

  // Get content helpers
  const getContent = useCallback((spanId: number, type: string) => {
    const items = spanContent[spanId] || [];
    return items.filter(c => c.content_type === type).map(c => c.content).join('\n');
  }, [spanContent]);

  const getToolsContent = useCallback((spanId: number) => {
    const items = spanContent[spanId] || [];
    return items
      .filter(c => c.content_type === 'tool_input' || c.content_type === 'tool_output')
      .map(c => ({ name: c.tool_name, type: c.content_type, content: c.content }));
  }, [spanContent]);

  // Model badge style
  const getModelStyle = (model: string | null): React.CSSProperties => {
    const name = getModelShortName(model);
    const base: React.CSSProperties = {
      padding: '2px 8px',
      borderRadius: '4px',
      fontSize: '11px',
      fontWeight: 500,
      flexShrink: 0,
    };
    if (name === 'opus') return { ...base, backgroundColor: '#8b5cf6', color: '#fff' };
    if (name === 'sonnet') return { ...base, backgroundColor: '#3b82f6', color: '#fff' };
    if (name === 'haiku') return { ...base, backgroundColor: '#22c55e', color: '#fff' };
    return { ...base, backgroundColor: '#e5e7eb', color: '#374151' };
  };

  // Determine span type for visual grouping
  const getSpanType = (span: TraceSpan): 'main' | 'subagent' | 'tool' => {
    // Subagents are typically haiku calls with specific patterns
    const model = getModelShortName(span.model);
    if (span.tool_names?.includes('Task')) return 'main';
    if (model === 'haiku' && !span.tool_names) return 'subagent';
    if (span.tool_names && span.tool_names.split(',').length > 2) return 'tool';
    return 'main';
  };

  return (
    <div style={styles.container}>
      <div style={styles.header}>
        <h2 style={styles.title}>API Traces</h2>
        <button style={styles.refreshBtn} onClick={fetchSessions}>↻ Refresh</button>
      </div>

      {sessions.length === 0 ? (
        <div style={styles.empty}>
          <p>No trace sessions found.</p>
          <p style={styles.emptyHint}>Run <code style={styles.code}>deciduous proxy -- claude</code> to capture API traffic.</p>
        </div>
      ) : (
        <div style={styles.list}>
          {sessions.map(session => {
            const isExpanded = expandedSessions.has(session.session_id);
            const spans = sessionSpans[session.session_id] || [];

            return (
              <div key={session.session_id} style={styles.sessionWrapper}>
                {/* Session Header */}
                <div
                  style={{
                    ...styles.sessionRow,
                    ...(isExpanded ? styles.sessionRowExpanded : {}),
                    ...(session.linked_node_id ? styles.sessionRowLinked : {}),
                  }}
                  onClick={() => toggleSession(session.session_id)}
                >
                  <span style={styles.expandIcon}>{isExpanded ? '▼' : '▶'}</span>
                  <span style={styles.sessionId}>{session.session_id.slice(0, 8)}</span>
                  <span style={styles.sessionMeta}>
                    {formatRelativeTime(session.started_at)} · {getSessionDuration(session)} · {spans.length || '…'} spans
                  </span>
                  <div style={styles.tokenGroup}>
                    <span style={styles.tokenIn}>{formatTokens(session.total_input_tokens)}↓</span>
                    <span style={styles.tokenOut}>{formatTokens(session.total_output_tokens)}↑</span>
                  </div>
                  {/* Display name - the most important info */}
                  <span style={styles.displayName}>
                    {session.display_name
                      ? (session.display_name.length > 70 ? session.display_name.slice(0, 70) + '…' : session.display_name)
                      : <span style={styles.noPreview}>(no user prompt captured)</span>
                    }
                  </span>
                  {session.linked_node_id && (
                    <span style={styles.linkedBadge}>#{session.linked_node_id}</span>
                  )}
                  {session.git_branch && <span style={styles.branch}>{session.git_branch}</span>}
                </div>

                {/* Spans (when session expanded) */}
                {isExpanded && (
                  <div style={styles.spansContainer}>
                    {spans.length === 0 ? (
                      <div style={styles.spanEmpty}>Loading spans...</div>
                    ) : (
                      spans.map(span => {
                        const isSpanExpanded = expandedSpans.has(span.id);
                        const spanType = getSpanType(span);
                        const thinking = getContent(span.id, 'thinking');
                        const response = getContent(span.id, 'response');
                        const tools = getToolsContent(span.id);
                        const system = getContent(span.id, 'system');

                        // Build span summary
                        const spanSummary = span.user_preview
                          || span.response_preview?.slice(0, 60)
                          || (span.tool_names ? `Tools: ${span.tool_names}` : null)
                          || '(API call)';

                        return (
                          <div
                            key={span.id}
                            style={{
                              ...styles.spanWrapper,
                              ...(spanType === 'subagent' ? styles.spanIndented : {}),
                            }}
                          >
                            {/* Span Header */}
                            <div
                              style={{
                                ...styles.spanRow,
                                ...(isSpanExpanded ? styles.spanRowExpanded : {}),
                                ...(spanType === 'subagent' ? styles.spanRowSubagent : {}),
                              }}
                              onClick={() => toggleSpan(session.session_id, span.id)}
                            >
                              <span style={styles.expandIcon}>{isSpanExpanded ? '▼' : '▶'}</span>
                              <span style={styles.spanNum}>#{span.sequence_num}</span>
                              <span style={getModelStyle(span.model)}>{getModelShortName(span.model)}</span>
                              <span style={styles.spanDuration}>{formatDuration(span.duration_ms)}</span>
                              <div style={styles.tokenGroupSmall}>
                                <span style={styles.tokenIn}>{span.input_tokens ? formatTokens(span.input_tokens) : '-'}↓</span>
                                <span style={styles.tokenOut}>{span.output_tokens ? formatTokens(span.output_tokens) : '-'}↑</span>
                              </div>
                              {span.tool_names && (
                                <span style={styles.toolBadge}>{span.tool_names.split(',').length} tools</span>
                              )}
                              {span.node_count && span.node_count > 0 && (
                                <span style={styles.nodeCount}>+{span.node_count} nodes</span>
                              )}
                              {/* Span summary - most important */}
                              <span style={styles.spanSummary}>
                                {spanSummary.length > 60 ? spanSummary.slice(0, 60) + '…' : spanSummary}
                              </span>
                            </div>

                            {/* Span Content (when expanded) */}
                            {isSpanExpanded && (
                              <div style={styles.spanContent}>
                                {/* Quick stats bar */}
                                <div style={styles.statsBar}>
                                  {span.model && <span>Model: {span.model}</span>}
                                  {span.stop_reason && <span>Stop: {span.stop_reason}</span>}
                                  {span.cache_read && span.cache_read > 0 && (
                                    <span style={styles.cacheHit}>Cache: {formatTokens(span.cache_read)} read</span>
                                  )}
                                </div>

                                {/* User Message */}
                                {span.user_preview && (
                                  <div style={styles.contentSection}>
                                    <div style={styles.contentLabel}>👤 USER</div>
                                    <div style={styles.userBox}>{span.user_preview}</div>
                                  </div>
                                )}

                                {/* Thinking */}
                                {(thinking || span.thinking_preview) && (
                                  <div style={styles.contentSection}>
                                    <div style={styles.contentLabel}>💭 THINKING</div>
                                    <div style={styles.thinkingBox}>
                                      {thinking || span.thinking_preview}
                                    </div>
                                  </div>
                                )}

                                {/* Response */}
                                {(response || span.response_preview) && (
                                  <div style={styles.contentSection}>
                                    <div style={styles.contentLabel}>💬 RESPONSE</div>
                                    <div style={styles.responseBox}>
                                      {response || span.response_preview}
                                    </div>
                                  </div>
                                )}

                                {/* Tools */}
                                {(tools.length > 0 || span.tool_names) && (
                                  <div style={styles.contentSection}>
                                    <div style={styles.contentLabel}>🔧 TOOLS {span.tool_names && `(${span.tool_names})`}</div>
                                    {tools.length > 0 ? (
                                      tools.map((tool, idx) => (
                                        <div key={idx} style={styles.toolBlock}>
                                          <div style={styles.toolHeader}>
                                            <span style={styles.toolName}>{tool.name || 'Tool'}</span>
                                            <span style={styles.toolType}>{tool.type === 'tool_input' ? 'INPUT' : 'OUTPUT'}</span>
                                          </div>
                                          <div style={styles.toolContent}>
                                            {tool.content.length > 500 ? tool.content.slice(0, 500) + '…' : tool.content}
                                          </div>
                                        </div>
                                      ))
                                    ) : (
                                      <div style={styles.noContent}>Tool details not captured</div>
                                    )}
                                  </div>
                                )}

                                {/* System prompt (collapsed by default) */}
                                {system && (
                                  <details style={styles.systemDetails}>
                                    <summary style={styles.systemSummary}>📋 System Prompt ({system.length} chars)</summary>
                                    <div style={styles.systemBox}>
                                      {system.length > 2000 ? system.slice(0, 2000) + '…' : system}
                                    </div>
                                  </details>
                                )}

                                {!thinking && !response && !span.thinking_preview && !span.response_preview && tools.length === 0 && (
                                  <div style={styles.noContent}>No content recorded for this span</div>
                                )}
                              </div>
                            )}
                          </div>
                        );
                      })
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};

// =============================================================================
// Styles
// =============================================================================

const styles: Record<string, React.CSSProperties> = {
  container: {
    height: '100%',
    overflow: 'auto',
    backgroundColor: '#fafafa',
    padding: '20px',
    color: '#374151',
  },
  header: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: '20px',
  },
  title: {
    fontSize: '20px',
    fontWeight: 600,
    color: '#111827',
    margin: 0,
  },
  refreshBtn: {
    padding: '6px 12px',
    border: '1px solid #d1d5db',
    borderRadius: '6px',
    backgroundColor: '#fff',
    color: '#374151',
    cursor: 'pointer',
    fontSize: '13px',
  },
  empty: {
    textAlign: 'center',
    padding: '60px 20px',
    color: '#6b7280',
  },
  emptyHint: {
    fontSize: '13px',
    marginTop: '8px',
  },
  code: {
    backgroundColor: '#f3f4f6',
    padding: '2px 6px',
    borderRadius: '4px',
    fontFamily: 'monospace',
    fontSize: '12px',
    color: '#0369a1',
  },
  list: {
    display: 'flex',
    flexDirection: 'column',
    gap: '8px',
  },
  sessionWrapper: {
    borderRadius: '8px',
    border: '1px solid #e5e7eb',
    backgroundColor: '#fff',
    overflow: 'hidden',
  },
  sessionRow: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    padding: '12px 16px',
    cursor: 'pointer',
    transition: 'background-color 0.15s',
  },
  sessionRowExpanded: {
    backgroundColor: '#f9fafb',
    borderBottom: '1px solid #e5e7eb',
  },
  sessionRowLinked: {
    borderLeft: '3px solid #22c55e',
  },
  expandIcon: {
    color: '#9ca3af',
    fontSize: '10px',
    width: '12px',
    flexShrink: 0,
  },
  sessionId: {
    fontFamily: 'monospace',
    fontSize: '13px',
    fontWeight: 600,
    color: '#0969da',
    flexShrink: 0,
  },
  sessionMeta: {
    color: '#6b7280',
    fontSize: '12px',
    flexShrink: 0,
  },
  linkedBadge: {
    backgroundColor: '#dcfce7',
    color: '#16a34a',
    padding: '2px 8px',
    borderRadius: '4px',
    fontSize: '11px',
    fontWeight: 500,
    flexShrink: 0,
  },
  tokenGroup: {
    display: 'flex',
    gap: '8px',
    fontSize: '12px',
    flexShrink: 0,
  },
  tokenGroupSmall: {
    display: 'flex',
    gap: '6px',
    fontSize: '11px',
    flexShrink: 0,
  },
  tokenIn: {
    color: '#16a34a',
  },
  tokenOut: {
    color: '#dc2626',
  },
  displayName: {
    color: '#374151',
    fontSize: '13px',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
    flex: 1,
  },
  noPreview: {
    color: '#9ca3af',
    fontStyle: 'italic',
  },
  branch: {
    backgroundColor: '#dbeafe',
    color: '#1d4ed8',
    padding: '2px 8px',
    borderRadius: '4px',
    fontSize: '11px',
    flexShrink: 0,
  },
  spansContainer: {
    padding: '8px 12px 12px 24px',
    backgroundColor: '#f9fafb',
  },
  spanEmpty: {
    color: '#9ca3af',
    fontSize: '13px',
    padding: '12px',
  },
  spanWrapper: {
    marginBottom: '4px',
  },
  spanIndented: {
    marginLeft: '20px',
  },
  spanRow: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '8px 12px',
    backgroundColor: '#fff',
    border: '1px solid #e5e7eb',
    borderRadius: '6px',
    cursor: 'pointer',
    fontSize: '12px',
  },
  spanRowExpanded: {
    backgroundColor: '#fffbeb',
    borderColor: '#fcd34d',
    borderBottomLeftRadius: 0,
    borderBottomRightRadius: 0,
  },
  spanRowSubagent: {
    backgroundColor: '#f9fafb',
    borderColor: '#e5e7eb',
  },
  spanNum: {
    fontFamily: 'monospace',
    color: '#6b7280',
    minWidth: '30px',
    flexShrink: 0,
  },
  spanDuration: {
    color: '#6b7280',
    minWidth: '45px',
    flexShrink: 0,
  },
  toolBadge: {
    backgroundColor: '#f3e8ff',
    color: '#7c3aed',
    padding: '2px 6px',
    borderRadius: '4px',
    fontSize: '10px',
    flexShrink: 0,
  },
  nodeCount: {
    backgroundColor: '#dcfce7',
    color: '#16a34a',
    padding: '2px 6px',
    borderRadius: '4px',
    fontSize: '10px',
    flexShrink: 0,
  },
  spanSummary: {
    color: '#6b7280',
    fontSize: '12px',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
    flex: 1,
  },
  spanContent: {
    backgroundColor: '#fffbeb',
    border: '1px solid #fcd34d',
    borderTop: 'none',
    borderBottomLeftRadius: '6px',
    borderBottomRightRadius: '6px',
    padding: '12px',
  },
  statsBar: {
    display: 'flex',
    gap: '16px',
    fontSize: '11px',
    color: '#6b7280',
    marginBottom: '12px',
    paddingBottom: '8px',
    borderBottom: '1px solid #e5e7eb',
  },
  cacheHit: {
    color: '#16a34a',
  },
  contentSection: {
    marginBottom: '12px',
  },
  contentLabel: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#92400e',
    marginBottom: '6px',
    letterSpacing: '0.5px',
  },
  userBox: {
    backgroundColor: '#eff6ff',
    border: '1px solid #93c5fd',
    borderRadius: '6px',
    padding: '10px',
    fontSize: '12px',
    lineHeight: 1.5,
    whiteSpace: 'pre-wrap',
    wordBreak: 'break-word',
    maxHeight: '150px',
    overflow: 'auto',
    color: '#1e40af',
  },
  thinkingBox: {
    backgroundColor: '#fef3c7',
    border: '1px solid #fcd34d',
    borderRadius: '6px',
    padding: '10px',
    fontSize: '12px',
    lineHeight: 1.5,
    whiteSpace: 'pre-wrap',
    wordBreak: 'break-word',
    maxHeight: '200px',
    overflow: 'auto',
    color: '#92400e',
  },
  responseBox: {
    backgroundColor: '#f0fdf4',
    border: '1px solid #86efac',
    borderRadius: '6px',
    padding: '10px',
    fontSize: '12px',
    lineHeight: 1.5,
    whiteSpace: 'pre-wrap',
    wordBreak: 'break-word',
    maxHeight: '200px',
    overflow: 'auto',
    color: '#166534',
  },
  toolBlock: {
    marginBottom: '8px',
    backgroundColor: '#fff',
    border: '1px solid #e5e7eb',
    borderRadius: '6px',
    overflow: 'hidden',
  },
  toolHeader: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: '6px 10px',
    backgroundColor: '#f9fafb',
    borderBottom: '1px solid #e5e7eb',
  },
  toolName: {
    fontWeight: 600,
    fontSize: '12px',
    color: '#7c3aed',
  },
  toolType: {
    fontSize: '10px',
    color: '#9ca3af',
  },
  toolContent: {
    padding: '8px 10px',
    fontSize: '11px',
    fontFamily: 'monospace',
    whiteSpace: 'pre-wrap',
    wordBreak: 'break-word',
    maxHeight: '120px',
    overflow: 'auto',
    color: '#374151',
  },
  systemDetails: {
    marginTop: '8px',
  },
  systemSummary: {
    cursor: 'pointer',
    fontSize: '11px',
    color: '#6b7280',
    padding: '4px 0',
  },
  systemBox: {
    backgroundColor: '#f3e8ff',
    border: '1px solid #c4b5fd',
    borderRadius: '6px',
    padding: '10px',
    fontSize: '11px',
    lineHeight: 1.4,
    whiteSpace: 'pre-wrap',
    wordBreak: 'break-word',
    maxHeight: '150px',
    overflow: 'auto',
    color: '#6b21a8',
    marginTop: '6px',
  },
  noContent: {
    color: '#9ca3af',
    fontSize: '12px',
    textAlign: 'center',
    padding: '16px',
    fontStyle: 'italic',
  },
};

export default TraceView;
