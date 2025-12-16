/**
 * Decision Graph Types
 *
 * These types MUST match:
 * - Rust backend structs in src/db.rs
 * - TUI types in src/tui/types.rs
 * - JSON Schema in schema/decision-graph.schema.json
 *
 * All three sources must stay in sync for consistent behavior.
 */

// =============================================================================
// Node Types - matches schema CHECK constraint
// =============================================================================

export const NODE_TYPES = ['goal', 'decision', 'option', 'action', 'outcome', 'observation'] as const;
export type NodeType = typeof NODE_TYPES[number];

export const NODE_STATUSES = ['pending', 'active', 'completed', 'rejected'] as const;
export type NodeStatus = typeof NODE_STATUSES[number];

// =============================================================================
// Edge Types - matches schema CHECK constraint
// =============================================================================

export const EDGE_TYPES = ['leads_to', 'requires', 'chosen', 'rejected', 'blocks', 'enables'] as const;
export type EdgeType = typeof EDGE_TYPES[number];

// =============================================================================
// Metadata - stored as JSON string in metadata_json field
// =============================================================================

export interface NodeMetadata {
  confidence?: number;  // 0-100 confidence score
  commit?: string;      // Git commit hash (full 40 chars)
  prompt?: string;      // User prompt that triggered this decision
  files?: string[];     // Associated files
  branch?: string;      // Git branch this node was created on
  [key: string]: unknown;  // Allow extension
}

// =============================================================================
// Core Types - Match Diesel models exactly
// =============================================================================

import {
  DecisionNode as GeneratedDecisionNode,
  DecisionEdge as GeneratedDecisionEdge,
} from './generated/schema';

// Re-export generated types as the source of truth
// Note: We extend the generated types to ensure string fields match our specific unions (NodeType/EdgeType)
export interface DecisionNode extends Omit<GeneratedDecisionNode, 'node_type' | 'status'> {
  node_type: NodeType;
  status: NodeStatus;
}

export interface DecisionEdge extends Omit<GeneratedDecisionEdge, 'edge_type'> {
  edge_type: EdgeType;
}

export type { DecisionContext, DecisionSession, CommandLog } from './generated/schema';

/**
 * GitHub configuration for external repo links
 */
export interface GithubConfig {
  commit_repo?: string;  // e.g., "phoenixframework/phoenix"
}

/**
 * Branch configuration
 */
export interface BranchConfig {
  main_branches?: string[];
  auto_detect?: boolean;
}

/**
 * Configuration from .deciduous/config.toml
 */
export interface DeciduousConfig {
  github?: GithubConfig;
  branch?: BranchConfig;
}

/**
 * Full graph data structure as exported by `deciduous sync`
 * This is the JSON format written to graph-data.json
 */
export interface GraphData {
  nodes: DecisionNode[];
  edges: DecisionEdge[];
  config?: DeciduousConfig;  // Optional config for external repo links
}

// =============================================================================
// Computed/Derived Types - Used by UI
// =============================================================================

/**
 * Node with parsed metadata for easier access
 */
export interface ParsedNode extends Omit<DecisionNode, 'metadata_json'> {
  metadata: NodeMetadata | null;
  confidence: number | null;
  commit: string | null;
  prompt: string | null;
  files: string[] | null;
  branch: string | null;
}

/**
 * Chain - a connected subgraph starting from a root node
 */
export interface Chain {
  root: DecisionNode;
  nodes: DecisionNode[];
  edges: DecisionEdge[];
}

/**
 * Session - nodes grouped by time proximity
 */
export interface Session {
  startTime: number;  // Unix timestamp ms
  endTime: number;    // Unix timestamp ms
  nodes: DecisionNode[];
  chains: Chain[];
}

/**
 * Git commit from git-history.json (for timeline view)
 */
export interface GitCommit {
  hash: string;
  short_hash: string;
  author: string;
  date: string;  // ISO 8601
  message: string;
  files_changed?: number;
}

/**
 * Merged timeline item - either a decision node or git commit
 */
export interface TimelineItem {
  type: 'node' | 'commit';
  timestamp: Date;
  node?: DecisionNode;
  commit?: GitCommit;
  linkedNodes?: DecisionNode[];  // Nodes linked to this commit
  linkedCommits?: GitCommit[];   // Commits linked to this node
}

