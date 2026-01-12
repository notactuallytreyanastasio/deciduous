/**
 * Color Constants
 *
 * Node type colors matching the existing CSS exactly.
 * Matches: docs/demo/index.html (lines 155-160) and docs/spelunk-graph.html (lines 354-361)
 */

import type { NodeType, EdgeType } from '../types/graph';

// =============================================================================
// Node Colors
// =============================================================================

export const NODE_COLORS: Record<NodeType, string> = {
  goal: '#22c55e',       // Green
  decision: '#eab308',   // Yellow/amber
  option: '#06b6d4',     // Cyan
  action: '#ef4444',     // Red
  outcome: '#a855f7',    // Purple
  observation: '#6b7280', // Gray
  revisit: '#f97316',    // Orange - pivot point / reconsideration
};

/**
 * Get color for a node type
 */
export function getNodeColor(type: NodeType): string {
  return NODE_COLORS[type] || '#6b7280';
}

// =============================================================================
// Edge Colors
// =============================================================================

export const EDGE_COLORS: Record<EdgeType, string> = {
  leads_to: '#3b82f6',   // Blue
  requires: '#8b5cf6',   // Violet
  chosen: '#22c55e',     // Green
  rejected: '#ef4444',   // Red
  blocks: '#f97316',     // Orange
  enables: '#06b6d4',    // Cyan
};

/**
 * Get color for an edge type
 */
export function getEdgeColor(type: EdgeType): string {
  return EDGE_COLORS[type] || '#6b7280';
}

// =============================================================================
// Confidence Colors
// =============================================================================

export const CONFIDENCE_COLORS = {
  high: {
    bg: '#22c55e33',    // Green with alpha
    text: '#4ade80',
  },
  med: {
    bg: '#eab30833',    // Yellow with alpha
    text: '#fbbf24',
  },
  low: {
    bg: '#ef444433',    // Red with alpha
    text: '#f87171',
  },
};

/**
 * Get confidence badge colors
 */
export function getConfidenceColors(level: 'high' | 'med' | 'low' | null): { bg: string; text: string } | null {
  if (!level) return null;
  return CONFIDENCE_COLORS[level];
}

// =============================================================================
// CSS Classes (for when using class-based styling)
// =============================================================================

/**
 * Get CSS class for node type
 */
export function getNodeTypeClass(type: NodeType): string {
  return `type-${type}`;
}

/**
 * Get CSS class for edge type
 */
export function getEdgeTypeClass(type: EdgeType): string {
  return `edge-${type}`;
}

/**
 * Get CSS class for confidence level
 */
export function getConfidenceClass(level: 'high' | 'med' | 'low' | null): string {
  if (!level) return '';
  return `confidence-${level}`;
}

// =============================================================================
// Theme Colors
// =============================================================================

export const THEME = {
  // Backgrounds
  bgPrimary: '#1a1a2e',
  bgSecondary: '#16213e',
  bgTertiary: '#0f3460',
  bgHover: '#252547',

  // Text
  textPrimary: '#eee',
  textSecondary: '#888',
  textMuted: '#666',

  // Accent
  accent: '#00d9ff',
  accentHover: '#33e5ff',

  // Borders
  border: '#0f3460',
  borderLight: '#333',

  // Commit badge
  commitBg: '#3b82f633',
  commitText: '#60a5fa',
};
