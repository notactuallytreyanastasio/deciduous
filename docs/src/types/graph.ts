/**
 * Decision Graph Types
 *
 * These types mirror the Rust backend structs in src/db.rs
 * and the SQLite schema in migrations/00000000000002_decision_graph/up.sql
 */

/** Node types in the decision graph */
export type NodeType = 'goal' | 'decision' | 'option' | 'action' | 'outcome' | 'observation';

/** Node status */
export type NodeStatus = 'pending' | 'active' | 'completed' | 'rejected';

/** Edge types showing relationships between nodes */
export type EdgeType = 'leads_to' | 'requires' | 'chosen' | 'rejected' | 'blocks' | 'enables';

/** Metadata stored in metadata_json field */
export interface NodeMetadata {
  confidence?: number;  // 0-100 confidence score
  commit?: string;       // Git commit hash
  [key: string]: unknown;
}

/**
 * Decision node - represents a point in the decision graph
 * Matches: src/db.rs DecisionNode struct
 */
export interface DecisionNode {
  id: number;
  node_type: NodeType;
  title: string;
  description: string | null;
  status: NodeStatus;
  created_at: string;  // ISO 8601 timestamp
  updated_at: string;  // ISO 8601 timestamp
  metadata_json: string | null;  // JSON string containing NodeMetadata
}

/**
 * Decision edge - connects two nodes with a relationship
 * Matches: src/db.rs DecisionEdge struct
 */
export interface DecisionEdge {
  id: number;
  from_node_id: number;
  to_node_id: number;
  edge_type: EdgeType;
  weight: number | null;  // For prioritization, defaults to 1.0
  rationale: string | null;  // Why this edge exists
  created_at: string;  // ISO 8601 timestamp
}

/**
 * Full graph data structure as exported by `losselot db graph`
 */
export interface GraphData {
  nodes: DecisionNode[];
  edges: DecisionEdge[];
}

/**
 * Parsed node with metadata extracted
 */
export interface ParsedNode extends Omit<DecisionNode, 'metadata_json'> {
  metadata: NodeMetadata | null;
  confidence: number | null;
}

// ============================================================================
// Helper functions
// ============================================================================

/**
 * Parse metadata_json string into NodeMetadata object
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
 */
export function getConfidence(node: DecisionNode): number | null {
  const meta = parseMetadata(node.metadata_json);
  return meta?.confidence ?? null;
}

/**
 * Extract commit hash from a node
 */
export function getCommit(node: DecisionNode): string | null {
  const meta = parseMetadata(node.metadata_json);
  return meta?.commit ?? null;
}

/**
 * Get short commit hash (7 chars)
 */
export function shortCommit(commit: string | null): string | null {
  if (!commit) return null;
  return commit.slice(0, 7);
}

/**
 * Create GitHub commit URL
 * @param commit - Full or short commit hash
 * @param repo - Repository in format "owner/repo"
 */
export function githubCommitUrl(commit: string, repo: string): string {
  return `https://github.com/${repo}/commit/${commit}`;
}

/**
 * Get confidence level category
 */
export function getConfidenceLevel(confidence: number | null): 'high' | 'med' | 'low' | null {
  if (confidence === null) return null;
  if (confidence >= 70) return 'high';
  if (confidence >= 40) return 'med';
  return 'low';
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
  };
}
