/**
 * filterByExpansion — given the full graph and a set of expanded group IDs,
 * returns only the nodes and edges that should be visible.
 *
 * Collapsed groups: hide child nodes, hide internal edges, keep the group
 * node itself as the edge target/source. Edges that pointed to a child
 * of a collapsed group are redirected to the group node.
 *
 * Expanded groups: the group node is hidden, child nodes are shown directly.
 */

import type { GraphNode, GraphEdge } from './types';

export function filterByExpansion(
  allNodes: GraphNode[],
  allEdges: GraphEdge[],
  expandedGroups: Set<string>
): { nodes: GraphNode[]; edges: GraphEdge[] } {
  // Build lookup: groupId → child node IDs
  const groupChildren = new Map<string, Set<string>>();
  // Build lookup: nodeId → its parentId (group)
  const nodeParent = new Map<string, string>();

  // Collect all group node IDs
  const groupNodeIds = new Set<string>();
  for (const node of allNodes) {
    if (node.type === 'subPipelineGroup') {
      groupNodeIds.add(node.id);
      if (!groupChildren.has(node.id)) {
        groupChildren.set(node.id, new Set());
      }
    }
  }

  // Map children to their groups
  for (const node of allNodes) {
    if (node.parentId && groupNodeIds.has(node.parentId)) {
      groupChildren.get(node.parentId)!.add(node.id);
      nodeParent.set(node.id, node.parentId);
    }
  }

  // Determine which nodes are hidden (children of collapsed groups)
  const hiddenNodes = new Set<string>();
  for (const [groupId, children] of groupChildren) {
    if (!expandedGroups.has(groupId)) {
      // Group is collapsed — hide all children
      for (const childId of children) {
        hiddenNodes.add(childId);
      }
    } else {
      // Group is expanded — hide the group node itself
      hiddenNodes.add(groupId);
    }
  }

  // Filter nodes
  const visibleNodes = allNodes.filter((n) => !hiddenNodes.has(n.id));

  // Remap edges: if an edge points to/from a hidden node, redirect to its group
  const visibleNodeIds = new Set(visibleNodes.map((n) => n.id));
  const remappedEdges: GraphEdge[] = [];
  const seenEdgeKeys = new Set<string>();

  for (const edge of allEdges) {
    let source = edge.source;
    let target = edge.target;

    // Redirect source if it's a hidden child → point from its group
    if (hiddenNodes.has(source)) {
      const parent = nodeParent.get(source);
      if (parent && visibleNodeIds.has(parent)) {
        source = parent;
      } else {
        continue; // Both endpoints hidden, skip
      }
    }

    // Redirect target if it's a hidden child → point to its group
    if (hiddenNodes.has(target)) {
      const parent = nodeParent.get(target);
      if (parent && visibleNodeIds.has(parent)) {
        target = parent;
      } else {
        continue;
      }
    }

    // Skip self-loops created by remapping
    if (source === target) continue;

    // Skip if both endpoints are not visible
    if (!visibleNodeIds.has(source) || !visibleNodeIds.has(target)) continue;

    // Deduplicate edges (remapping can create duplicates)
    const key = `${source}->${target}::${edge.label ?? ''}`;
    if (seenEdgeKeys.has(key)) continue;
    seenEdgeKeys.add(key);

    remappedEdges.push({
      ...edge,
      id: source === edge.source && target === edge.target
        ? edge.id
        : `${source}->${target}::${edge.label ?? ''}`,
      source,
      target,
    });
  }

  return { nodes: visibleNodes, edges: remappedEdges };
}
