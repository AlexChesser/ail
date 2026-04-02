/**
 * RunnerService — orchestrates pipeline runs.
 *
 * Supports multiple concurrent pipeline runs. Each run gets its own
 * AilProcess instance identified by a UUID. The shared client on
 * ServiceContext is used only for validate() calls.
 */

import * as vscode from 'vscode';
import { randomUUID } from 'crypto';
import { ServiceContext } from './ServiceContext';
import { EventBus } from './EventBus';
import { StagePanel } from '../panels/StagePanel';
import { ChatViewProvider } from '../views/ChatViewProvider';
import { StepsTreeProvider } from '../views/StepsTreeProvider';
import { AilProcess } from '../infrastructure/AilProcess';
import { IAilClient } from './IAilClient';
import { RunnerEvent } from './events';
import { AilEvent } from '../types';

/** Minimal panel interface needed by RunnerService — allows injection in tests. */
export interface IStagePanel {
  onEvent(event: AilEvent): void;
  dispose(): void;
}

/** Injectable factories for per-run dependencies. Defaults to real implementations. */
export interface RunnerDeps {
  createProcess(binaryPath: string, cwd?: string): IAilClient;
  createPanel(
    ctx: vscode.ExtensionContext,
    writeStdin?: (msg: object) => void,
  ): IStagePanel;
}

/** One active pipeline run. */
interface RunContext {
  process: IAilClient;
  panel: IStagePanel;
  startTime: number;
}

export class RunnerService {
  private readonly _ctx: ServiceContext;
  private readonly _bus: EventBus;
  private readonly _deps: RunnerDeps;
  /** Active runs keyed by run UUID. */
  private readonly _activeRuns = new Map<string, RunContext>();
  private _chatView: ChatViewProvider | undefined;
  private _stepsView: StepsTreeProvider | undefined;
  private _statusBarItem: vscode.StatusBarItem | undefined;
  private _onRunComplete: (() => void) | undefined;

  constructor(ctx: ServiceContext, bus: EventBus, deps?: RunnerDeps) {
    this._ctx = ctx;
    this._bus = bus;
    this._deps = deps ?? {
      createProcess: (bin, cwd) => new AilProcess(bin, cwd),
      createPanel: (extCtx, writeStdin) => StagePanel.create(extCtx, writeStdin),
    };
  }

  /** Register a callback to be invoked after each run completes (used for history refresh). */
  setOnRunComplete(cb: () => void): void {
    this._onRunComplete = cb;
  }

  /** Inject optional view/UI references (call from extension.ts after views are created). */
  setViews(
    statusBarItem: vscode.StatusBarItem,
    chatView?: ChatViewProvider,
    stepsView?: StepsTreeProvider,
  ): void {
    this._statusBarItem = statusBarItem;
    this._chatView = chatView;
    this._stepsView = stepsView;
  }

  /** True if at least one run is currently active. */
  get isRunning(): boolean {
    return this._activeRuns.size > 0;
  }

