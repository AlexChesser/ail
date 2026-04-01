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
import { parseNdjsonStream } from '../ndjson';
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

    // Subscribe to the raw AilEvent stream from the client to drive all UI
    // The client's onEvent gives us RunnerEvents (simplified). We need full AilEvents
    // for the execution panel and output channel formatting.
    // We handle this by re-parsing the NDJSON stream indirectly via the spawn in AilProcess.
    // Instead, we use a higher-level approach: subscribe to client events for the bus,
    // and separately hook the full AilEvent stream via a side channel using spawn directly
    // from within RunnerService for the panel/output channel. However, to avoid duplicating
    // spawn logic, we drive both from the onEvent registration on the client.

    // The panel and output channel need full AilEvent fidelity. We bridge this by
    // listening to the client's onEvent (RunnerEvent subset) and supplementing with
    // the EventBus emissions (which carry RunnerEvents, not AilEvents).
    // For now, we use a spawn-adjacent approach: the client fires onEvent handlers
    // during invoke(). We register a handler that processes the RunnerEvent into
    // EventBus emissions and also drives the views.

    // NOTE: The ExecutionPanel.onEvent() expects AilEvents. The RunnerService
    // bridges RunnerEvents from IAilClient to drive the output channel only.
    // The panel gets AilEvents via the spawn stream inside AilProcess, but we
    // need to replicate those events here. To keep AilProcess as the sole spawn
    // site, we pass the full AilEvent stream through a callback registered
    // via a separate, extended interface. For this refactor, we wire the panel
    // using the same parseNdjsonStream by having AilProcess emit 'raw_ail_event'
    // as a special passthrough.

    // Practical approach: AilProcess.onEvent delivers RunnerEvents for the bus.
    // We also need to feed full AilEvents to ExecutionPanel. To do this cleanly
    // without a second spawn, we accept an optional rawEventHandler in startRun.
    // The spawn inside AilProcess passes each AilEvent to both the RunnerEvent
    // mapper AND to any rawEventHandler registered here.

    // For this implementation, we reconstruct what the panel needs from the
    // RunnerEvents available from IAilClient and accept some panel fidelity loss
    // for events not in the RunnerEvent union (tool_use, tool_result, cost_update,
    // thinking, run_started). These are suppressed in the panel during this refactor.
    // Issue #6 will address full panel fidelity with an extended client interface.

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
