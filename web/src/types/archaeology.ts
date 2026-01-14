/**
 * Archaeology Types
 *
 * Types for exploring codebase history through narrative chunks.
 * Used by the /archaeology view for querying decision evolution with Claude.
 */

import type { DecisionNode, DecisionEdge } from './graph';
import { parseMetadata } from './graph';

// =============================================================================
// Core Archaeology Types
// =============================================================================

/**
 * A GitHub artifact link extracted from node metadata
 */
export interface GithubLink {
  type: 'commit' | 'pr' | 'issue';
  identifier: string;  // commit hash or PR/issue number as string
  repo: string;        // "owner/repo" format
  url: string;         // Full GitHub URL
  nodeId: number;      // Which node this link came from
}

/**
 * A pivot point where the approach changed
 *
 * Detected pattern: observation(s) -> revisit -> new decision/action
 */
export interface Pivot {
  /** The revisit node marking the pivot */
  revisitNode: DecisionNode;
  /** Observations that triggered reconsidering the approach */
  triggeringObservations: DecisionNode[];
  /** Nodes representing the old approach being superseded */
  supersededNodes: DecisionNode[];
  /** Nodes representing the new approach adopted */
  newApproachNodes: DecisionNode[];
}

/**
 * A narrative chunk - a coherent story arc in the decision graph
 *
 * Narratives are built from:
 * - Goals (primary story starters)
 * - Orphan revisits (pivot-initiated stories)
 */
export interface Narrative {
  /** Unique identifier (root node's change_id) */
  id: string;
  /** Display name (from metadata narrative_name or root title) */
  name: string;
  /** Root node that starts this narrative (goal or revisit) */
  root: DecisionNode;
  /** All nodes in this narrative (BFS from root) */
  nodes: DecisionNode[];
  /** Edges within this narrative */
  edges: DecisionEdge[];
  /** Pivot points detected within this narrative */
  pivots: Pivot[];
  /** Observation nodes (the "why" explanations) */
  observations: DecisionNode[];
  /** Time bounds of the narrative */
  timeRange: {
    start: Date;
    end: Date;
  };
  /** Aggregated GitHub links from all nodes */
  githubLinks: GithubLink[];
}

// =============================================================================
// Extended Metadata Helpers
// =============================================================================

/**
 * Extract github_pr number from node metadata
 */
export function getGithubPr(node: DecisionNode): number | null {
  const meta = parseMetadata(node.metadata_json);
  if (!meta) return null;
  const pr = meta.github_pr;
  if (typeof pr === 'number') return pr;
  if (typeof pr === 'string') {
    const parsed = parseInt(pr, 10);
    return isNaN(parsed) ? null : parsed;
  }
  return null;
}

/**
 * Extract github_issue number from node metadata
 */
export function getGithubIssue(node: DecisionNode): number | null {
  const meta = parseMetadata(node.metadata_json);
  if (!meta) return null;
  const issue = meta.github_issue;
  if (typeof issue === 'number') return issue;
  if (typeof issue === 'string') {
    const parsed = parseInt(issue, 10);
    return isNaN(parsed) ? null : parsed;
  }
  return null;
}

/**
 * Extract github_repo from node metadata (overrides config default)
 */
export function getGithubRepo(node: DecisionNode): string | null {
  const meta = parseMetadata(node.metadata_json);
  if (!meta) return null;
  const repo = meta.github_repo;
  return typeof repo === 'string' ? repo : null;
}

/**
 * Extract narrative_name from node metadata (for explicit naming)
 */
export function getNarrativeName(node: DecisionNode): string | null {
  const meta = parseMetadata(node.metadata_json);
  if (!meta) return null;
  const name = meta.narrative_name;
  return typeof name === 'string' ? name : null;
}

// =============================================================================
// GitHub URL Builders
// =============================================================================

const DEFAULT_REPO = 'notactuallytreyanastasio/deciduous';

/**
 * Build a GitHub commit URL
 */
export function buildCommitUrl(hash: string, repo: string = DEFAULT_REPO): string {
  return `https://github.com/${repo}/commit/${hash}`;
}

/**
 * Build a GitHub PR URL
 */
export function buildPrUrl(prNumber: number, repo: string = DEFAULT_REPO): string {
  return `https://github.com/${repo}/pull/${prNumber}`;
}

/**
 * Build a GitHub issue URL
 */
export function buildIssueUrl(issueNumber: number, repo: string = DEFAULT_REPO): string {
  return `https://github.com/${repo}/issues/${issueNumber}`;
}

// =============================================================================
// API Context Types (for /api/ask integration)
// =============================================================================

/**
 * Pivot context for API request (serializable version)
 */
export interface PivotContext {
  revisit_id: number;
  observation_ids: number[];
  superseded_ids: number[];
  new_approach_ids: number[];
}

/**
 * GitHub link context for API request (serializable version)
 */
export interface GithubLinkContext {
  type: 'commit' | 'pr' | 'issue';
  identifier: string;
  repo: string;
}

/**
 * Narrative context for API request
 * Sent to backend for Claude prompt building
 */
export interface NarrativeContext {
  name: string;
  root_id: number;
  node_ids: number[];
  pivots: PivotContext[];
  github_links: GithubLinkContext[];
}

/**
 * Extended AskContext with narrative support
 */
export interface ArchaeologyAskContext {
  selected_node_id?: number;
  visible_node_ids?: number[];
  current_branch?: string;
  narrative?: NarrativeContext;
}

// =============================================================================
// Filter Types
// =============================================================================

/**
 * Filter state for archaeology view
 */
export interface ArchaeologyFilters {
  /** Only show narratives with pivots */
  pivotsOnly: boolean;
  /** Only show narratives with GitHub links */
  hasGithubLinks: boolean;
  /** Search query for narrative names/content */
  searchQuery: string;
  /** Date range filter */
  dateRange: {
    start: Date | null;
    end: Date | null;
  };
}

/**
 * Default filter state
 */
export const DEFAULT_ARCHAEOLOGY_FILTERS: ArchaeologyFilters = {
  pivotsOnly: false,
  hasGithubLinks: false,
  searchQuery: '',
  dateRange: {
    start: null,
    end: null,
  },
};

// =============================================================================
// Chat Types
// =============================================================================

/**
 * A message in the archaeology chat
 */
export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: Date;
}

/**
 * Chat state for a narrative
 */
export interface NarrativeChatState {
  narrativeId: string;
  messages: ChatMessage[];
  isLoading: boolean;
  error: string | null;
}
