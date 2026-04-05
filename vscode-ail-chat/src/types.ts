/**
 * Shared types for vscode-ail-chat.
 *
 * Two sections:
 *   1. AilEvent — the NDJSON wire contract emitted by `ail --output-format json`.
 *      Copied from vscode-ail/src/types.ts; both must track the same protocol.
 *      Source of truth: spec/core/s23-structured-output.md
 *
 *   2. WebviewMessage — the postMessage protocol between the extension host
 *      and the React webview panel. Typed as a discriminated union on `type`.
 */

// ── AilEvent (NDJSON wire contract) ──────────────────────────────────────────

export interface StreamDeltaEvent {
  type: 'stream_delta';
  text: string;
}

export interface ThinkingEvent {
  type: 'thinking';
  text: string;
}

export interface ToolUseEvent {
  type: 'tool_use';
  tool_name: string;
  tool_use_id?: string;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  input?: any;
}

export interface ToolResultEvent {
  type: 'tool_result';
  tool_name: string;
  tool_use_id?: string;
  content?: string;
  is_error?: boolean;
}

export interface CostUpdateEvent {
  type: 'cost_update';
  cost_usd: number;
  input_tokens: number;
  output_tokens: number;
}

export interface PermissionRequestedEvent {
  type: 'permission_requested';
  display_name: string;
  display_detail: string;
  tool_input?: unknown;
}

export interface RunCompletedEvent {
  type: 'completed';
  response: string;
  cost_usd: number | null;
  session_id: string | null;
}

export interface RunnerErrorEvent {
  type: 'error';
  message: string;
}

export type RunnerEvent =
  | StreamDeltaEvent
  | ThinkingEvent
  | ToolUseEvent
  | ToolResultEvent
  | CostUpdateEvent
  | PermissionRequestedEvent
  | RunCompletedEvent
  | RunnerErrorEvent;

export interface RunStartedEvent {
  type: 'run_started';
  run_id: string;
  pipeline_source: string | null;
  total_steps: number;
}

export interface StepStartedEvent {
  type: 'step_started';
  step_id: string;
  step_index: number;
  total_steps: number;
  resolved_prompt: string | null;
}

export interface StepCompletedEvent {
  type: 'step_completed';
  step_id: string;
  cost_usd: number | null;
  input_tokens: number;
  output_tokens: number;
  response: string | null;
  model?: string;
}

export interface StepSkippedEvent {
  type: 'step_skipped';
  step_id: string;
}

export interface StepFailedEvent {
  type: 'step_failed';
  step_id: string;
  error: string;
}

export interface HitlGateReachedEvent {
  type: 'hitl_gate_reached';
  step_id: string;
  message?: string;
}

export interface RunnerEventWrapper {
  type: 'runner_event';
  event: RunnerEvent;
}

export interface PipelineCompletedEvent {
  type: 'pipeline_completed';
  outcome: 'completed' | 'break';
  step_id?: string;
}

export interface PipelineErrorEvent {
  type: 'pipeline_error';
  error: string;
  error_type: string;
}

export type AilEvent =
  | RunStartedEvent
  | StepStartedEvent
  | StepCompletedEvent
  | StepSkippedEvent
  | StepFailedEvent
  | HitlGateReachedEvent
  | RunnerEventWrapper
  | PipelineCompletedEvent
  | PipelineErrorEvent;

// ── WebviewMessage (postMessage protocol) ────────────────────────────────────

/** A persisted session summary sent in the sessionsUpdated message. */
export interface SessionSummary {
  id: string;
  title: string;
  timestamp: number;
  totalCostUsd: number;
}

/**
 * Messages sent FROM the extension host TO the webview.
 */
export type HostToWebviewMessage =
  | { type: 'runStarted'; runId: string; totalSteps: number }
  | { type: 'stepStarted'; stepId: string; stepIndex: number; totalSteps: number }
  | { type: 'streamDelta'; text: string }
  | { type: 'thinking'; text: string }
  | { type: 'toolUse'; toolName: string; toolUseId: string; input: unknown }
  | { type: 'toolResult'; toolUseId: string; content: string; isError: boolean }
  | { type: 'stepCompleted'; stepId: string; costUsd: number | null; inputTokens: number; outputTokens: number; response?: string | null }
  | { type: 'stepSkipped'; stepId: string }
  | { type: 'stepFailed'; stepId: string; error: string }
  | { type: 'hitlGate'; stepId: string; message?: string }
  | { type: 'permissionRequested'; displayName: string; displayDetail: string; toolInput?: unknown }
  | { type: 'pipelineCompleted' }
  | { type: 'pipelineError'; error: string }
  | { type: 'processError'; message: string }
  | { type: 'sessionsUpdated'; sessions: SessionSummary[] }
  | { type: 'pipelineChanged'; path: string | null; displayName: string | null };

/**
 * Messages sent FROM the webview TO the extension host.
 */
export type WebviewToHostMessage =
  | { type: 'ready' }
  | { type: 'submitPrompt'; text: string }
  | { type: 'hitlResponse'; stepId: string; text: string }
  | { type: 'permissionResponse'; allowed: boolean; reason?: string }
  | { type: 'killProcess' }
  | { type: 'switchSession'; sessionId: string }
  | { type: 'newSession' }
  | { type: 'loadPipeline' };
