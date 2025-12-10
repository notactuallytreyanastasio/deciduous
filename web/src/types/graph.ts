/**
 * Decision Graph Types
 *
 * These types MUST match the Rust backend structs in src/db.rs
 * and the SQLite schema in migrations/00000000000002_decision_graph/up.sql
 *
 * Rust DecisionNode (src/db.rs:186-198):
 *   pub struct DecisionNode {
 *       pub id: i32,
 *       pub node_type: String,
 *       pub title: String,
 *       pub description: Option<String>,
 *       pub status: String,
 *       pub created_at: String,
 *       pub updated_at: String,
 *       pub metadata_json: Option<String>,
 *   }
 *
 * Rust DecisionEdge (src/db.rs:200-210):
 *   pub struct DecisionEdge {
 *       pub id: i32,
 *       pub from_node_id: i32,
 *       pub to_node_id: i32,
 *       pub edge_type: String,
 *       pub weight: Option<f64>,
 *       pub rationale: Option<String>,
 *       pub created_at: String,
 *   }
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

/**
 * Decision node - represents a point in the decision graph
 * Matches: src/db.rs DecisionNode struct (lines 186-198)
 */
export interface DecisionNode {
  id: number;
  node_type: NodeType;
  title: string;
  description: string | null;
  status: NodeStatus;
  created_at: string;  // ISO 8601 timestamp from SQLite TEXT
  updated_at: string;  // ISO 8601 timestamp from SQLite TEXT
  metadata_json: string | null;  // JSON string containing NodeMetadata
}

/**
 * Decision edge - connects two nodes with a relationship
 * Matches: src/db.rs DecisionEdge struct (lines 200-210)
 */
export interface DecisionEdge {
  id: number;
  from_node_id: number;
  to_node_id: number;
  edge_type: EdgeType;
  weight: number | null;  // SQLite REAL, defaults to 1.0
  rationale: string | null;
  created_at: string;  // ISO 8601 timestamp
}

/**
 * Full graph data structure as exported by `losselot db graph`
 * This is the JSON format written to graph-data.json
 */
export interface GraphData {
  nodes: DecisionNode[];
  edges: DecisionEdge[];
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
 * Create GitHub commit URL
 */
export function githubCommitUrl(commit: string, repo: string = 'notactuallytreyanastasio/losselot'): string {
  return `https://github.com/${repo}/commit/${commit}`;
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
    node_type: node.node_type,
    title: node.title,
    description: node.description,
    status: node.status,
    created_at: node.created_at,
    updated_at: node.updated_at,
    metadata,
    confidence: metadata?.confidence ?? null,
    commit: metadata?.commit ?? null,
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
