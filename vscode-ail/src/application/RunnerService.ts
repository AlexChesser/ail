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
import { UnifiedPanel, IUnifiedPanel } from '../panels/UnifiedPanel';
import { MonitorViewProvider } from '../panels/MonitorViewProvider';
import { ChatViewProvider } from '../views/ChatViewProvider';
import { StepsTreeProvider } from '../views/StepsTreeProvider';
import { AilProcess } from '../infrastructure/AilProcess';
import { IAilClient } from './IAilClient';
import { RunnerEvent } from './events';
import { AilEvent, RunnerEventWrapper } from '../types';
import { HistoryService } from './HistoryService';

/** Minimal panel interface needed by RunnerService — allows injection in tests. */
export interface IStagePanel {
  onEvent(event: AilEvent): void;
  /** Called when a run ends so the panel can release per-run resources. */
  onRunComplete(runId: string): void;
  dispose(): void;
}

/** Injectable factories for per-run dependencies. Defaults to real implementations. */
export interface RunnerDeps {
  createProcess(binaryPath: string, cwd?: string): IAilClient;
  createPanel(
    ctx: vscode.ExtensionContext,
    runId: string,
    writeStdin: (msg: object) => void,
    prompt?: string,
    pipelinePath?: string,
  ): IStagePanel;
}

/** One active pipeline run. */
interface RunContext {
  process: IAilClient;
  panel: IStagePanel;
  startTime: number;
  /** The step currently emitting stream_delta output (for live trace prefixing). */
  currentStepId: string;
  /** Whether the current step's OUTPUT: prefix has been emitted yet. */
  outputPrefixEmitted: boolean;
  /** Cumulative cost in USD for this run. */
  costUsd: number;
  /** Number of steps that have completed (or failed/skipped). */
  completedSteps: number;
  /** Total steps in this run (set from the first step_started event). */
  totalSteps: number;
  /** Whether the cost warning threshold has already fired for this run. */
  costWarningSent: boolean;
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
  private _historyService: HistoryService | undefined;
  /** Summary of the most recently completed run, for post-run status bar display. */
  private _lastRunSummary: { costUsd: number; completed: number; total: number } | undefined;
  /** Timer to hide the status bar after a completed run. */
  private _statusBarHideTimer: ReturnType<typeof setTimeout> | undefined;

  constructor(ctx: ServiceContext, bus: EventBus, deps?: RunnerDeps) {
    this._ctx = ctx;
    this._bus = bus;
    this._deps = deps ?? {
      createProcess: (bin, cwd) => new AilProcess(bin, cwd),
      createPanel: (extCtx, runId, writeStdin, prompt, pipelinePath) => {
        const monitor = MonitorViewProvider.getInstance();
        monitor.startLiveRun(extCtx, runId, writeStdin, prompt || '', pipelinePath || '');
        return monitor;
      },
    };
  }

  /** Register a callback to be invoked after each run completes (used for history refresh). */
  setOnRunComplete(cb: () => void): void {
    this._onRunComplete = cb;
  }

