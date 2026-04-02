/**
 * AilProcess — concrete IAilClient implementation using child_process.spawn().
 *
 * This is the only file in the extension that calls spawn() for pipeline runs.
 * All child_process interaction for the main pipeline run is contained here.
 */

import { spawn, execFile, ChildProcess } from 'child_process';
import { IAilClient, InvokeOptions, ValidationResult, ValidationError, Disposable } from '../application/IAilClient';
import { RunnerEvent } from '../application/events';
import { parseNdjsonStream } from '../ndjson';
import { AilEvent, StepStartedEvent, StepCompletedEvent, StepFailedEvent } from '../types';

type EventHandler = (event: RunnerEvent) => void;
type RawEventHandler = (event: AilEvent) => void;

export class AilProcess implements IAilClient {
  private readonly _binaryPath: string;
  private readonly _cwd: string | undefined;
  private _activeProcess: ChildProcess | undefined;
  private readonly _handlers = new Set<EventHandler>();
  private readonly _rawHandlers = new Set<RawEventHandler>();

  constructor(binaryPath: string, cwd?: string) {
    this._binaryPath = binaryPath;
    this._cwd = cwd;
  }

  onEvent(handler: EventHandler): Disposable {
    this._handlers.add(handler);
    return {
      dispose: () => {
        this._handlers.delete(handler);
      },
    };
  }

  onRawEvent(handler: RawEventHandler): Disposable {
    this._rawHandlers.add(handler);
    return {
      dispose: () => {
        this._rawHandlers.delete(handler);
      },
    };
  }

  private _emit(event: RunnerEvent): void {
    for (const h of this._handlers) {
      h(event);
    }
  }

  private _emitRaw(event: AilEvent): void {
    for (const h of this._rawHandlers) {
      h(event);
    }
  }

  invoke(prompt: string, pipeline: string, options: InvokeOptions): Promise<void> {
    if (this._activeProcess) {
      return Promise.reject(new Error('An ail pipeline is already running'));
    }

    const args = [
      '--once', prompt,
      '--pipeline', pipeline,
      '--output-format', options.outputFormat ?? 'json',
    ];
    if (options.headless) {
      args.push('--headless');
    }

    return new Promise<void>((resolve, reject) => {
      const spawnEnv = options.env
        ? { ...process.env, ...options.env }
        : undefined;
      const proc = spawn(this._binaryPath, args, { cwd: this._cwd, env: spawnEnv });
      this._activeProcess = proc;

      parseNdjsonStream(
        proc.stdout!,
        (ailEvent: AilEvent) => {
          this._emitRaw(ailEvent);
          const runnerEvent = this._mapAilEvent(ailEvent);
          if (runnerEvent) {
            this._emit(runnerEvent);
          }
        },
        (err) => {
          console.error(`[ail] NDJSON stream error: ${err.message}`);
        }
      );

      // Consume stderr silently (callers can subscribe to 'error' events via onEvent)
      proc.stderr?.resume();

      proc.on('close', (code) => {
        this._activeProcess = undefined;
        if (code !== 0 && code !== null) {
          this._emit({ type: 'error', message: `ail exited with code ${code}` });
        }
        resolve();
      });

      proc.on('error', (err) => {
        this._activeProcess = undefined;
        this._emit({ type: 'error', message: `Failed to spawn ail: ${err.message}` });
        reject(err);
      });
    });
  }

  validate(pipeline: string): Promise<ValidationResult> {
    return new Promise<ValidationResult>((resolve) => {
      execFile(
        this._binaryPath,
        ['validate', '--pipeline', pipeline, '--output-format', 'json'],
        { timeout: 15000, cwd: this._cwd },
        (_err, stdout, _stderr) => {
          const raw = stdout.trim();
          if (!raw) {
            // Binary produced no output — treat as generic failure
            resolve({ valid: false, errors: [{ message: 'ail validate produced no output' }] });
            return;
          }
          let parsed: { valid: boolean; errors?: ValidationError[] };
          try {
            parsed = JSON.parse(raw) as { valid: boolean; errors?: ValidationError[] };
          } catch {
            resolve({ valid: false, errors: [{ message: raw }] });
            return;
          }
          resolve({
            valid: parsed.valid,
            errors: parsed.errors ?? [],
          });
        }
      );
    });
  }

  writeStdin(message: object): void {
    if (!this._activeProcess?.stdin) {
      return;
    }
    this._activeProcess.stdin.write(JSON.stringify(message) + '\n');
  }

  cancel(): void {
    if (!this._activeProcess) {
      return;
    }

    const proc = this._activeProcess;
    proc.kill('SIGTERM');

    // Hard kill after 5 seconds if still running
    const timeout = setTimeout(() => {
      if (this._activeProcess === proc) {
        proc.kill('SIGKILL');
      }
    }, 5000);

    proc.once('close', () => {
      clearTimeout(timeout);
    });
  }

  /** Map an AilEvent to a RunnerEvent understood by the application layer. */
  private _mapAilEvent(event: AilEvent): RunnerEvent | undefined {
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
        return undefined;
      case 'pipeline_completed':
        return { type: 'pipeline_completed' };
      case 'pipeline_error':
        return { type: 'error', message: event.error };
      default:
        return undefined;
    }
  }
}
