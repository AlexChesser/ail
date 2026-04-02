/**
 * UnifiedPanel — singleton 3-column WebviewPanel.
 *
 * Columns: [Run History] | [Steps] | [Step Detail]
 *
 * A single panel instance is reused across live runs and history reviews.
 * Destroyed only when the user explicitly closes the tab; recreated on next use.
 *
 * Live mode:   driven by onEvent(AilEvent) during a run.
 * Review mode: triggered by ail.openHistoryRun → host loads RunRecord and
 *              posts reviewData to the webview.
 */

import * as vscode from 'vscode';
import {
  AilEvent,
  StepStartedEvent,
  StepCompletedEvent,
  StepFailedEvent,
  RunStartedEvent,
} from '../types';
import { RunRecord } from '../application/HistoryService';
import { HistoryService } from '../application/HistoryService';
import { MessageBuffer } from './MessageBuffer';
import { getUnifiedPanelHtml } from './unifiedPanelHtml';

// ── Public interface for RunnerService ───────────────────────────────────────

export interface IUnifiedPanel {
  onEvent(event: AilEvent): void;
  /** Called by RunnerService when a run ends to release stdin resources. */
  onRunComplete(runId: string): void;
  dispose(): void;
}

// ── RunSummary: lightweight run descriptor for column 1 ──────────────────────

interface RunSummary {
  runId: string;
  timestamp: number;
  pipelineSource: string;
  outcome: string;
  totalCostUsd: number;
  invocationPrompt: string;
  isLive: boolean;
}

// ── UnifiedPanel ─────────────────────────────────────────────────────────────

export class UnifiedPanel implements IUnifiedPanel {
  private static readonly viewType = 'ail.unifiedPanel';

  /** Singleton instance. Cleared in onDidDispose. */
  private static _instance: UnifiedPanel | undefined;

  /** Injected once from extension.ts after activation. */
  private static _historyService: HistoryService | undefined;

  /**
   * Injectable factory for WebviewPanel creation.
   * Override in tests to intercept panel creation without module cache issues.
   */
  static _createWebviewPanel: (
    viewType: string,
    title: string,
    location: { viewColumn: number; preserveFocus: boolean },
    options: object,
  ) => vscode.WebviewPanel = (viewType, title, location, options) =>
    vscode.window.createWebviewPanel(viewType, title, location, options);

  private readonly _panel: vscode.WebviewPanel;
  private readonly _buffer: MessageBuffer;
  private _disposables: vscode.Disposable[] = [];

  /** Per-run stdin callbacks. Keyed by runId. */
  private readonly _writeStdinMap = new Map<string, (msg: object) => void>();

  /** The run currently receiving live events. */
  private _liveRunId: string | undefined;

  /** Step start timestamps for latency calculation. */
  private readonly _stepStartTimes = new Map<string, number>();

  /** Running total cost for the current live run. */
  private _liveTotalCost = 0;

  // ── Constructor ─────────────────────────────────────────────────────────────

  private constructor(panel: vscode.WebviewPanel) {
    this._panel = panel;
    this._buffer = new MessageBuffer((msg) => {
      void this._panel.webview.postMessage(msg);
    });
    this._panel.webview.html = getUnifiedPanelHtml();

    this._panel.onDidDispose(() => this.dispose(), null, this._disposables);

    this._panel.webview.onDidReceiveMessage(
      (msg: { type: string; runId?: string; stepId?: string; text?: string; allowed?: boolean; reason?: string }) => {
        switch (msg.type) {
          case 'ready':
            this._buffer.markReady();
            break;
          case 'selectRun':
            if (msg.runId) void this._handleSelectRun(msg.runId);
            break;
          case 'hitl_response': {
            const cb = this._liveRunId ? this._writeStdinMap.get(this._liveRunId) : undefined;
            cb?.({ type: 'hitl_response', step_id: msg.stepId, text: msg.text ?? '' });
            break;
          }
          case 'permission_response': {
            const cb = this._liveRunId ? this._writeStdinMap.get(this._liveRunId) : undefined;
            cb?.({ type: 'permission_response', allowed: msg.allowed ?? false, reason: msg.reason ?? '' });
            break;
          }
        }
      },
      null,
      this._disposables
    );
  }

