/**
 * App — root component for the pipeline graph webview.
 *
 * Receives pipeline graph data from the extension host via postMessage,
 * lays it out with dagre, and renders it with React Flow.
 */

import React, { useCallback, useEffect, useState } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  BackgroundVariant,
  MarkerType,
  type Node,
  type Edge,
  type NodeTypes,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import { StepNode } from './components/StepNode';
import { SubPipelineGroupNode } from './components/SubPipelineGroupNode';
import { DetailPanel } from './components/DetailPanel';
import { layoutGraph } from './layout';
import type {
  GraphHostToWebviewMessage,
  GraphWebviewToHostMessage,
  StepNodeData,
  GraphNode,
  GraphEdge,
} from './types';

// ── VS Code API ────────────────────────────────────────────────────────────────

declare function acquireVsCodeApi(): {
  postMessage: (msg: GraphWebviewToHostMessage) => void;
  getState: () => unknown;
  setState: (state: unknown) => void;
};

const vscode = typeof acquireVsCodeApi !== 'undefined' ? acquireVsCodeApi() : null;

function postToHost(msg: GraphWebviewToHostMessage): void {
  vscode?.postMessage(msg);
}

// ── Custom node types ───────────────────────────────────────────────────────

const nodeTypes: NodeTypes = {
  stepNode: StepNode,
  subPipelineGroup: SubPipelineGroupNode,
};

// ── App ─────────────────────────────────────────────────────────────────────

export function App(): React.ReactElement {
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);
  const [pipelineName, setPipelineName] = useState<string>('');
  const [selectedNode, setSelectedNode] = useState<StepNodeData | null>(null);
  const [errors, setErrors] = useState<string[]>([]);

  const applyGraphData = useCallback(
    (graphNodes: GraphNode[], graphEdges: GraphEdge[], name: string) => {
      const { nodes: laid, edges: styledEdges } = layoutGraph(graphNodes, graphEdges);
      setNodes(laid);
      setEdges(styledEdges);
      setPipelineName(name);
      setSelectedNode(null);
    },
    [setNodes, setEdges]
  );

  useEffect(() => {
    const handler = (event: MessageEvent<GraphHostToWebviewMessage>) => {
      const msg = event.data;
      switch (msg.type) {
        case 'init':
        case 'update':
          applyGraphData(msg.data.nodes, msg.data.edges, msg.pipelineName);
          if (msg.data.errors.length > 0) {
            setErrors(msg.data.errors);
          } else {
            setErrors([]);
          }
          break;
        case 'error':
          setErrors((prev) => [...prev, msg.message]);
          break;
      }
    };

    window.addEventListener('message', handler);
    postToHost({ type: 'ready' });

    return () => window.removeEventListener('message', handler);
  }, [applyGraphData]);

  const onNodeClick = useCallback(
    (_event: React.MouseEvent, node: Node) => {
      const data = node.data as unknown as StepNodeData;
      setSelectedNode(data);
    },
    []
  );

  const onPaneClick = useCallback(() => {
    setSelectedNode(null);
  }, []);

  const defaultEdgeOptions = {
    type: 'smoothstep' as const,
    markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 },
  };

  return (
    <div style={{ width: '100%', height: '100%', display: 'flex' }}>
      <div style={{ flex: 1, position: 'relative' }}>
        {pipelineName && (
          <div
            style={{
              position: 'absolute',
              top: 8,
              left: 8,
              zIndex: 10,
              padding: '4px 10px',
              background: 'var(--vscode-badge-background)',
              color: 'var(--vscode-badge-foreground)',
              borderRadius: 4,
              fontSize: 12,
              fontWeight: 600,
            }}
          >
            {pipelineName}
          </div>
        )}
        {errors.length > 0 && (
          <div
            style={{
              position: 'absolute',
              top: 8,
              right: selectedNode ? 308 : 8,
              zIndex: 10,
              padding: '6px 10px',
              background: 'var(--vscode-inputValidation-errorBackground)',
              border: '1px solid var(--vscode-inputValidation-errorBorder)',
              color: 'var(--vscode-errorForeground)',
              borderRadius: 4,
              fontSize: 11,
              maxWidth: 300,
            }}
          >
            {errors.map((e, i) => (
              <div key={i}>{e}</div>
            ))}
          </div>
        )}
        <ReactFlow
          nodes={nodes}
          edges={edges}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onNodeClick={onNodeClick}
          onPaneClick={onPaneClick}
          nodeTypes={nodeTypes}
          defaultEdgeOptions={defaultEdgeOptions}
          fitView
          fitViewOptions={{ padding: 0.2 }}
          minZoom={0.1}
          maxZoom={2}
          proOptions={{ hideAttribution: true }}
        >
          <Background variant={BackgroundVariant.Dots} gap={16} size={1} />
          <Controls />
          <MiniMap
            nodeStrokeWidth={3}
            style={{ background: 'var(--vscode-sideBar-background)' }}
            maskColor="rgba(0, 0, 0, 0.2)"
          />
        </ReactFlow>
      </div>
      {selectedNode && (
        <DetailPanel
          data={selectedNode}
          onClose={() => setSelectedNode(null)}
          onOpenInEditor={(sourceFile, sourceLine) =>
            postToHost({ type: 'openStepInEditor', sourceFile, sourceLine })
          }
        />
      )}
    </div>
  );
}
