/**
 * Shared types between the graph webview and the extension host.
 * These mirror the types in pipeline-graph/graphTransform.ts and
 * pipeline-graph/PipelineGraphPanel.ts but are defined here so the
 * webview bundle doesn't import from Node-only code.
 */

export interface StepNodeData {
  stepId: string;
  type: 'prompt' | 'context' | 'pipeline' | 'action' | 'skill' | 'invocation';
  pipelineName?: string;
  sourceFile: string;
  sourceLine: number;
  prompt?: string;
  systemPrompt?: string;
  appendSystemPromptCount?: number;
  tools?: { allow: string[]; deny: string[] };
  model?: string;
  onResultCount?: number;
  subPipelinePath?: string;
  isSubPipelineGroup?: boolean;
  branchLabel?: string;
  childStepCount?: number;
}

export interface GraphNode {
  id: string;
  type: 'stepNode' | 'subPipelineGroup';
  position: { x: number; y: number };
  data: StepNodeData;
  parentId?: string;
}

export interface GraphEdge {
  id: string;
  source: string;
  target: string;
  label?: string;
  conditional?: boolean;
}

export interface TransformResult {
  nodes: GraphNode[];
  edges: GraphEdge[];
  errors: string[];
}

/** Messages from the extension host to the graph webview. */
export type GraphHostToWebviewMessage =
  | { type: 'init'; data: TransformResult; pipelinePath: string; pipelineName: string }
  | { type: 'update'; data: TransformResult; pipelinePath: string; pipelineName: string }
  | { type: 'error'; message: string };

/** Messages from the graph webview to the extension host. */
export type GraphWebviewToHostMessage =
  | { type: 'ready' }
  | { type: 'openStepInEditor'; sourceFile: string; sourceLine: number };
