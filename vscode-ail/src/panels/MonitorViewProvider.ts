/**
 * MonitorViewProvider — WebviewView for live run monitoring.
 *
 * Docks in the bottom panel, reusing the UnifiedPanel HTML/CSS/JS.
 * Singleton instance managed by getInstance().
 *
 * Driven by onEvent(AilEvent) during a run.
 */

import * as vscode from 'vscode';
import {
  AilEvent,
  RunStartedEvent,
  StepStartedEvent,
  StepCompletedEvent,
  StepFailedEvent,
  HitlGateReachedEvent,
  RunnerEventWrapper,
} from '../types';
import { HistoryService, RunRecord } from '../application/HistoryService';
import { MessageBuffer } from './MessageBuffer';
import { getUnifiedPanelHtml } from './unifiedPanelHtml';
import { parseStepsFromYaml } from '../utils/parseYaml';

// ── RunSummary: lightweight run descriptor ──────────────────────────────────

interface RunSummary {
  runId: string;
  timestamp: number;
  pipelineSource: string;
  outcome: string;
  totalCostUsd: number;
  invocationPrompt: string;
  isLive: boolean;
}

// ── MonitorViewProvider ──────────────────────────────────────────────────────

export class MonitorViewProvider implements vscode.WebviewViewProvider {
  public static readonly viewId = 'ail.monitorView';

  private static _instance: MonitorViewProvider | undefined;
  private static _historyService: HistoryService | undefined;
  private static _cwd: string = process.cwd();
  private static _cachedHistory: RunSummary[] = [];

  private _view?: vscode.WebviewView;
  private _buffer?: MessageBuffer;
  private _disposables: vscode.Disposable[] = [];

  /** Per-run stdin callbacks. Keyed by runId. */
  private readonly _writeStdinMap = new Map<string, (msg: object) => void>();

  /** The run currently receiving live events. */
  private _liveRunId: string | undefined;

  /** Step start timestamps for latency calculation. */
  private readonly _stepStartTimes = new Map<string, number>();

  /** The step_id of the most recently started step in the live run. */
  private _liveCurrentStepId: string | undefined;

  /** Running total cost for the current live run. */
  private _liveTotalCost = 0;

  /** The prompt text for the current live run. */
  private _livePrompt = '';

  /** Parsed step manifest for the current live run. */
  private _liveStepManifest: { id: string; type: string }[] = [];

  /** Timer handles for HITL gate 2-minute escalation warnings. */
  private readonly _hitlTimers = new Map<string, ReturnType<typeof setTimeout>>();

  // ── Static accessors ────────────────────────────────────────────────────────

  static getInstance(): MonitorViewProvider {
    if (!MonitorViewProvider._instance) {
      MonitorViewProvider._instance = new MonitorViewProvider();
    }
    return MonitorViewProvider._instance;
  }

  static setHistoryService(svc: HistoryService): void {
    MonitorViewProvider._historyService = svc;
  }

  static setCwd(cwd: string): void {
    MonitorViewProvider._cwd = cwd;
  }

  static async initializeHistory(historyService: HistoryService): Promise<void> {
    if (!historyService) return;
    try {
      const records = await historyService.getHistory();
      MonitorViewProvider._cachedHistory = records.map((r) => ({
        runId: r.runId,
        timestamp: r.timestamp,
        pipelineSource: r.pipelineSource,
        outcome: r.outcome,
        totalCostUsd: r.totalCostUsd,
        invocationPrompt: r.invocationPrompt,
        isLive: false,
      }));
    } catch (e) {
      // Silently fail — fallback to loading on first view
    }
  }

  static async refreshHistory(): Promise<void> {
    const instance = MonitorViewProvider._instance;
    if (!instance || !instance._buffer || !MonitorViewProvider._historyService) return;

    const records = await MonitorViewProvider._historyService.getHistory();
    const summaries: RunSummary[] = records.map((r) => ({
      runId: r.runId,
      timestamp: r.timestamp,
      pipelineSource: r.pipelineSource,
      outcome: r.outcome,
      totalCostUsd: r.totalCostUsd,
      invocationPrompt: r.invocationPrompt,
      isLive: false,
    }));
    MonitorViewProvider._cachedHistory = summaries;
    instance._post({ cmd: 'historyUpdated', runs: summaries });
  }