  /** Inject the HistoryService for post-run cost regression detection. */
  setHistoryService(historyService: HistoryService): void {
    this._historyService = historyService;
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

    // Create a dedicated process per run so multiple can coexist.
    const proc = this._deps.createProcess(this._ctx.binaryPath, this._ctx.cwd);

    // createPanel receives runId so the singleton panel can track per-run stdin callbacks.
    const panel = this._deps.createPanel(
      this._ctx.extensionContext,
      runId,
      (msg) => proc.writeStdin(msg),
      prompt,
      pipelinePath,
    );

    const runCtx: RunContext = {
      process: proc,
      panel,
      startTime: Date.now(),
      currentStepId: 'invocation',
      outputPrefixEmitted: false,
      costUsd: 0,
      completedSteps: 0,
      totalSteps: 0,
      costWarningSent: false,
    };
    this._activeRuns.set(runId, runCtx);

    this._updateStatusBar();
    this._chatView?.setRunning(true);
    this._stepsView?.resetStatuses();

    const out = this._ctx.outputChannel;
    const ts = () => new Date().toLocaleTimeString('en', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
    const logLine = (stepId: string, text: string) => out.appendLine(`[${ts()}] [${stepId}] ${text}`);
    out.appendLine(`\n${'─'.repeat(60)}`);
    logLine('ail', `run [${runId}]`);
    logLine('ail', `pipeline: ${pipelinePath}`);
    logLine('ail', `prompt: ${prompt}`);

    // Feed full-fidelity AilEvents to the panel and emit live-trace lines for
    // thinking blocks and tool calls.
    const rawDisposable = proc.onRawEvent((ailEvent: AilEvent) => {
      panel.onEvent(ailEvent);

      if (ailEvent.type === 'runner_event') {
        const wrapper = ailEvent as RunnerEventWrapper;
        const inner = wrapper.event;
        if (inner.type === 'thinking') {
          // Emit a THINKING: prefix for each thinking block (collapsed to one line)
          const summary = inner.text.replace(/\n/g, ' ').slice(0, 120);
          logLine(runCtx.currentStepId, `THINKING: ${summary}`);
          // The next stream_delta will need a fresh OUTPUT: prefix
          runCtx.outputPrefixEmitted = false;
        } else if (inner.type === 'tool_use') {
          logLine(runCtx.currentStepId, `TOOL: ${inner.tool_name}`);
          runCtx.outputPrefixEmitted = false;
        } else if (inner.type === 'tool_result') {
          logLine(runCtx.currentStepId, `TOOL RESULT: ${inner.tool_name}`);
          runCtx.outputPrefixEmitted = false;
        }
      }
    });

    const disposable = proc.onEvent((runnerEvent: RunnerEvent) => {
      // Drive EventBus
      this._bus.emit(runnerEvent);

      // Drive steps view + cost/progress tracking
      switch (runnerEvent.type) {
        case 'step_started':
          this._stepsView?.setStepStatus(runnerEvent.stepId, 'running');
          runCtx.totalSteps = runnerEvent.totalSteps;
          this._updateStatusBar();
          break;
        case 'step_completed':
          this._stepsView?.setStepStatus(runnerEvent.stepId, 'completed');
          runCtx.completedSteps++;
          this._updateStatusBar();
          break;
        case 'step_failed':
          this._stepsView?.setStepStatus(runnerEvent.stepId, 'failed');
          runCtx.completedSteps++;
          this._updateStatusBar();
          break;
        case 'step_skipped':
          this._stepsView?.setStepStatus(runnerEvent.stepId, 'skipped');
          runCtx.completedSteps++;
          this._updateStatusBar();
          break;
        case 'hitl_gate_reached':
          this._stepsView?.setStepStatus(runnerEvent.stepId, 'hitl');
          break;
        case 'cost_update': {
          runCtx.costUsd = runnerEvent.costUsd;
          this._updateStatusBar();
          // Cost warning threshold
          const threshold = vscode.workspace.getConfiguration('ail').get<number>('costWarningThreshold', 0);
          if (threshold > 0 && runnerEvent.costUsd >= threshold && !runCtx.costWarningSent) {
            runCtx.costWarningSent = true;
            void vscode.window.showWarningMessage(
              `ail: Run cost $${runnerEvent.costUsd.toFixed(2)} has exceeded the warning threshold of $${threshold.toFixed(2)}.`
            );
          }
          break;
        }
      }

      // Drive output channel — live trace with step-prefixed lines
      switch (runnerEvent.type) {
        case 'step_started': {
          const e = runnerEvent as Extract<RunnerEvent, { type: 'step_started' }>;
          runCtx.currentStepId = e.stepId;
          runCtx.outputPrefixEmitted = false;
          logLine('ail', `[${e.stepIndex + 1}/${e.totalSteps}] ${e.stepId} — running`);
          break;
        }
        case 'step_completed':
          // Close any open stream_delta line before printing the completion glyph.
          if (runCtx.outputPrefixEmitted) {
            out.appendLine('');
            runCtx.outputPrefixEmitted = false;
          }
          logLine(runnerEvent.stepId, '✓ completed');
          break;
        case 'step_skipped':
          logLine(runnerEvent.stepId, '⊘ skipped');
          break;
        case 'step_failed':
          if (runCtx.outputPrefixEmitted) {
            out.appendLine('');
            runCtx.outputPrefixEmitted = false;
          }
          logLine(runnerEvent.stepId, `✗ failed: ${runnerEvent.error}`);
          break;
        case 'hitl_gate_reached':
          logLine(runnerEvent.stepId, '⏸ waiting for approval');
          break;
        case 'stream_delta': {
          // Emit "[step_id] OUTPUT: " prefix before the first fragment of each step.
          if (!runCtx.outputPrefixEmitted) {
            out.append(`[${ts()}] [${runCtx.currentStepId}] OUTPUT: `);
            runCtx.outputPrefixEmitted = true;
          }
          out.append(runnerEvent.text);
          break;
        }
        case 'pipeline_completed':
          logLine('ail', '✓ pipeline completed');
          break;
        case 'error':
          logLine('ail', `✗ error: ${runnerEvent.message}`);
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
      panel.onRunComplete(runId);
      // Save summary before deleting the run context
      this._lastRunSummary = {
        costUsd: runCtx.costUsd,
        completed: runCtx.completedSteps,
        total: runCtx.totalSteps,
      };
      this._activeRuns.delete(runId);
      this._updateStatusBar();
      if (this._activeRuns.size === 0) {
        this._chatView?.setRunning(false);
      }
      this._onRunComplete?.();

      // Cost regression detection: compare this run's cost to rolling average.
      if (this._historyService && runCtx.costUsd > 0) {
        void this._checkCostRegression(runCtx.costUsd);
      }
    }
  }

  private async _checkCostRegression(thisCostUsd: number): Promise<void> {
    const multiplier = vscode.workspace
      .getConfiguration('ail')
      .get<number>('costRegressionMultiplier', 2.0);

    let avgCost: number;
    try {
      avgCost = await this._historyService!.getRecentAverageCost(10);
    } catch {
      return;
    }

    if (avgCost <= 0) {
      return;
    }

    if (thisCostUsd > multiplier * avgCost) {
      void vscode.window.showWarningMessage(
        `ail: This run cost $${thisCostUsd.toFixed(2)} — ${(thisCostUsd / avgCost).toFixed(1)}x your recent average of $${avgCost.toFixed(2)} per run.`
      );
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

    // Cancel any pending hide timer — a new run starting should reset it.
    if (this._statusBarHideTimer !== undefined) {
      clearTimeout(this._statusBarHideTimer);
      this._statusBarHideTimer = undefined;
    }

    const count = this._activeRuns.size;

    if (count === 0) {
      // Show last run summary for 30 seconds after completion.
      if (this._lastRunSummary) {
        const s = this._lastRunSummary;
        const costStr = s.costUsd > 0 ? `$${s.costUsd.toFixed(2)}` : '—';
        const stepStr = s.total > 0 ? `${s.completed}/${s.total}` : '';
        this._statusBarItem.text = `ail: ${costStr}${stepStr ? ` | ${stepStr} ✓` : ' ✓'}`;
        this._statusBarItem.tooltip = 'ail: last run complete — click to open run monitor';
        this._statusBarItem.command = 'ail.openUnifiedPanel';
        this._statusBarItem.show();
        this._statusBarHideTimer = setTimeout(() => {
          this._statusBarItem?.hide();
          this._statusBarHideTimer = undefined;
        }, 30_000);
      } else {
        this._statusBarItem.hide();
      }
      return;
    }

    // Aggregate cost and step progress across all active runs.
    let totalCost = 0;
    let completedSteps = 0;
    let totalSteps = 0;
    for (const ctx of this._activeRuns.values()) {
      totalCost += ctx.costUsd;
      completedSteps += ctx.completedSteps;
      totalSteps += ctx.totalSteps;
    }

    const costStr = totalCost > 0 ? `$${totalCost.toFixed(2)}` : '—';
    const stepStr = totalSteps > 0 ? ` | ${completedSteps}/${totalSteps}` : '';
    const prefix = count === 1 ? '$(loading~spin) ail:' : `$(loading~spin) ail (${count}):`;
    this._statusBarItem.text = `${prefix} ${costStr}${stepStr}`;
    this._statusBarItem.tooltip = count === 1
      ? 'ail pipeline running — click to open run monitor'
      : `${count} ail pipelines running — click to open run monitor`;
    this._statusBarItem.command = 'ail.openUnifiedPanel';
    this._statusBarItem.show();
  }

  dispose(): void {
    if (this._statusBarHideTimer !== undefined) {
      clearTimeout(this._statusBarHideTimer);
      this._statusBarHideTimer = undefined;
    }
  }
}