  // ── Static factory methods ───────────────────────────────────────────────────

  /** Register the HistoryService once after extension activation. */
  static setHistoryService(svc: HistoryService): void {
    UnifiedPanel._historyService = svc;
  }

  /** Get or create the singleton panel. Does not reveal it. */
  private static _getOrCreate(context: vscode.ExtensionContext): UnifiedPanel {
    if (!UnifiedPanel._instance) {
      const panel = UnifiedPanel._createWebviewPanel(
        UnifiedPanel.viewType,
        'ail: Monitor',
        { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true },
        {
          enableScripts: true,
          retainContextWhenHidden: true,
          localResourceRoots: [vscode.Uri.joinPath(context.extensionUri, 'out')],
        }
      );
      UnifiedPanel._instance = new UnifiedPanel(panel);
      // Send init on creation
      UnifiedPanel._instance._post({ cmd: 'init' });
    }
    return UnifiedPanel._instance;
  }

  /**
   * Called by RunnerService at the start of each run.
   * Reuses the singleton, registers the writeStdin callback, reveals the panel.
   */
  static startLiveRun(
    context: vscode.ExtensionContext,
    runId: string,
    writeStdin: (msg: object) => void,
  ): UnifiedPanel {
    const instance = UnifiedPanel._getOrCreate(context);
    instance._writeStdinMap.set(runId, writeStdin);
    instance._liveRunId = runId;
    instance._liveTotalCost = 0;
    instance._stepStartTimes.clear();
    instance._panel.reveal(vscode.ViewColumn.Beside, true);

    // Push history for column 1 (fire-and-forget)
    void UnifiedPanel._pushHistory(instance);

    return instance;
  }

  /**
   * Called by ail.openHistoryRun to show a past run.
   * Reuses the singleton, loads the record, reveals the panel.
   */
  static openReview(context: vscode.ExtensionContext, record: RunRecord): void {
    const instance = UnifiedPanel._getOrCreate(context);
    instance._panel.reveal(vscode.ViewColumn.Beside, true);

    void UnifiedPanel._pushHistory(instance).then(() => {
      instance._post({
        cmd: 'reviewData',
        runId:           record.runId,
        timestamp:       record.timestamp,
        pipelineSource:  record.pipelineSource,
        outcome:         record.outcome,
        totalCostUsd:    record.totalCostUsd,
        invocationPrompt: record.invocationPrompt,
        steps:           record.steps,
      });
    });
  }

  /**
   * Refresh column 1 history list. Called after each run completes.
   * No-op if no panel exists.
   */
  static async refreshHistory(): Promise<void> {
    if (!UnifiedPanel._instance) return;
    await UnifiedPanel._pushHistory(UnifiedPanel._instance);
  }

  // ── IUnifiedPanel ────────────────────────────────────────────────────────────

