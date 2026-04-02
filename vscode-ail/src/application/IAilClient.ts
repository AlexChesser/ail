/**
 * IAilClient — interface abstracting the spawned ail process.
 *
 * All interaction with the ail binary goes through this interface.
 * AilProcess implements it; tests use mock implementations.
 */

import { RunnerEvent } from './events';
import { AilEvent } from '../types';

export interface Disposable {
  dispose(): void;
}

export interface InvokeOptions {
  headless?: boolean;
  outputFormat?: 'text' | 'json';
  /** Extra environment variables to pass to the ail child process. */
  env?: Record<string, string>;
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
}

export interface IAilClient {
  /**
   * Start a `--once` run with the given prompt and pipeline path.
   * Events are delivered via `onEvent()` handlers registered before this call.
   * Resolves when the process exits (regardless of exit code).
   */
  invoke(prompt: string, pipeline: string, options: InvokeOptions): Promise<void>;

  /**
   * Run `ail validate --pipeline <pipeline>` and return the result.
   */
  validate(pipeline: string): Promise<ValidationResult>;

  /**
   * Kill the active child process (SIGTERM → SIGKILL after 5 s).
   * No-op if no process is running.
   */
  cancel(): void;

  /**
   * Register a handler for runner events emitted during `invoke()`.
   * Returns a Disposable that unregisters the handler.
   */
  onEvent(handler: (event: RunnerEvent) => void): Disposable;

  /**
   * Register a handler for the full-fidelity AilEvent stream during `invoke()`.
   * Delivers every raw event from the ail binary before any mapping occurs.
   * Use this for consumers (e.g. ExecutionPanel) that need events not present
   * in the simplified RunnerEvent union (thinking, tool_use, cost_update, etc.).
   */
  onRawEvent(handler: (event: AilEvent) => void): Disposable;
}
