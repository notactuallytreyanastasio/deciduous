/**
 * Trace View
 *
 * Displays API trace sessions and spans captured by `deciduous proxy`.
 * Redesigned with inline expandable content for better UX.
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
// Component
// =============================================================================

const TraceView: React.FC = () => {
  const [sessions, setSessions] = useState<TraceSession[]>([]);
  const [expandedSession, setExpandedSession] = useState<string | null>(null);
  const [spans, setSpans] = useState<TraceSpan[]>([]);
  const [expandedSpan, setExpandedSpan] = useState<number | null>(null);
  const [spanContent, setSpanContent] = useState<Record<number, TraceContent[]>>({});

  // Fetch sessions on mount
  useEffect(() => {
    fetchSessions();
  }, []);

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

  const fetchSpans = async (sessionId: string) => {
    try {
      const res = await fetch(`/api/traces/${sessionId}`);
      const data = await res.json();
      if (data.ok && data.data) {
        setSpans(data.data);
      }
    } catch (e) {
      console.error('Failed to fetch spans:', e);
    }
  };

  const fetchContent = async (sessionId: string, spanId: number) => {
    if (spanContent[spanId]) return; // Already loaded
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
    if (expandedSession === sessionId) {
      setExpandedSession(null);
      setSpans([]);
      setExpandedSpan(null);
    } else {
      setExpandedSession(sessionId);
      fetchSpans(sessionId);
      setExpandedSpan(null);
    }
  };

  const toggleSpan = (spanId: number) => {
    if (expandedSpan === spanId) {
      setExpandedSpan(null);
    } else {
      setExpandedSpan(spanId);
      if (expandedSession) {
        fetchContent(expandedSession, spanId);
      }
    }
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

  const getSystemContent = useCallback((spanId: number) => {
    const items = spanContent[spanId] || [];
    return items.filter(c => c.content_type === 'system').map(c => c.content).join('\n');
  }, [spanContent]);

  // Model badge style
  const getModelStyle = (model: string | null): React.CSSProperties => {
    const name = getModelShortName(model);
    const base: React.CSSProperties = {
      padding: '2px 8px',
      borderRadius: '4px',
      fontSize: '11px',
      fontWeight: 500,
    };
    if (name === 'opus') return { ...base, backgroundColor: '#8b5cf6', color: '#fff' };
    if (name === 'sonnet') return { ...base, backgroundColor: '#3b82f6', color: '#fff' };
    if (name === 'haiku') return { ...base, backgroundColor: '#22c55e', color: '#fff' };
    return { ...base, backgroundColor: '#e5e7eb', color: '#374151' };
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
            const isExpanded = expandedSession === session.session_id;
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
                  {session.linked_node_id && (
                    <span style={styles.linkedBadge}>→ #{session.linked_node_id}</span>
                  )}
                  <span style={styles.sessionTime}>{formatRelativeTime(session.started_at)}</span>
                  <span style={styles.sessionDuration}>{getSessionDuration(session)}</span>
                  <div style={styles.tokenGroup}>
                    <span style={styles.tokenIn}>{formatTokens(session.total_input_tokens)}↓</span>
                    <span style={styles.tokenOut}>{formatTokens(session.total_output_tokens)}↑</span>
                  </div>
                  {session.git_branch && <span style={styles.branch}>{session.git_branch}</span>}
                </div>

                {/* Spans (when session expanded) */}
                {isExpanded && (
                  <div style={styles.spansContainer}>
                    {spans.length === 0 ? (
                      <div style={styles.spanEmpty}>No spans recorded</div>
                    ) : (
                      spans.map(span => {
                        const isSpanExpanded = expandedSpan === span.id;
                        const thinking = getContent(span.id, 'thinking');
                        const response = getContent(span.id, 'response');
                        const tools = getToolsContent(span.id);
                        const system = getSystemContent(span.id);

                        return (
                          <div key={span.id} style={styles.spanWrapper}>
                            {/* Span Header */}
                            <div
                              style={{
                                ...styles.spanRow,
                                ...(isSpanExpanded ? styles.spanRowExpanded : {}),
                              }}
                              onClick={() => toggleSpan(span.id)}
                            >
                              <span style={styles.expandIcon}>{isSpanExpanded ? '▼' : '▶'}</span>
                              <span style={styles.spanNum}>#{span.sequence_num}</span>
                              <span style={getModelStyle(span.model)}>{getModelShortName(span.model)}</span>
                              <span style={styles.spanDuration}>{formatDuration(span.duration_ms)}</span>
                              <div style={styles.tokenGroup}>
                                <span style={styles.tokenIn}>{span.input_tokens ? formatTokens(span.input_tokens) : '-'}↓</span>
                                <span style={styles.tokenOut}>{span.output_tokens ? formatTokens(span.output_tokens) : '-'}↑</span>
                              </div>
                              {span.tool_names && <span style={styles.tools}>{span.tool_names}</span>}
                              {span.node_count && span.node_count > 0 && (
                                <span style={styles.nodeCount}>+{span.node_count}</span>
                              )}
                            </div>

                            {/* Span Content (when expanded) */}
                            {isSpanExpanded && (
                              <div style={styles.spanContent}>
                                {/* User Preview */}
                                {span.user_preview && (
                                  <div style={styles.contentSection}>
                                    <div style={styles.contentLabel}>User</div>
                                    <div style={styles.contentBox}>{span.user_preview}</div>
                                  </div>
                                )}

                                {/* System Prompt */}
                                {system && (
                                  <div style={styles.contentSection}>
                                    <div style={styles.contentLabel}>System Prompt</div>
                                    <div style={{...styles.contentBox, ...styles.systemBox}}>
                                      {system.length > 2000 ? system.slice(0, 2000) + '...' : system}
                                    </div>
                                  </div>
                                )}

                                {/* Thinking */}
                                {(thinking || span.thinking_preview) && (
                                  <div style={styles.contentSection}>
                                    <div style={styles.contentLabel}>Thinking</div>
                                    <div style={{...styles.contentBox, ...styles.thinkingBox}}>
                                      {thinking || span.thinking_preview || 'No thinking content'}
                                    </div>
                                  </div>
                                )}

                                {/* Response */}
                                {(response || span.response_preview) && (
                                  <div style={styles.contentSection}>
                                    <div style={styles.contentLabel}>Response</div>
                                    <div style={styles.contentBox}>
                                      {response || span.response_preview || 'No response content'}
                                    </div>
                                  </div>
                                )}

                                {/* Tools */}
                                {tools.length > 0 && (
                                  <div style={styles.contentSection}>
                                    <div style={styles.contentLabel}>Tools ({tools.length})</div>
                                    {tools.map((tool, idx) => (
                                      <div key={idx} style={styles.toolBlock}>
                                        <div style={styles.toolHeader}>
                                          <span style={styles.toolName}>{tool.name || 'Tool'}</span>
                                          <span style={styles.toolType}>{tool.type === 'tool_input' ? 'input' : 'output'}</span>
                                        </div>
                                        <div style={styles.toolContent}>
                                          {tool.content.length > 500 ? tool.content.slice(0, 500) + '...' : tool.content}
                                        </div>
                                      </div>
                                    ))}
                                  </div>
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
  },
  sessionId: {
    fontFamily: 'monospace',
    fontSize: '14px',
    fontWeight: 600,
    color: '#0969da',
  },
  linkedBadge: {
    backgroundColor: '#dcfce7',
    color: '#16a34a',
    padding: '2px 8px',
    borderRadius: '4px',
    fontSize: '11px',
    fontWeight: 500,
  },
  sessionTime: {
    color: '#6b7280',
    fontSize: '12px',
  },
  sessionDuration: {
    color: '#6b7280',
    fontSize: '12px',
  },
  tokenGroup: {
    display: 'flex',
    gap: '8px',
    fontSize: '12px',
  },
  tokenIn: {
    color: '#16a34a',
  },
  tokenOut: {
    color: '#dc2626',
  },
  branch: {
    backgroundColor: '#dbeafe',
    color: '#1d4ed8',
    padding: '2px 8px',
    borderRadius: '4px',
    fontSize: '11px',
    marginLeft: 'auto',
  },
  spansContainer: {
    padding: '8px 16px 16px 28px',
    backgroundColor: '#fafafa',
  },
  spanEmpty: {
    color: '#9ca3af',
    fontSize: '13px',
    padding: '12px',
  },
  spanWrapper: {
    marginBottom: '4px',
  },
  spanRow: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    padding: '8px 12px',
    backgroundColor: '#fff',
    border: '1px solid #e5e7eb',
    borderRadius: '6px',
    cursor: 'pointer',
    fontSize: '13px',
  },
  spanRowExpanded: {
    backgroundColor: '#fffbeb',
    borderColor: '#fcd34d',
    borderBottomLeftRadius: 0,
    borderBottomRightRadius: 0,
  },
  spanNum: {
    fontFamily: 'monospace',
    color: '#6b7280',
    minWidth: '35px',
  },
  spanDuration: {
    color: '#6b7280',
    minWidth: '50px',
  },
  tools: {
    color: '#ea580c',
    fontSize: '12px',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
    maxWidth: '200px',
  },
  nodeCount: {
    backgroundColor: '#dcfce7',
    color: '#16a34a',
    padding: '2px 6px',
    borderRadius: '4px',
    fontSize: '11px',
    fontWeight: 500,
    marginLeft: 'auto',
  },
  spanContent: {
    backgroundColor: '#fffbeb',
    border: '1px solid #fcd34d',
    borderTop: 'none',
    borderBottomLeftRadius: '6px',
    borderBottomRightRadius: '6px',
    padding: '12px',
  },
  contentSection: {
    marginBottom: '12px',
  },
  contentLabel: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#92400e',
    textTransform: 'uppercase',
    marginBottom: '6px',
  },
  contentBox: {
    backgroundColor: '#fff',
    border: '1px solid #e5e7eb',
    borderRadius: '6px',
    padding: '10px',
    fontSize: '12px',
    lineHeight: 1.5,
    whiteSpace: 'pre-wrap',
    wordBreak: 'break-word',
    maxHeight: '200px',
    overflow: 'auto',
    color: '#374151',
  },
  thinkingBox: {
    backgroundColor: '#fef3c7',
    borderColor: '#fcd34d',
    color: '#92400e',
  },
  systemBox: {
    backgroundColor: '#f3e8ff',
    borderColor: '#c084fc',
    color: '#6b21a8',
    fontSize: '11px',
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
    color: '#ea580c',
  },
  toolType: {
    fontSize: '10px',
    color: '#9ca3af',
    textTransform: 'uppercase',
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
  noContent: {
    color: '#9ca3af',
    fontSize: '12px',
    textAlign: 'center',
    padding: '16px',
  },
};

export default TraceView;
