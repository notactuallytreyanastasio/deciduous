/**
 * Q&A Interaction Types
 *
 * Types for the Q&A history browser with FTS5 search support.
 * These types match the Rust backend structs in src/db.rs.
 */

// =============================================================================
// Core Types - Match Diesel models exactly
// =============================================================================

export interface QaInteraction {
  id: number;
  user_prompt: string;
  total_prompt: string;
  response: string;
  context_json: string | null;
  inserted_at: string;
  deleted_at: string | null;
}

export interface QaSearchResult {
  interaction: QaInteraction;
  rank: number;           // BM25 relevance score (negative, closer to 0 = more relevant)
  snippet_prompt: string; // Highlighted snippet from user_prompt
  snippet_response: string; // Highlighted snippet from response
}

// =============================================================================
// API Response Types
// =============================================================================

export interface QaListResponse {
  items: QaInteraction[];
  total: number;
}

// =============================================================================
// Parsed Context - Deserialized from context_json
// =============================================================================

export interface QaContext {
  selected_node_id?: number;
  visible_node_ids?: number[];
  current_branch?: string;
  narrative?: {
    name: string;
    root_id: number;
    node_ids: number[];
    pivots: Array<{
      revisit_id: number;
      observation_ids: number[];
      superseded_ids: number[];
      new_approach_ids: number[];
    }>;
    github_links: Array<{
      type: string;
      identifier: string;
      repo: string;
    }>;
  };
}

// =============================================================================
// Helper Functions
// =============================================================================

/**
 * Parse the context_json field into a typed object
 */
export function parseContext(contextJson: string | null): QaContext | null {
  if (!contextJson) return null;
  try {
    return JSON.parse(contextJson) as QaContext;
  } catch {
    return null;
  }
}

/**
 * Format the inserted_at timestamp for display
 */
export function formatTimestamp(isoString: string): string {
  const date = new Date(isoString);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMs / 3600000);
  const diffDays = Math.floor(diffMs / 86400000);

  if (diffMins < 1) return 'just now';
  if (diffMins < 60) return `${diffMins}m ago`;
  if (diffHours < 24) return `${diffHours}h ago`;
  if (diffDays < 7) return `${diffDays}d ago`;

  return date.toLocaleDateString();
}

/**
 * Truncate text to a maximum length with ellipsis
 */
export function truncateText(text: string, maxLength: number): string {
  if (text.length <= maxLength) return text;
  return text.slice(0, maxLength - 3) + '...';
}
