/**
 * Focus Mode Hook
 *
 * Manages focus mode state for isolating a subgraph.
 * When in focus mode, only selected nodes and their connections are visible.
 */

import { useState, useCallback, useMemo } from 'react';
import type { DecisionNode, DecisionEdge } from '../types/graph';

export interface UseFocusModeResult {
  isFocused: boolean;
  focusedNodeIds: Set<number>;
  enterFocus: (nodeIds: number[], includeConnections?: boolean) => void;
  exitFocus: () => void;
  toggleNodeFocus: (nodeId: number) => void;
  focusedNodes: DecisionNode[];
  focusedEdges: DecisionEdge[];
}

/**
 * Hook for managing focus mode
 *
 * @param allNodes - All nodes in the graph
 * @param allEdges - All edges in the graph
 */
export function useFocusMode(
  allNodes: DecisionNode[],
  allEdges: DecisionEdge[]
): UseFocusModeResult {
  const [focusedNodeIds, setFocusedNodeIds] = useState<Set<number>>(new Set());

  const isFocused = focusedNodeIds.size > 0;

  /**
   * Enter focus mode with the given node IDs
   * @param nodeIds - Initial nodes to focus on
   * @param includeConnections - If true, also include directly connected nodes
   */
  const enterFocus = useCallback(
    (nodeIds: number[], includeConnections: boolean = true) => {
      const focused = new Set<number>(nodeIds);

      if (includeConnections) {
        // Add nodes connected by edges
        allEdges.forEach((edge) => {
          if (focused.has(edge.from_node_id)) {
            focused.add(edge.to_node_id);
          }
          if (focused.has(edge.to_node_id)) {
            focused.add(edge.from_node_id);
          }
        });
      }

      setFocusedNodeIds(focused);
    },
    [allEdges]
  );

  /**
   * Exit focus mode and show all nodes
   */
  const exitFocus = useCallback(() => {
    setFocusedNodeIds(new Set());
  }, []);

  /**
   * Toggle a single node's inclusion in focus mode
   */
  const toggleNodeFocus = useCallback((nodeId: number) => {
    setFocusedNodeIds((prev) => {
      const next = new Set(prev);
      if (next.has(nodeId)) {
        next.delete(nodeId);
      } else {
        next.add(nodeId);
      }
      return next;
    });
  }, []);

  /**
   * Get the subset of nodes that are focused
   */
  const focusedNodes = useMemo(() => {
    if (!isFocused) return allNodes;
    return allNodes.filter((node) => focusedNodeIds.has(node.id));
  }, [allNodes, focusedNodeIds, isFocused]);

  /**
   * Get edges between focused nodes only
   */
  const focusedEdges = useMemo(() => {
    if (!isFocused) return allEdges;
    return allEdges.filter(
      (edge) => focusedNodeIds.has(edge.from_node_id) && focusedNodeIds.has(edge.to_node_id)
    );
  }, [allEdges, focusedNodeIds, isFocused]);

  return {
    isFocused,
    focusedNodeIds,
    enterFocus,
    exitFocus,
    toggleNodeFocus,
    focusedNodes,
    focusedEdges,
  };
}

export default useFocusMode;
