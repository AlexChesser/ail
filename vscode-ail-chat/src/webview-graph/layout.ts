/**
 * layout — positions nodes using dagre (top-to-bottom DAG layout).
 */

import Dagre from '@dagrejs/dagre';
import type { Node, Edge, MarkerType } from '@xyflow/react';
import type { GraphNode, GraphEdge, StepNodeData } from './types';

const NODE_WIDTH = 200;
const NODE_HEIGHT = 60;
const GROUP_NODE_WIDTH = 220;
const GROUP_NODE_HEIGHT = 52;

export function layoutGraph(
  graphNodes: GraphNode[],
  graphEdges: GraphEdge[]
): { nodes: Node[]; edges: Edge[] } {
  const g = new Dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({
    rankdir: 'TB',
    nodesep: 50,
    ranksep: 70,
    marginx: 20,
    marginy: 20,
  });

  for (const node of graphNodes) {
    const isGroup = node.type === 'subPipelineGroup';
    g.setNode(node.id, {
      width: isGroup ? GROUP_NODE_WIDTH : NODE_WIDTH,
      height: isGroup ? GROUP_NODE_HEIGHT : NODE_HEIGHT,
    });
  }

  for (const edge of graphEdges) {
    g.setEdge(edge.source, edge.target);
  }

  let layoutSucceeded = true;
  try {
    Dagre.layout(g);
  } catch {
    layoutSucceeded = false;
  }

  const nodes: Node[] = graphNodes.map((node, i) => {
    const isGroup = node.type === 'subPipelineGroup';
    const w = isGroup ? GROUP_NODE_WIDTH : NODE_WIDTH;
    const h = isGroup ? GROUP_NODE_HEIGHT : NODE_HEIGHT;
    let x = 0;
    let y = 0;
    if (layoutSucceeded) {
      const pos = g.node(node.id);
      x = (pos?.x ?? 0) - w / 2;
      y = (pos?.y ?? 0) - h / 2;
    } else {
      // Fallback: simple vertical stack so the graph is still usable.
      x = 0;
      y = i * (NODE_HEIGHT + 40);
    }
    return {
      id: node.id,
      type: node.type,
      position: { x, y },
      data: node.data as StepNodeData & Record<string, unknown>,
    };
  });

  const edges: Edge[] = graphEdges.map((edge) => ({
    id: edge.id,
    source: edge.source,
    target: edge.target,
    label: edge.label,
    type: 'smoothstep',
    animated: edge.conditional,
    markerEnd: { type: 'arrowclosed' as unknown as MarkerType, width: 16, height: 16 },
    style: edge.conditional
      ? { stroke: 'var(--vscode-charts-orange, #f97316)', strokeWidth: 2, strokeDasharray: '6 3' }
      : { stroke: 'var(--vscode-charts-blue, #3b82f6)', strokeWidth: 2 },
    labelStyle: { fontSize: 10, fontWeight: 600, fontFamily: 'var(--vscode-font-family)', fill: 'var(--vscode-editor-foreground)' },
    labelBgStyle: edge.conditional
      ? { fill: 'var(--vscode-editor-background)', fillOpacity: 0.9 }
      : undefined,
    labelBgPadding: [4, 2] as [number, number],
    labelBgBorderRadius: 3,
  }));

  return { nodes, edges };
}
