/**
 * layout — positions nodes using dagre (top-to-bottom DAG layout).
 */

import Dagre from '@dagrejs/dagre';
import type { Node, Edge } from '@xyflow/react';
import type { GraphNode, GraphEdge, StepNodeData } from './types';

const NODE_WIDTH = 200;
const NODE_HEIGHT = 60;

export function layoutGraph(
  graphNodes: GraphNode[],
  graphEdges: GraphEdge[]
): { nodes: Node[]; edges: Edge[] } {
  const g = new Dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({
    rankdir: 'TB',
    nodesep: 40,
    ranksep: 80,
    marginx: 20,
    marginy: 20,
  });

  for (const node of graphNodes) {
    g.setNode(node.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
  }

  for (const edge of graphEdges) {
    g.setEdge(edge.source, edge.target);
  }

  Dagre.layout(g);

  const nodes: Node[] = graphNodes.map((node) => {
    const pos = g.node(node.id);
    return {
      id: node.id,
      type: 'stepNode',
      position: {
        x: (pos?.x ?? 0) - NODE_WIDTH / 2,
        y: (pos?.y ?? 0) - NODE_HEIGHT / 2,
      },
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
    style: edge.conditional
      ? { stroke: 'var(--vscode-charts-orange)', strokeDasharray: '5 3' }
      : { stroke: 'var(--vscode-charts-blue)' },
    labelStyle: edge.conditional
      ? { fill: 'var(--vscode-charts-orange)', fontSize: 10, fontWeight: 500 }
      : undefined,
  }));

  return { nodes, edges };
}
