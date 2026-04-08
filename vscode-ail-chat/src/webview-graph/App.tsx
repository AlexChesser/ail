/**
 * App — root component for the pipeline graph webview.
 *
 * Receives pipeline graph data from the extension host via postMessage,
 * filters by expansion state, lays it out with dagre, and renders with React Flow.
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
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
import { filterByExpansion } from './filterByExpansion';
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

// ── App ─────────────────────────────────────────────────────────────────────

export function App(): React.ReactElement {
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);
  const [pipelineName, setPipelineName] = useState<string>('');
  const [selectedNode, setSelectedNode] = useState<StepNodeData | null>(null);
  const [errors, setErrors] = useState<string[]>([]);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());

  // Keep full graph data so we can re-filter on expand/collapse.
  const fullGraphRef = useRef<{ nodes: GraphNode[]; edges: GraphEdge[] }>({ nodes: [], edges: [] });

  const applyLayout = useCallback(
    (graphNodes: GraphNode[], graphEdges: GraphEdge[], expanded: Set<string>) => {
      const { nodes: filtered, edges: filteredEdges } = filterByExpansion(graphNodes, graphEdges, expanded);
      const { nodes: laid, edges: styledEdges } = layoutGraph(filtered, filteredEdges);
      setNodes(laid);
      setEdges(styledEdges);
    },
    [setNodes, setEdges]
  );

  const applyGraphData = useCallback(
    (graphNodes: GraphNode[], graphEdges: GraphEdge[], name: string) => {
      fullGraphRef.current = { nodes: graphNodes, edges: graphEdges };
      // Reset expansion state on new data.
      const newExpanded = new Set<string>();
      setExpandedGroups(newExpanded);
      applyLayout(graphNodes, graphEdges, newExpanded);
      setPipelineName(name);
      setSelectedNode(null);
    },
    [applyLayout]
  );

  useEffect(() => {
    const handler = (event: MessageEvent<GraphHostToWebviewMessage>) => {
      const msg = event.data;
      switch (msg.type) {
        case 'init':
        case 'update':
          try {
            applyGraphData(msg.data.nodes, msg.data.edges, msg.pipelineName);
            if (msg.data.errors.length > 0) {
              setErrors(msg.data.errors);
            } else {
              setErrors([]);
            }
          } catch (err) {
            const errMsg = err instanceof Error ? err.message : String(err);
            setErrors([`Layout error: ${errMsg}`]);
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

  const toggleGroup = useCallback(
    (groupId: string) => {
      setExpandedGroups((prev) => {
        const next = new Set(prev);
        if (next.has(groupId)) {
          next.delete(groupId);
        } else {
          next.add(groupId);
        }
        applyLayout(fullGraphRef.current.nodes, fullGraphRef.current.edges, next);
        return next;
      });
    },
    [applyLayout]
  );

  const allGroupIds = useMemo(() => {
    return new Set(
      fullGraphRef.current.nodes
        .filter((n) => n.type === 'subPipelineGroup')
        .map((n) => n.id)
    );
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  }, [nodes]);

  const expandAll = useCallback(() => {
    const all = new Set(
      fullGraphRef.current.nodes
        .filter((n) => n.type === 'subPipelineGroup')
        .map((n) => n.id)
    );
    setExpandedGroups(all);
    applyLayout(fullGraphRef.current.nodes, fullGraphRef.current.edges, all);
  }, [applyLayout]);

  const collapseAll = useCallback(() => {
    const none = new Set<string>();
    setExpandedGroups(none);
    applyLayout(fullGraphRef.current.nodes, fullGraphRef.current.edges, none);
  }, [applyLayout]);

  const onNodeClick = useCallback(
    (_event: React.MouseEvent, node: Node) => {
      const data = node.data as unknown as StepNodeData;
      setSelectedNode(data);
    },
    []
  );

  const onNodeDoubleClick = useCallback(
    (_event: React.MouseEvent, node: Node) => {
      // Double-click on group nodes toggles expansion.
      if (node.type === 'subPipelineGroup') {
        toggleGroup(node.id);
      }
    },
    [toggleGroup]
  );

  const onPaneClick = useCallback(() => {
    setSelectedNode(null);
  }, []);

  const defaultEdgeOptions = {
    type: 'smoothstep' as const,
    markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 },
  };

  const toolbarBtnStyle: React.CSSProperties = {
    background: 'var(--vscode-button-secondaryBackground)',
    color: 'var(--vscode-button-secondaryForeground)',
    border: 'none',
    borderRadius: 3,
    padding: '3px 8px',
    fontSize: 11,
    cursor: 'pointer',
    fontFamily: 'var(--vscode-font-family)',
  };

  // Memoize node types to include the toggle callback via wrapper.
  const nodeTypes: NodeTypes = useMemo(
    () => ({
      stepNode: StepNode,
      subPipelineGroup: (props: Record<string, unknown>) => {
        const nodeProps = props as Parameters<typeof SubPipelineGroupNode>[0];
        const nodeData = nodeProps.data as unknown as StepNodeData;
        const nodeId = nodeProps.id as unknown as string;
        const isExpanded = expandedGroups.has(nodeId);
        return (
          <SubPipelineGroupNode
            {...nodeProps}
            data={{ ...nodeData, _expanded: isExpanded, _onToggle: () => toggleGroup(nodeId) } as unknown as typeof nodeProps.data}
          />
        );
      },
    }),
    [expandedGroups, toggleGroup]
  );

  return (
    <div style={{ width: '100%', height: '100%', display: 'flex' }}>
      <div style={{ flex: 1, position: 'relative' }}>
        {/* Toolbar */}
        <div
          style={{
            position: 'absolute',
            top: 8,
            left: 8,
            zIndex: 10,
            display: 'flex',
            gap: 6,
            alignItems: 'center',
          }}
        >
          {pipelineName && (
            <div
              style={{
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
          {allGroupIds.size > 0 && (
            <>
              <button onClick={expandAll} style={toolbarBtnStyle} title="Expand all sub-pipelines">
                Expand All
              </button>
              <button onClick={collapseAll} style={toolbarBtnStyle} title="Collapse all sub-pipelines">
                Collapse All
              </button>
            </>
          )}
        </div>
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
          onNodeDoubleClick={onNodeDoubleClick}
          onPaneClick={onPaneClick}
          nodeTypes={nodeTypes}
          defaultEdgeOptions={defaultEdgeOptions}
          fitView
          fitViewOptions={{ padding: 0.2 }}
          minZoom={0.1}
          maxZoom={2}
          proOptions={{ hideAttribution: true }}
        >
          <Background variant={BackgroundVariant.Dots} gap={16} size={1} color="var(--vscode-panel-border)" />
          <Controls />
          <MiniMap
            nodeStrokeWidth={3}
            style={{ background: 'var(--vscode-sideBar-background)' }}
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
