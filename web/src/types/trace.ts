/**
 * Trace Types for API traffic capture
 *
 * These types match the Rust backend structs in src/db.rs
 * for trace_sessions, trace_spans, and trace_content tables.
 */

// =============================================================================
// Trace Session - Groups API calls from one Claude Code run
// =============================================================================

export interface TraceSession {
  id: number;
  session_id: string;
  started_at: string;
  ended_at: string | null;
  working_dir: string | null;
  git_branch: string | null;
  command: string | null;
  summary: string | null;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_read: number;
  total_cache_write: number;
  linked_node_id: number | null;
  linked_change_id: string | null;
  // Enriched fields from API
  display_name: string | null;
  linked_node_title: string | null;
}

// =============================================================================
// Trace Span - Individual API request/response pairs
// =============================================================================

export interface TraceSpan {
  id: number;
  change_id: string;
  session_id: string;
  sequence_num: number;
  started_at: string;
  completed_at: string | null;
  duration_ms: number | null;
  model: string | null;
  request_id: string | null;
  stop_reason: string | null;
  input_tokens: number | null;
  output_tokens: number | null;
  cache_read: number | null;
  cache_write: number | null;
  user_preview: string | null;
  thinking_preview: string | null;
  response_preview: string | null;
  tool_names: string | null;
  linked_node_id: number | null;
  linked_change_id: string | null;
  // Added: count of nodes created during this span
  node_count?: number;
}

// =============================================================================
// Trace Content - Large content stored separately
// =============================================================================

export type TraceContentType = 'thinking' | 'response' | 'tool_input' | 'tool_output' | 'system' | 'tool_definitions';

export interface TraceContent {
  id: number;
  span_id: number;
  content_type: TraceContentType;
  tool_name: string | null;
  tool_use_id: string | null;
  content: string;
  sequence_num: number;
}

// =============================================================================
// Helper functions
// =============================================================================

/**
 * Format token count with K suffix for large numbers
 */
export function formatTokens(count: number): string {
  if (count >= 10000) {
    return `${Math.round(count / 1000)}k`;
  } else if (count >= 1000) {
    return `${(count / 1000).toFixed(1)}k`;
  }
  return String(count);
}

/**
 * Format duration in milliseconds to human readable
 */
export function formatDuration(ms: number | null): string {
  if (ms === null) return '-';
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

/**
 * Format relative time (e.g., "2h ago")
 */
export function formatRelativeTime(timestamp: string): string {
  const date = new Date(timestamp);
  const now = new Date();
  const seconds = Math.floor((now.getTime() - date.getTime()) / 1000);

  if (seconds < 60) return 'now';
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}

/**
 * Get short model name (opus, sonnet, haiku)
 */
export function getModelShortName(model: string | null): string {
  if (!model) return '-';
  if (model.includes('opus')) return 'opus';
  if (model.includes('sonnet')) return 'sonnet';
  if (model.includes('haiku')) return 'haiku';
  return 'model';
}

/**
 * Calculate session duration from start/end timestamps
 */
export function getSessionDuration(session: TraceSession): string {
  if (!session.ended_at) return '...';

  const start = new Date(session.started_at);
  const end = new Date(session.ended_at);
  const secs = Math.floor((end.getTime() - start.getTime()) / 1000);

  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
  return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
}