  // ── Constructor ──────────────────────────────────────────────────────────

  private constructor() {
    // Initialization deferred to resolveWebviewView
  }

  resolveWebviewView(webviewView: vscode.WebviewView): void {
    this._view = webviewView;
    this._buffer = new MessageBuffer((msg) => {
      void this._view!.webview.postMessage(msg);
    });

    webviewView.webview.options = { enableScripts: true };
    webviewView.webview.html = getUnifiedPanelHtml();

    // Post cached history immediately if available
    if (MonitorViewProvider._cachedHistory.length > 0) {
      this._post({ cmd: 'historyUpdated', runs: MonitorViewProvider._cachedHistory });
    }

    webviewView.webview.onDidReceiveMessage(
      (msg: { type: string; runId?: string; stepId?: string; text?: string; allowed?: boolean; reason?: string; filePath?: string }) => {
        switch (msg.type) {
          case 'ready':
            this._buffer!.markReady();
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
    );
  }

  // ── Live run API (called by RunnerService) ──────────────────────────────────

  startLiveRun(
    _context: vscode.ExtensionContext,
    runId: string,
    writeStdin: (msg: object) => void,
    prompt: string,
    pipelinePath: string,
  ): void {
    // Reveal the view
    if (this._view) {
      this._view.show?.(true);
    }

    this._liveRunId = runId;
    this._livePrompt = prompt;
    this._liveTotalCost = 0;
    this._liveCurrentStepId = undefined;
    this._stepStartTimes.clear();
    this._writeStdinMap.set(runId, writeStdin);

    // Parse steps from YAML
    try {
      const parsed = parseStepsFromYaml(pipelinePath);
      this._liveStepManifest = [
        { id: 'invocation', type: 'prompt' },
        ...parsed.map((s) => ({ id: s.id, type: s.type })),
      ];
    } catch {
      this._liveStepManifest = [{ id: 'invocation', type: 'prompt' }];
    }

    // Push history (fire-and-forget)
    void MonitorViewProvider.refreshHistory();
  }

  // ── Event handling ──────────────────────────────────────────────────────────

  onEvent(event: AilEvent): void {
    switch (event.type) {
      case 'run_started': {
        const e = event as RunStartedEvent;
        this._post({
          cmd: 'liveRunStarted',
          runId: e.run_id,
          totalSteps: e.total_steps,
          pipelineSource: e.pipeline_source ?? 'unknown',
          invocationPrompt: this._livePrompt,
          stepManifest: this._liveStepManifest,
        });
        break;
      }
      case 'step_started': {
        const e = event as StepStartedEvent;
        this._liveCurrentStepId = e.step_id;
        this._stepStartTimes.set(e.step_id, Date.now());
        this._post({
          cmd: 'stepStarted',
          stepId: e.step_id,
          stepIndex: e.step_index,
          totalSteps: e.total_steps,
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
          cmd: 'stepCompleted',
          stepId: e.step_id,
          costUsd: e.cost_usd,
          inputTokens: e.input_tokens,
          outputTokens: e.output_tokens,
          latencyMs,
          totalCost: this._liveTotalCost,
          response: e.response,
        });
        // Cancel any pending HITL escalation timer for this step
        const pendingTimer = this._hitlTimers.get(e.step_id);
        if (pendingTimer !== undefined) {
          clearTimeout(pendingTimer);
          this._hitlTimers.delete(e.step_id);
        }
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
      case 'hitl_gate_reached': {
        const hEvent = event as HitlGateReachedEvent;
        this._post({ cmd: 'hitlGate', stepId: hEvent.step_id, message: hEvent.message ?? null });

        // VS Code notification
        const notifText = hEvent.message
          ? `ail: Step '${hEvent.step_id}' is waiting for approval — ${hEvent.message}`
          : `ail: Step '${hEvent.step_id}' is waiting for approval.`;
        void vscode.window.showInformationMessage(notifText, 'Show Panel').then((choice) => {
          if (choice === 'Show Panel' && this._view) {
            this._view.show?.(true);
          }
        });

        // 2-minute escalation timer
        const stepId = hEvent.step_id;
        const timer = setTimeout(() => {
          this._hitlTimers.delete(stepId);
          void vscode.window.showWarningMessage(
            `ail: Step '${stepId}' has been waiting for approval for 2 minutes.`,
            'Show Panel',
          ).then((choice) => {
            if (choice === 'Show Panel' && this._view) {
              this._view.show?.(true);
            }
          });
        }, 2 * 60 * 1000);
        this._hitlTimers.set(stepId, timer);
        break;
      }
      case 'runner_event': {
        const re = (event as RunnerEventWrapper).event;
        if (re.type === 'stream_delta') {
          this._post({ cmd: 'streamDelta', text: re.text });
        } else if (re.type === 'thinking') {
          this._post({ cmd: 'thinking', text: re.text });
        } else if (re.type === 'tool_use') {
          this._post({ cmd: 'toolUse', toolName: re.tool_name, toolUseId: re.tool_use_id, input: re.input });
        } else if (re.type === 'tool_result') {
          this._post({
            cmd: 'toolResult',
            toolName: re.tool_name,
            toolUseId: re.tool_use_id,
            content: re.content,
            isError: re.is_error,
          });
        } else if (re.type === 'cost_update') {
          this._liveTotalCost = re.cost_usd;
          this._post({ cmd: 'costUpdate', totalCost: this._liveTotalCost });
        } else if (re.type === 'permission_requested') {
          this._post({
            cmd: 'permissionReq',
            displayName: re.display_name,
            displayDetail: re.display_detail,
          });
          // VS Code notification
          void vscode.window.showInformationMessage(
            `ail: Tool permission requested for '${re.display_name}'.`,
            'Show Panel',
          ).then((choice) => {
            if (choice === 'Show Panel' && this._view) {
              this._view.show?.(true);
            }
          });
        }
        break;
      }
      case 'pipeline_completed':
        // Post stepResultCode for break outcome
        if (event.outcome === 'break' && event.step_id) {
          this._post({ cmd: 'stepResultCode', stepId: event.step_id, resultCode: 'break' });
        }
        this._post({
          cmd: 'pipelineCompleted',
          outcome: event.outcome,
          totalCost: this._liveTotalCost,
        });
        break;
      case 'pipeline_error':
        // Post stepResultCode for aborted pipeline
        if (event.error_type === 'ail:pipeline/aborted' && this._liveCurrentStepId) {
          this._post({ cmd: 'stepResultCode', stepId: this._liveCurrentStepId, resultCode: 'abort_pipeline' });
        }
        this._post({ cmd: 'pipelineError', error: event.error, errorType: event.error_type });
        break;
    }
  }

  onRunComplete(runId: string): void {
    this._writeStdinMap.delete(runId);
    if (this._liveRunId === runId) {
      this._liveRunId = undefined;
      this._liveCurrentStepId = undefined;
      // Clear any lingering HITL escalation timers from this run
      for (const [stepId, timer] of this._hitlTimers) {
        clearTimeout(timer);
        this._hitlTimers.delete(stepId);
      }
    }
  }

  // ── Private helpers ────────────────────────────────────────────────────────

  private _post(message: object): void {
    this._buffer?.post(message);
  }

  private async _handleSelectRun(runId: string): Promise<void> {
    if (!MonitorViewProvider._historyService) return;
    try {
      const record = await MonitorViewProvider._historyService.getRunDetail(runId);
      if (record) {
        this._post({
          cmd: 'reviewData',
          runId: record.runId,
          timestamp: record.timestamp,
          pipelineSource: record.pipelineSource,
          outcome: record.outcome,
          totalCostUsd: record.totalCostUsd,
          invocationPrompt: record.invocationPrompt,
        });
      }
    } catch (err) {
      vscode.window.showErrorMessage(`Failed to load run details: ${String(err)}`);
    }
  }

  dispose(): void {
    this._hitlTimers.forEach((timer) => clearTimeout(timer));
    this._hitlTimers.clear();
    this._disposables.forEach((d) => d.dispose());
    this._disposables = [];
  }
}
