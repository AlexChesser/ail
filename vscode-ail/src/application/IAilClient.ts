/**
 * IAilClient — interface abstracting the spawned ail process.
 *
 * All interaction with the ail binary goes through this interface.
 * AilProcess implements it; tests use mock implementations.
 */

import { RunnerEvent } from './events';

export interface Disposable {
  dispose(): void;
}

export interface InvokeOptions {
  headless?: boolean;
  outputFormat?: 'text' | 'json';
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
}
