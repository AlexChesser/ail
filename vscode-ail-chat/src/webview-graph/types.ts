/**
 * Shared types between the graph webview and the extension host.
 * These mirror the types in pipeline-graph/graphTransform.ts and
 * pipeline-graph/PipelineGraphPanel.ts but are defined here so the
 * webview bundle doesn't import from Node-only code.
 */

/** Describes one on_result branch for display in the detail panel. */
export interface OnResultBranch {
  matcher: string;
  action: string;
  prompt?: string;
}

/** Describes one append_system_prompt entry. */
export interface AppendSystemPromptEntry {
  type: 'text' | 'file' | 'shell';
  value: string;
}

export interface StepNodeData {
  stepId: string;
  type: 'prompt' | 'context' | 'pipeline' | 'action' | 'skill' | 'invocation';
  pipelineName?: string;
  sourceFile: string;
  sourceLine: number;
  prompt?: string;
  /** Whether the prompt value is a file path (starts with ./, ../, ~/, or /). */
  promptIsFile?: boolean;
  systemPrompt?: string;
  /** Whether system_prompt is a file path. */
  systemPromptIsFile?: boolean;
  appendSystemPromptCount?: number;
  /** Detailed append_system_prompt entries for display. */
  appendSystemPromptEntries?: AppendSystemPromptEntry[];
  tools?: { allow: string[]; deny: string[] };
  model?: string;
  onResultCount?: number;
  /** Detailed on_result branches for display. */
  onResultBranches?: OnResultBranch[];
  subPipelinePath?: string;
  isSubPipelineGroup?: boolean;
  branchLabel?: string;
  childStepCount?: number;
  /** For context steps: the shell command. */
  shellCommand?: string;
  /** For action steps: the action kind. */
  actionKind?: string;
  /** Step condition (always/never). */
  condition?: string;
  /** Whether this step resumes a previous session. */
  resume?: boolean;
  /** HITL gate message. */
  message?: string;
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
