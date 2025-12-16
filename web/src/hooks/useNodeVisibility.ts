/**
 * Node Visibility Hook
 *
 * Tracks which nodes are visible in the current viewport at the current zoom level.
 * Categorizes nodes as: visible, too-small (visible but unreadable), or off-screen.
 */

import { useState, useEffect, useCallback, useRef } from 'react';

export type NodeVisibility = 'visible' | 'too-small' | 'off-screen';

export interface VisibilityInfo {
  visibility: NodeVisibility;
  screenX: number;
  screenY: number;
  screenWidth: number;
  screenHeight: number;
}

export interface UseNodeVisibilityResult {
  visibilityMap: Map<number, VisibilityInfo>;
  getVisibility: (nodeId: number) => NodeVisibility;
  updateVisibility: () => void;
  visibleCount: number;
  tooSmallCount: number;
  offScreenCount: number;
}

// Minimum size in pixels for a node to be considered "readable"
const MIN_READABLE_SIZE = 20;

/**
 * Hook to track node visibility based on viewport and zoom
 *
 * @param svgRef - Reference to the SVG element
 * @param nodePositions - Map of node ID to position { x, y, width, height } in graph coordinates
 * @param zoom - Current zoom level
 * @param transform - Current pan transform { x, y }
 */
export function useNodeVisibility(
  svgRef: React.RefObject<SVGSVGElement | null>,
  nodePositions: Map<number, { x: number; y: number; width: number; height: number }>,
  zoom: number,
  transform: { x: number; y: number }
): UseNodeVisibilityResult {
  const [visibilityMap, setVisibilityMap] = useState<Map<number, VisibilityInfo>>(new Map());
  const updateScheduled = useRef(false);

  const updateVisibility = useCallback(() => {
    if (!svgRef.current) return;

    const svg = svgRef.current;
    const rect = svg.getBoundingClientRect();
    const viewportWidth = rect.width;
    const viewportHeight = rect.height;

    const newMap = new Map<number, VisibilityInfo>();

    nodePositions.forEach((pos, nodeId) => {
      // Transform graph coordinates to screen coordinates
      // Note: pos.x and pos.y are CENTER coordinates from dagre
      const screenCenterX = pos.x * zoom + transform.x;
      const screenCenterY = pos.y * zoom + transform.y;
      const screenWidth = pos.width * zoom;
      const screenHeight = pos.height * zoom;

      // Calculate top-left corner for bounds checking
      const screenX = screenCenterX - screenWidth / 2;
      const screenY = screenCenterY - screenHeight / 2;

      // Check if node is in viewport (with some padding)
      const padding = 50;
      const isInViewport =
        screenX + screenWidth >= -padding &&
        screenX <= viewportWidth + padding &&
        screenY + screenHeight >= -padding &&
        screenY <= viewportHeight + padding;

      // Check if node is large enough to read
      const isReadable = screenWidth >= MIN_READABLE_SIZE && screenHeight >= MIN_READABLE_SIZE;

      let visibility: NodeVisibility;
      if (!isInViewport) {
        visibility = 'off-screen';
      } else if (!isReadable) {
        visibility = 'too-small';
      } else {
        visibility = 'visible';
      }

      newMap.set(nodeId, {
        visibility,
        screenX,
        screenY,
        screenWidth,
        screenHeight,
      });
    });

    setVisibilityMap(newMap);
  }, [svgRef, nodePositions, zoom, transform]);

  // Debounced update on zoom/pan changes
  useEffect(() => {
    if (updateScheduled.current) return;
    updateScheduled.current = true;

    const timeoutId = setTimeout(() => {
      updateVisibility();
      updateScheduled.current = false;
    }, 50);

    return () => {
      clearTimeout(timeoutId);
      updateScheduled.current = false;
    };
  }, [updateVisibility]);

  const getVisibility = useCallback(
    (nodeId: number): NodeVisibility => {
      return visibilityMap.get(nodeId)?.visibility ?? 'off-screen';
    },
    [visibilityMap]
  );

  // Calculate counts
  let visibleCount = 0;
  let tooSmallCount = 0;
  let offScreenCount = 0;

  visibilityMap.forEach((info) => {
    switch (info.visibility) {
      case 'visible':
        visibleCount++;
        break;
      case 'too-small':
        tooSmallCount++;
        break;
      case 'off-screen':
        offScreenCount++;
        break;
    }
  });

  return {
    visibilityMap,
    getVisibility,
    updateVisibility,
    visibleCount,
    tooSmallCount,
    offScreenCount,
  };
}

export default useNodeVisibility;
