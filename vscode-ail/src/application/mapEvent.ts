/**
 * mapAilEvent — pure function: AilEvent → RunnerEvent | undefined.
 *
 * Extracted from AilProcess._mapAilEvent() so it can be unit-tested
 * without spawning a child process or pulling in the vscode API.
 */

import {
  AilEvent,
  StepStartedEvent,
  StepCompletedEvent,
  StepFailedEvent,
} from '../types';
import { RunnerEvent } from './events';

export function mapAilEvent(event: AilEvent): RunnerEvent | undefined {
  switch (event.type) {
    case 'step_started': {
      const e = event as StepStartedEvent;
      return {
        type: 'step_started',
        stepId: e.step_id,
        stepIndex: e.step_index,
        totalSteps: e.total_steps,
      };
    }
    case 'step_completed': {
      const e = event as StepCompletedEvent;
      return { type: 'step_completed', stepId: e.step_id };
    }
    case 'step_skipped':
      return { type: 'step_skipped', stepId: event.step_id };
    case 'step_failed': {
      const e = event as StepFailedEvent;
      return { type: 'step_failed', stepId: e.step_id, error: e.error };
    }
    case 'hitl_gate_reached':
      return { type: 'hitl_gate_reached', stepId: event.step_id };
    case 'runner_event':
      if (event.event.type === 'stream_delta') {
        return { type: 'stream_delta', text: event.event.text };
      }
      if (event.event.type === 'cost_update') {
        return {
          type: 'cost_update',
          costUsd: event.event.cost_usd,
          inputTokens: event.event.input_tokens,
          outputTokens: event.event.output_tokens,
        };
      }
      return undefined;
    case 'pipeline_completed':
      return { type: 'pipeline_completed' };
    case 'pipeline_error':
      return { type: 'error', message: event.error };
    default:
      return undefined;
  }
}
