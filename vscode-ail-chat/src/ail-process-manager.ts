/**
 * AilProcessManager — spawns `ail --once` and translates NDJSON events
 * into HostToWebviewMessage objects for the React panel.
 *
 * Key invariants:
 *   - CLAUDECODE env var is removed before spawn to avoid the nested-session guard.
 *   - Only one ail process may be active at a time; start() rejects if one is running.
 *   - cancel() sends SIGTERM, then SIGKILL after 5 seconds.
 */

import { spawn, ChildProcess } from 'child_process';
import { AilEvent, HostToWebviewMessage } from './types';
import { parseNdjsonStream } from './ndjson';

export interface StartOptions {
  headless?: boolean;
}

type MessageHandler = (msg: HostToWebviewMessage) => void;

export class AilProcessManager {
  private readonly _binaryPath: string;
  private readonly _cwd: string | undefined;
  private _activeProcess: ChildProcess | undefined;
  private readonly _handlers = new Set<MessageHandler>();

  constructor(binaryPath: string, cwd?: string) {
    this._binaryPath = binaryPath;
    this._cwd = cwd;
  }

  /** Register a handler that receives HostToWebviewMessages as they arrive. */
  onMessage(handler: MessageHandler): void {
    this._handlers.add(handler);
  }

  private _emit(msg: HostToWebviewMessage): void {
    for (const h of this._handlers) {
      h(msg);
    }
  }

  /** Write a JSON message to the active process stdin (for HITL responses, etc.). */
  writeStdin(message: object): void {
    if (!this._activeProcess?.stdin) {
      return;
    }
    this._activeProcess.stdin.write(JSON.stringify(message) + '\n');
  }

  /** Returns true when a process is currently running. */
  get isRunning(): boolean {
    return this._activeProcess !== undefined;
  }

  /**
   * Spawn `ail --once <prompt> [--pipeline <pipeline>] --output-format json`.
   * When pipeline is omitted, ail runs in passthrough mode (no pipeline steps).
   * Rejects immediately if a process is already running.
   */
  start(prompt: string, pipeline?: string, options: StartOptions = {}): Promise<void> {
    if (this._activeProcess) {
      return Promise.reject(new Error('A pipeline is already running'));
    }

    const args = ['--once', prompt, '--output-format', 'json'];
    if (pipeline) {
      args.push('--pipeline', pipeline);
    }
    if (options.headless) {
      args.push('--headless');
    }

    return new Promise<void>((resolve, reject) => {
      // Remove CLAUDECODE to bypass the nested Claude Code session guard.
      const env = { ...process.env };
      delete env['CLAUDECODE'];

      const proc = spawn(this._binaryPath, args, { cwd: this._cwd, env });
      this._activeProcess = proc;

      const mapper = new AilEventMapper();
      parseNdjsonStream(
        proc.stdout!,
        (event: AilEvent) => {
          const msgs = mapper.map(event);
          for (const msg of msgs) {
            this._emit(msg);
          }
        },
        (err) => {
          console.error(`[ail-chat] NDJSON stream error: ${err.message}`);
        }
      );

      // Consume stderr silently
      proc.stderr?.resume();

      proc.on('close', (code) => {
        this._activeProcess = undefined;
        if (code !== 0 && code !== null) {
          this._emit({ type: 'processError', message: `ail exited with code ${code}` });
        }
        resolve();
      });

      proc.on('error', (err) => {
        this._activeProcess = undefined;
        this._emit({ type: 'processError', message: `Failed to spawn ail: ${err.message}` });
        reject(err);
      });
    });
  }

  /** Send SIGTERM, escalate to SIGKILL after 5 seconds. */
  cancel(): void {
    if (!this._activeProcess) {
      return;
    }
    const proc = this._activeProcess;
    proc.kill('SIGTERM');

    const timeout = setTimeout(() => {
      if (this._activeProcess === proc) {
        proc.kill('SIGKILL');
      }
    }, 5000);

    proc.once('close', () => clearTimeout(timeout));
  }
}

/**
 * Stateful event mapper that tracks the current step ID and annotates
 * streamDelta messages so the webview reducer can route deltas to the
 * correct step.
 */
export class AilEventMapper {
  private _currentStepId: string | null = null;

  map(event: AilEvent): HostToWebviewMessage[] {
    if (event.type === 'step_started') {
      this._currentStepId = event.step_id;
    } else if (event.type === 'run_started') {
      this._currentStepId = null;
    }

    const msgs = mapAilEventToMessages(event);

    // Annotate streamDelta messages with current step ID
    if (this._currentStepId) {
      return msgs.map(msg =>
        msg.type === 'streamDelta' ? { ...msg, stepId: this._currentStepId! } : msg
      );
    }
    return msgs;
  }

  reset(): void {
    this._currentStepId = null;
  }
}

/**
 * Map a single AilEvent to zero or more HostToWebviewMessages.
 * Pure function — no side effects.
 */
export function mapAilEventToMessages(event: AilEvent): HostToWebviewMessage[] {
  switch (event.type) {
    case 'run_started':
      return [{ type: 'runStarted', runId: event.run_id, totalSteps: event.total_steps }];

    case 'step_started':
      return [{
        type: 'stepStarted',
        stepId: event.step_id,
        stepIndex: event.step_index,
        totalSteps: event.total_steps,
      }];

    case 'step_completed':
      return [{
        type: 'stepCompleted',
        stepId: event.step_id,
        costUsd: event.cost_usd,
        inputTokens: event.input_tokens,
        outputTokens: event.output_tokens,
        response: event.response,
      }];

    case 'step_skipped':
      return [{ type: 'stepSkipped', stepId: event.step_id }];

    case 'step_failed':
      return [{ type: 'stepFailed', stepId: event.step_id, error: event.error }];

    case 'hitl_gate_reached':
      return [{ type: 'hitlGate', stepId: event.step_id, message: event.message }];

    case 'pipeline_completed':
      return [{ type: 'pipelineCompleted' }];

    case 'pipeline_error':
      return [{ type: 'pipelineError', error: event.error }];

    case 'runner_event': {
      const inner = event.event;
      switch (inner.type) {
        case 'stream_delta':
          return [{ type: 'streamDelta', text: inner.text }];
        case 'thinking':
          return [{ type: 'thinking', text: inner.text }];
        case 'tool_use':
          return [{
            type: 'toolUse',
            toolName: inner.tool_name,
            toolUseId: inner.tool_use_id ?? '',
            input: inner.input,
          }];
        case 'tool_result':
          return [{
            type: 'toolResult',
            toolUseId: inner.tool_use_id ?? '',
            content: inner.content ?? '',
            isError: inner.is_error ?? false,
          }];
        case 'permission_requested':
          return [{
            type: 'permissionRequested',
            displayName: inner.display_name,
            displayDetail: inner.display_detail,
            toolInput: inner.tool_input,
          }];
        default:
          return [];
      }
    }

    default:
      return [];
  }
}