// =============================================================================
// Helper Functions - Preserve existing logic exactly
// =============================================================================

/**
 * Parse metadata_json string into NodeMetadata object
 * Matches: docs/src/types/graph.ts parseMetadata (lines 76-83)
 */
export function parseMetadata(json: string | null): NodeMetadata | null {
  if (!json) return null;
  try {
    return JSON.parse(json) as NodeMetadata;
  } catch {
    return null;
  }
}

/**
 * Extract confidence from a node
 * Matches: docs/demo/index.html getConfidence (lines 742-748)
 */
export function getConfidence(node: DecisionNode): number | null {
  const meta = parseMetadata(node.metadata_json);
  return meta?.confidence ?? null;
}

/**
 * Extract commit hash from a node
 * Matches: docs/demo/index.html getCommit (lines 750-756)
 */
export function getCommit(node: DecisionNode): string | null {
  const meta = parseMetadata(node.metadata_json);
  return meta?.commit ?? null;
}

/**
 * Extract branch from a node
 */
export function getBranch(node: DecisionNode): string | null {
  const meta = parseMetadata(node.metadata_json);
  return meta?.branch ?? null;
}

/**
 * Extract prompt from a node
 */
export function getPrompt(node: DecisionNode): string | null {
  const meta = parseMetadata(node.metadata_json);
  return meta?.prompt ?? null;
}

/**
 * Extract associated files from a node
 */
export function getFiles(node: DecisionNode): string[] | null {
  const meta = parseMetadata(node.metadata_json);
  return meta?.files ?? null;
}

/**
 * Get all unique branches from a list of nodes
 */
export function getUniqueBranches(nodes: DecisionNode[]): string[] {
  const branches = new Set<string>();
  for (const node of nodes) {
    const branch = getBranch(node);
    if (branch) branches.add(branch);
  }
  return Array.from(branches).sort();
}

/**
 * Get short commit hash (7 chars)
 */
export function shortCommit(commit: string | null): string | null {
  if (!commit) return null;
  return commit.slice(0, 7);
}

/**
 * Get confidence level category
 * Matches: docs/demo/index.html confidenceBadge logic (lines 758-762)
 */
export function getConfidenceLevel(confidence: number | null): 'high' | 'med' | 'low' | null {
  if (confidence === null) return null;
  if (confidence >= 70) return 'high';
  if (confidence >= 40) return 'med';
  return 'low';
}

/**
 * Default repository for commit links (when no config is provided)
 */
export const DEFAULT_COMMIT_REPO = 'notactuallytreyanastasio/deciduous';

/**
 * Get the commit repo from graph config, with fallback to default
 */
export function getCommitRepo(graphData: GraphData | null): string {
  return graphData?.config?.github?.commit_repo ?? DEFAULT_COMMIT_REPO;
}

/**
 * Create GitHub commit URL
 */
export function githubCommitUrl(commit: string, repo: string = DEFAULT_COMMIT_REPO): string {
  return `https://github.com/${repo}/commit/${commit}`;
}

/**
 * Create GitHub PR URL
 */
export function githubPrUrl(prNumber: number | string, repo: string = DEFAULT_COMMIT_REPO): string {
  return `https://github.com/${repo}/pull/${prNumber}`;
}

/**
 * Truncate string with ellipsis
 * Matches: docs/demo/index.html truncate (lines 728-730)
 */
export function truncate(str: string | null | undefined, len: number): string {
  if (!str) return '';
  return str.length > len ? str.substring(0, len) + '...' : str;
}

/**
 * Format duration between two timestamps
 * Matches: docs/demo/index.html getDuration (lines 732-740)
 */
