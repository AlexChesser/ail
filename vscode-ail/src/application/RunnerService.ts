/**
 * RunnerService — orchestrates pipeline runs.
 *
 * Holds the run state (is a run active?) and co-ordinates between
 * the IAilClient, EventBus, and VS Code UI components (output channel,
 * status bar, views) without importing those directly — it receives them
 * via ServiceContext and explicit callbacks.
 */

import * as vscode from 'vscode';
import { ServiceContext } from './ServiceContext';
import { EventBus, Disposable } from './EventBus';
import { AilEvent, StepStartedEvent } from '../types';
import { ExecutionPanel } from '../panels/ExecutionPanel';
import { ChatViewProvider } from '../views/ChatViewProvider';
import { StepsTreeProvider } from '../views/StepsTreeProvider';

/** Format an AilEvent into a human-readable Output Channel line. Returns undefined to suppress. */
function formatEvent(event: AilEvent): string | undefined {
  switch (event.type) {
    case 'run_started':
      return `\n▶ Pipeline run ${event.run_id} started (${event.total_steps} step(s))`;
    case 'step_started': {
      const e = event as StepStartedEvent;
      return `\n[${e.step_index + 1}/${e.total_steps}] ${e.step_id} — running...`;
    }
    case 'step_completed':
      return `    ✓ ${event.step_id} (${event.input_tokens} in / ${event.output_tokens} out)`;
    case 'step_skipped':
      return `    ⊘ ${event.step_id} (skipped)`;
    case 'step_failed':
      return `    ✗ ${event.step_id}: ${event.error}`;
    case 'hitl_gate_reached':
      return `\n⏸ HITL gate: ${event.step_id} — waiting for approval`;
    case 'runner_event': {
      const re = event.event;
      switch (re.type) {
        case 'stream_delta':
          return re.text;
        case 'tool_use':
          return `\n  → ${re.tool_name}`;
        case 'tool_result':
          return `  ← ${re.tool_name}`;
        case 'cost_update':
          return undefined;
        case 'thinking':
          return undefined;
        case 'completed':
          return undefined;
        default:
          return undefined;
      }
    }
    case 'pipeline_completed':
      return event.outcome === 'break'
        ? `\n✓ Pipeline completed (break at ${event.step_id})`
        : `\n✓ Pipeline completed`;
    case 'pipeline_error':
      return `\n✗ Pipeline error [${event.error_type}]: ${event.error}`;
    default:
      return undefined;
  }
}

export class RunnerService {
  private readonly _ctx: ServiceContext;
  private readonly _bus: EventBus;
  private _isRunning = false;
  private _activePanel: ExecutionPanel | undefined;
  private _chatView: ChatViewProvider | undefined;
  private _stepsView: StepsTreeProvider | undefined;
  private _statusBarItem: vscode.StatusBarItem | undefined;

  constructor(ctx: ServiceContext, bus: EventBus) {
    this._ctx = ctx;
    this._bus = bus;
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

  get isRunning(): boolean {
    return this._isRunning;
  }

  async startRun(prompt: string, pipelinePath: string): Promise<void> {
    if (this._isRunning) {
      void vscode.window.showWarningMessage(
        "An ail pipeline is already running. Use 'Ail: Stop Pipeline' to cancel it first."
      );
      return;
    }

    this._isRunning = true;
    this._updateStatusBar(true);
    this._chatView?.setRunning(true);
    this._stepsView?.resetStatuses();

    this._activePanel = ExecutionPanel.create(this._ctx.extensionContext);

    const out = this._ctx.outputChannel;
    out.show(true);
    out.appendLine(`\n${'─'.repeat(60)}`);
    out.appendLine(`ail run — ${new Date().toLocaleTimeString()}`);
    out.appendLine(`Pipeline: ${pipelinePath}`);
    out.appendLine(`Prompt: ${prompt}`);

    let totalCost = 0;

    // Feed full-fidelity AilEvents to the panel (thinking, tool_use, cost_update, etc.)
    const rawDisposable = this._ctx.client.onRawEvent((ailEvent) => {
      this._activePanel?.onEvent(ailEvent);
    });

    const disposable = this._ctx.client.onEvent((runnerEvent) => {
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
        case 'step_started':
          out.appendLine(`\n[${runnerEvent.stepIndex + 1}/${runnerEvent.totalSteps}] ${runnerEvent.stepId} — running...`);
          break;
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
      await this._ctx.client.invoke(prompt, pipelinePath, {
        headless: true,
        outputFormat: 'json',
      });

      void vscode.window.showInformationMessage('ail: Pipeline completed');
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      out.appendLine(`\n[error] Failed to spawn ail: ${msg}`);
      void vscode.window.showErrorMessage(`ail: ${msg}`);
    } finally {
      rawDisposable.dispose();
      disposable.dispose();
      this._isRunning = false;
      this._activePanel = undefined;
      this._updateStatusBar(false);
      this._chatView?.setRunning(false);
    }
  }

  stopRun(): void {
    if (!this._isRunning) {
      void vscode.window.showInformationMessage('No ail pipeline is running.');
      return;
    }

    const out = this._ctx.outputChannel;
    out.appendLine('\n⏹ Stop requested — sending SIGTERM...');
    this._ctx.client.cancel();
  }

  private _updateStatusBar(running: boolean): void {
    if (!this._statusBarItem) return;
    if (running) {
      this._statusBarItem.text = '$(loading~spin) ail: running';
      this._statusBarItem.tooltip = 'ail pipeline is running — click to stop';
      this._statusBarItem.command = 'ail.stopPipeline';
      this._statusBarItem.show();
    } else {
      this._statusBarItem.hide();
    }
  }
}
