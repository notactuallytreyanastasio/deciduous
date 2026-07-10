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

export const NODE_TYPES = ['goal', 'decision', 'option', 'action', 'outcome', 'observation', 'revisit'] as const;
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
  github_pr?: number | string;    // GitHub PR number
  github_issue?: number | string; // GitHub issue number
  github_repo?: string;           // GitHub repo in "owner/repo" format
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
 * Theme definition - matches Rust Theme struct in db.rs
 */
export interface Theme {
  id: number;
  change_id: string;
  name: string;
  color: string;
  description: string | null;
  created_at: string;
  updated_at: string;
}

/**
 * Node-theme association - matches Rust NodeTheme struct in db.rs
 */
export interface NodeThemeAssociation {
  node_id: number;
  theme_id: number;
  source: string;  // 'manual' | 'suggested'
  created_at: string;
}

/**
 * Document attached to a node - matches Rust NodeDocument struct in db.rs
 */
export interface NodeDocument {
  id: number;
  change_id: string;
  node_id: number;
  node_change_id: string;
  content_hash: string;
  original_filename: string;
  storage_filename: string;
  mime_type: string;
  file_size: number;
  description: string | null;
  description_source: string;  // 'none' | 'ai' | 'manual'
  attached_at: string;
  attached_by: string | null;
  detached_at: string | null;
}

/**
 * Full graph data structure as exported by `deciduous sync`
 * This is the JSON format written to graph-data.json
 */
export interface GraphData {
  nodes: DecisionNode[];
  edges: DecisionEdge[];
  config?: DeciduousConfig;  // Optional config for external repo links
  themes?: Theme[];
  node_themes?: NodeThemeAssociation[];
  documents?: NodeDocument[];
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
 * Default repository for commit links (when no config is provided)
 */
export const DEFAULT_COMMIT_REPO = 'notactuallytreyanastasio/deciduous';

/**
 * Get themes for a specific node
 */
export function getNodeThemes(nodeId: number, graphData: GraphData): Theme[] {
  if (!graphData.themes || !graphData.node_themes) return [];
  const themeIds = graphData.node_themes
    .filter(nt => nt.node_id === nodeId)
    .map(nt => nt.theme_id);
  return graphData.themes.filter(t => themeIds.includes(t.id));
}

/**
 * Get theme association source for a node-theme pair
 */
export function getNodeThemeSource(nodeId: number, themeId: number, graphData: GraphData): string {
  const assoc = graphData.node_themes?.find(nt => nt.node_id === nodeId && nt.theme_id === themeId);
  return assoc?.source || 'manual';
}

/**
 * Get documents for a specific node (excludes detached)
 */
export function getNodeDocuments(nodeId: number, graphData: GraphData): NodeDocument[] {
  if (!graphData.documents) return [];
  return graphData.documents.filter(d => d.node_id === nodeId && !d.detached_at);
}

/**
 * Format file size for display
 */
export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
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

// =============================================================================
// Structural Grouping Types - For browsing by goal tree / connected subgraph
// =============================================================================

/**
 * A structural group represents a connected subgraph, typically rooted at a goal
 */
export interface StructuralGroup {
  id: string;                       // Unique identifier for this group
  rootNode: DecisionNode;           // The root node (usually a goal)
  nodes: DecisionNode[];            // All nodes in this subgraph
  edges: DecisionEdge[];            // All edges within this subgraph
  isAutoDetected: boolean;          // true for goal-rooted, false for manual selection
  label?: string;                   // Optional display label
}

/**
 * Tree node for hierarchical rendering
 * Used by HierarchicalTree component
 */
export interface TreeNode {
  node: DecisionNode;
  children: TreeNode[];
  edgeFromParent?: DecisionEdge;    // The edge connecting to parent
  depth: number;
  isExpanded: boolean;
  isRevisit: boolean;               // Special styling for REVISIT nodes
  duplicateIndex?: number;          // When node appears multiple times (multi-parent)
}

// =============================================================================
// Branch Grouping Types - For browsing by git branch
// =============================================================================

/**
 * A branch group contains all nodes created on a specific git branch
 */
export interface BranchGroup {
  branch: string;                   // Branch name
  nodes: DecisionNode[];            // Nodes on this branch
  edges: DecisionEdge[];            // Edges between nodes in this branch
  crossBranchEdges: DecisionEdge[]; // Edges to/from nodes on other branches
  commits: GitCommit[];             // Git commits linked to nodes on this branch
  dateRange: {
    start: Date;
    end: Date;
  };
  nodeCount: number;
}

// =============================================================================
// Narrative/Pivot Types - For browsing evolution over time
// =============================================================================

/**
 * A pivot event captures a REVISIT node and its context:
 * what was superseded, why, and what replaced it
 */
export interface PivotEvent {
  revisitNode: DecisionNode;        // The REVISIT node itself
  supersededNodes: DecisionNode[];  // Nodes that were abandoned/changed
  reasonNodes: DecisionNode[];      // Observations that triggered the revisit
  replacementNodes: DecisionNode[]; // New approach that followed
  timestamp: Date;
}

/**
 * Narrative timeline item - can be a regular node event or a pivot
 */
export interface NarrativeItem {
  type: 'goal' | 'decision' | 'action' | 'outcome' | 'observation' | 'pivot';
  timestamp: Date;
  node: DecisionNode;
  pivotEvent?: PivotEvent;          // Present when type === 'pivot'
  relatedNodes?: DecisionNode[];    // Options for decisions, linked nodes for outcomes
  linkedCommit?: GitCommit;         // Git commit if present
}

/**
 * A chapter/phase in the narrative - groups related items
 */
export interface NarrativeChapter {
  id: string;                       // Unique chapter ID
  title: string;                    // Chapter title (e.g., "Auth Design", "Auth Pivot")
  summary: string;                  // Brief summary of what happened in this chapter
  items: NarrativeItem[];           // Items in this chapter
  startTime: Date;
  endTime: Date;
  pivot?: PivotEvent;               // If this chapter ends with/contains a pivot
  outcome?: DecisionNode;           // Key outcome of this chapter if any
  decisions: DecisionNode[];        // Decisions made in this chapter
}

/**
 * Generated narrative summary
 */
export interface NarrativeSummary {
  headline: string;                 // One-line summary
  goalsAchieved: string[];          // What was accomplished
  keyPivots: string[];              // Major direction changes
  openQuestions: string[];          // Unresolved decisions/observations
}

/**
 * Complete narrative for a structural group
 */
export interface Narrative {
  group: StructuralGroup;
  items: NarrativeItem[];           // Chronological timeline
  chapters: NarrativeChapter[];     // Grouped into chapters
  pivots: PivotEvent[];             // All pivot points
  summary: NarrativeSummary;        // Generated summary
  totalDuration: string;            // Human-readable duration
}