  onEvent(event: AilEvent): void {
    switch (event.type) {
      case 'run_started': {
        const e = event as RunStartedEvent;
        this._panel.title = 'ail: Running';
        this._post({
          cmd: 'liveRunStarted',
          runId:          e.run_id,
          totalSteps:     e.total_steps,
          pipelineSource: e.pipeline_source ?? 'unknown',
        });
        break;
      }
      case 'step_started': {
        const e = event as StepStartedEvent;
        this._stepStartTimes.set(e.step_id, Date.now());
        this._post({
          cmd:           'stepStarted',
          stepId:        e.step_id,
          stepIndex:     e.step_index,
          totalSteps:    e.total_steps,
          resolvedPrompt: e.resolved_prompt ?? null,
        });
        break;
      }
      case 'step_completed': {
        const e = event as StepCompletedEvent;
        if (e.cost_usd != null) this._liveTotalCost += e.cost_usd;
        const startTime = this._stepStartTimes.get(e.step_id);
        const latencyMs = startTime != null ? Date.now() - startTime : null;
        this._post({
          cmd:          'stepCompleted',
          stepId:       e.step_id,
          costUsd:      e.cost_usd,
          inputTokens:  e.input_tokens,
          outputTokens: e.output_tokens,
          latencyMs,
          totalCost:    this._liveTotalCost,
        });
        break;
      }
      case 'step_skipped':
        this._post({ cmd: 'stepSkipped', stepId: event.step_id });
        break;
      case 'step_failed': {
        const e = event as StepFailedEvent;
        this._post({ cmd: 'stepFailed', stepId: e.step_id, error: e.error });
        break;
      }
      case 'hitl_gate_reached':
        this._panel.title = 'ail: Waiting for approval';
        this._post({ cmd: 'hitlGate', stepId: event.step_id });
        break;
      case 'runner_event': {
        const re = event.event;
        if (re.type === 'stream_delta') {
          this._post({ cmd: 'streamDelta', text: re.text });
        } else if (re.type === 'thinking') {
          this._post({ cmd: 'thinking', text: re.text });
        } else if (re.type === 'tool_use') {
          this._post({ cmd: 'toolUse', toolName: re.tool_name });
        } else if (re.type === 'tool_result') {
          this._post({ cmd: 'toolResult', toolName: re.tool_name });
        } else if (re.type === 'cost_update') {
          this._liveTotalCost = re.cost_usd;
          this._post({ cmd: 'costUpdate', totalCost: this._liveTotalCost });
        } else if (re.type === 'permission_requested') {
          this._post({
            cmd:           'permissionReq',
            displayName:   re.display_name,
            displayDetail: re.display_detail,
          });
        }
        break;
      }
      case 'pipeline_completed':
        this._panel.title = 'ail: Completed';
        this._post({
          cmd:       'pipelineCompleted',
          outcome:   event.outcome,
          totalCost: this._liveTotalCost,
        });
        break;
      case 'pipeline_error':
        this._panel.title = 'ail: Error';
        this._post({ cmd: 'pipelineError', error: event.error, errorType: event.error_type });
        break;
    }
  }

  onRunComplete(runId: string): void {
    this._writeStdinMap.delete(runId);
    if (this._liveRunId === runId) {
      this._liveRunId = undefined;
    }
  }

  dispose(): void {
    if (UnifiedPanel._instance === this) {
      UnifiedPanel._instance = undefined;
    }
    for (const d of this._disposables) d.dispose();
    this._disposables = [];
  }

  // ── Private helpers ──────────────────────────────────────────────────────────

  private _post(message: object): void {
    this._buffer.post(message);
  }

  private async _handleSelectRun(runId: string): Promise<void> {
    const svc = UnifiedPanel._historyService;
    if (!svc) return;
    const record = await svc.getRunDetail(runId);
    if (!record) return;
    this._post({
      cmd:             'reviewData',
      runId:           record.runId,
      timestamp:       record.timestamp,
      pipelineSource:  record.pipelineSource,
      outcome:         record.outcome,
      totalCostUsd:    record.totalCostUsd,
      invocationPrompt: record.invocationPrompt,
      steps:           record.steps,
    });
  }

  private static async _pushHistory(instance: UnifiedPanel): Promise<void> {
    const svc = UnifiedPanel._historyService;
    if (!svc) return;
    const records = await svc.getHistory();
    const summaries: RunSummary[] = records.map((r) => ({
      runId:            r.runId,
      timestamp:        r.timestamp,
      pipelineSource:   r.pipelineSource,
      outcome:          r.outcome,
      totalCostUsd:     r.totalCostUsd,
      invocationPrompt: r.invocationPrompt,
      isLive:           false,
    }));
    instance._post({ cmd: 'historyUpdated', runs: summaries });
  }
}
