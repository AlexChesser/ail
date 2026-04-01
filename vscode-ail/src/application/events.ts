/**
 * Typed runner events for the application layer.
 *
 * These are a simplified view of the full AilEvent union from types.ts,
 * surfacing only the events that application services need to act on.
 * The full AilEvent type remains in types.ts for use by views and panels.
 */

export type RunnerEvent =
  | { type: 'step_started'; stepId: string; stepIndex: number; totalSteps: number }
  | { type: 'stream_delta'; text: string }
  | { type: 'step_completed'; stepId: string }
  | { type: 'step_skipped'; stepId: string }
  | { type: 'step_failed'; stepId: string; error: string }
  | { type: 'hitl_gate_reached'; stepId: string }
  | { type: 'pipeline_completed' }
  | { type: 'error'; message: string };