export function getDuration(start: string, end: string): string {
  const ms = new Date(end).getTime() - new Date(start).getTime();
  const mins = Math.floor(ms / 60000);
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ${mins % 60}m`;
  const days = Math.floor(hours / 24);
  return `${days}d ${hours % 24}h`;
}

/**
 * Convert DecisionNode to ParsedNode with extracted metadata
 */
export function parseNode(node: DecisionNode): ParsedNode {
  const metadata = parseMetadata(node.metadata_json);
  return {
    id: node.id,
    change_id: node.change_id,
    node_type: node.node_type,
    title: node.title,
    description: node.description,
    status: node.status,
    created_at: node.created_at,
    updated_at: node.updated_at,
    metadata,
    confidence: metadata?.confidence ?? null,
    commit: metadata?.commit ?? null,
    prompt: metadata?.prompt ?? null,
    files: metadata?.files ?? null,
    branch: metadata?.branch ?? null,
  };
}

/**
 * Type guard for NodeType
 */
export function isNodeType(value: string): value is NodeType {
  return NODE_TYPES.includes(value as NodeType);
}

/**
 * Type guard for EdgeType
 */
export function isEdgeType(value: string): value is EdgeType {
  return EDGE_TYPES.includes(value as EdgeType);
}

// =============================================================================
// Filter Functions - Mirrors TUI state.rs
// =============================================================================

/**
 * Filter nodes by type
 * Mirrors: src/tui/state.rs filter_by_type
 */
export function filterByType(nodes: DecisionNode[], typeFilter: NodeType | null): DecisionNode[] {
  if (!typeFilter) return nodes;
  return nodes.filter(n => n.node_type === typeFilter);
}

/**
 * Filter nodes by branch
 * Mirrors: src/tui/state.rs filter_by_branch
 */
export function filterByBranch(nodes: DecisionNode[], branch: string | null): DecisionNode[] {
  if (!branch) return nodes;
  return nodes.filter(n => getBranch(n) === branch);
}

/**
 * Filter nodes by search query (title and description)
 * Mirrors: src/tui/state.rs filter_by_search
 */
export function filterBySearch(nodes: DecisionNode[], query: string): DecisionNode[] {
  if (!query) return nodes;
  const lowerQuery = query.toLowerCase();
  return nodes.filter(n =>
    n.title.toLowerCase().includes(lowerQuery) ||
    (n.description?.toLowerCase().includes(lowerQuery) ?? false)
  );
}

/**
 * Sort nodes by created_at timestamp
 * Mirrors: src/tui/state.rs sort_by_time
 */
export function sortByTime(nodes: DecisionNode[], oldestFirst: boolean): DecisionNode[] {
  const sorted = [...nodes];
  sorted.sort((a, b) => {
    const timeA = new Date(a.created_at).getTime();
    const timeB = new Date(b.created_at).getTime();
    return oldestFirst ? timeA - timeB : timeB - timeA;
  });
  return sorted;
}

/**
 * Apply all filters and sorting
 * Mirrors: src/tui/state.rs apply_all_filters
 */
export function applyAllFilters(
  nodes: DecisionNode[],
  typeFilter: NodeType | null,
  branchFilter: string | null,
  searchQuery: string,
  oldestFirst: boolean
): DecisionNode[] {
  let filtered = filterByType(nodes, typeFilter);
  filtered = filterByBranch(filtered, branchFilter);
  filtered = filterBySearch(filtered, searchQuery);
  return sortByTime(filtered, oldestFirst);
}

/**
 * Cycle through type filters
 * Mirrors: src/tui/state.rs cycle_type_filter
 */
export function cycleTypeFilter(current: NodeType | null): NodeType | null {
  if (!current) return NODE_TYPES[0];
  const idx = NODE_TYPES.indexOf(current);
  if (idx === -1 || idx + 1 >= NODE_TYPES.length) return null;
  return NODE_TYPES[idx + 1];
}

/**
 * Cycle through branch filters
 * Mirrors: src/tui/state.rs cycle_branch_filter
 */
export function cycleBranchFilter(current: string | null, branches: string[]): string | null {
  if (branches.length === 0) return null;
  if (!current) return branches[0];
  const idx = branches.indexOf(current);
  if (idx === -1 || idx + 1 >= branches.length) return null;
  return branches[idx + 1];
}

/**
 * Get incoming edges for a node
 * Mirrors: src/tui/types.rs get_incoming_edges
 */
export function getIncomingEdges(nodeId: number, edges: DecisionEdge[]): DecisionEdge[] {
  return edges.filter(e => e.to_node_id === nodeId);
}

/**
 * Get outgoing edges from a node
 * Mirrors: src/tui/types.rs get_outgoing_edges
 */
export function getOutgoingEdges(nodeId: number, edges: DecisionEdge[]): DecisionEdge[] {
  return edges.filter(e => e.from_node_id === nodeId);
}