  async startRun(prompt: string, pipelinePath: string, env?: Record<string, string>): Promise<void> {
    const runId = randomUUID();

    // Create a dedicated process + panel per run so multiple can coexist.
    const proc = this._deps.createProcess(this._ctx.binaryPath, this._ctx.cwd);

    const panel = this._deps.createPanel(
      this._ctx.extensionContext,
      (msg) => proc.writeStdin(msg),
    );

    const runCtx: RunContext = { process: proc, panel, startTime: Date.now() };
    this._activeRuns.set(runId, runCtx);

    this._updateStatusBar();
    this._chatView?.setRunning(true);
    this._stepsView?.resetStatuses();

    const out = this._ctx.outputChannel;
    out.show(true);
    out.appendLine(`\n${'─'.repeat(60)}`);
    out.appendLine(`ail run [${runId}] — ${new Date().toLocaleTimeString()}`);
    out.appendLine(`Pipeline: ${pipelinePath}`);
    out.appendLine(`Prompt: ${prompt}`);

    // Feed full-fidelity AilEvents to the panel.
    const rawDisposable = proc.onRawEvent((ailEvent) => {
      panel.onEvent(ailEvent);
    });

    const disposable = proc.onEvent((runnerEvent: RunnerEvent) => {
      // Drive EventBus
      this._bus.emit(runnerEvent);

      // Drive steps view
      switch (runnerEvent.type) {
        case 'step_started':
          this._stepsView?.setStepStatus(runnerEvent.stepId, 'running');
          break;
        case 'step_completed':
          this._stepsView?.setStepStatus(runnerEvent.stepId, 'completed');
          break;
        case 'step_failed':
          this._stepsView?.setStepStatus(runnerEvent.stepId, 'failed');
          break;
        case 'step_skipped':
          this._stepsView?.setStepStatus(runnerEvent.stepId, 'skipped');
          break;
        case 'hitl_gate_reached':
          this._stepsView?.setStepStatus(runnerEvent.stepId, 'hitl');
          break;
      }

      // Drive output channel
      switch (runnerEvent.type) {
        case 'step_started': {
          const e = runnerEvent as Extract<RunnerEvent, { type: 'step_started' }>;
          out.appendLine(`\n[${e.stepIndex + 1}/${e.totalSteps}] ${e.stepId} — running...`);
          break;
        }
        case 'step_completed':
          out.appendLine(`    ✓ ${runnerEvent.stepId}`);
          break;
        case 'step_skipped':
          out.appendLine(`    ⊘ ${runnerEvent.stepId} (skipped)`);
          break;
        case 'step_failed':
          out.appendLine(`    ✗ ${runnerEvent.stepId}: ${runnerEvent.error}`);
          break;
        case 'hitl_gate_reached':
          out.appendLine(`\n⏸ HITL gate: ${runnerEvent.stepId} — waiting for approval`);
          break;
        case 'stream_delta':
          out.append(runnerEvent.text);
          break;
        case 'pipeline_completed':
          out.appendLine(`\n✓ Pipeline completed`);
          break;
        case 'error':
          out.appendLine(`\n✗ Error: ${runnerEvent.message}`);
          break;
      }
    });

    try {
      await proc.invoke(prompt, pipelinePath, {
        headless: true,
        outputFormat: 'json',
        env,
      });

      void vscode.window.showInformationMessage('ail: Pipeline completed');
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      out.appendLine(`\n[error] Failed to spawn ail: ${msg}`);
      void vscode.window.showErrorMessage(`ail: ${msg}`);
    } finally {
      rawDisposable.dispose();
      disposable.dispose();
      this._activeRuns.delete(runId);
      this._updateStatusBar();
      if (this._activeRuns.size === 0) {
        this._chatView?.setRunning(false);
      }
      this._onRunComplete?.();
    }
  }

  /**
   * Stop a run. If runId is given, stop that specific run. If omitted,
   * stop the most recently started run (last entry in the map).
   */
  stopRun(runId?: string): void {
    if (this._activeRuns.size === 0) {
      void vscode.window.showInformationMessage('No ail pipeline is running.');
      return;
    }

    const targetId = runId ?? [...this._activeRuns.keys()].at(-1);
    if (!targetId) {
      return;
    }

    const ctx = this._activeRuns.get(targetId);
    if (!ctx) {
      void vscode.window.showWarningMessage(`ail: No active run with id ${targetId}.`);
      return;
    }

    const out = this._ctx.outputChannel;
    out.appendLine(`\n⏹ Stop requested for run [${targetId}] — sending SIGTERM...`);
    ctx.process.cancel();
  }

  private _updateStatusBar(): void {
    if (!this._statusBarItem) return;
    const count = this._activeRuns.size;
    if (count === 0) {
      this._statusBarItem.hide();
    } else if (count === 1) {
      this._statusBarItem.text = '$(loading~spin) ail: running';
      this._statusBarItem.tooltip = 'ail pipeline is running — click to stop';
      this._statusBarItem.command = 'ail.stopPipeline';
      this._statusBarItem.show();
    } else {
      this._statusBarItem.text = `$(loading~spin) ail: ${count} running`;
      this._statusBarItem.tooltip = `${count} ail pipelines are running — click to stop the most recent`;
      this._statusBarItem.command = 'ail.stopPipeline';
      this._statusBarItem.show();
    }
  }
}
