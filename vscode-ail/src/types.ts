/**
 * TypeScript interfaces mirroring the Rust ExecutorEvent and RunnerEvent enums.
 * These types are the NDJSON contract between ail and any programmatic consumer.
 * Source of truth: spec/core/s23-structured-output.md
 */

// ── Runner Events ────────────────────────────────────────────────────────────

export interface StreamDeltaEvent {
  type: "stream_delta";
  text: string;
}

export interface ThinkingEvent {
  type: "thinking";
  text: string;
}

export interface ToolUseEvent {
  type: "tool_use";
  tool_name: string;
}

export interface ToolResultEvent {
  type: "tool_result";
  tool_name: string;
}

export interface CostUpdateEvent {
  type: "cost_update";
  cost_usd: number;
  input_tokens: number;
  output_tokens: number;
}

export interface PermissionRequestedEvent {
  type: "permission_requested";
  display_name: string;
  display_detail: string;
}

export interface RunCompletedEvent {
  type: "completed";
  response: string;
  cost_usd: number | null;
  session_id: string | null;
}

export interface RunnerErrorEvent {
  type: "error";
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

// ── Executor Events ──────────────────────────────────────────────────────────

export interface RunStartedEvent {
  type: "run_started";
  run_id: string;
  pipeline_source: string | null;
  total_steps: number;
}

export interface StepStartedEvent {
  type: "step_started";
  step_id: string;
  step_index: number;
  total_steps: number;
  /** Fully-resolved prompt sent to the runner. null for non-prompt steps. */
  resolved_prompt: string | null;
}

export interface StepCompletedEvent {
  type: "step_completed";
  step_id: string;
  cost_usd: number | null;
  input_tokens: number;
  output_tokens: number;
  /** Runner response text. null for non-prompt steps. */
  response: string | null;
}

export interface StepSkippedEvent {
  type: "step_skipped";
  step_id: string;
}

export interface StepFailedEvent {
  type: "step_failed";
  step_id: string;
  error: string;
}

export interface HitlGateReachedEvent {
  type: "hitl_gate_reached";
  step_id: string;
  /** Optional operator-facing message from the step's `message:` YAML field. */
  message?: string;
}

export interface RunnerEventWrapper {
  type: "runner_event";
  event: RunnerEvent;
}

export interface PipelineCompletedEvent {
  type: "pipeline_completed";
  outcome: "completed" | "break";
  step_id?: string;
}

export interface PipelineErrorEvent {
  type: "pipeline_error";
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
