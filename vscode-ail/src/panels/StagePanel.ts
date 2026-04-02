/**
 * StagePanel — unified WebviewPanel for both live execution and history review.
 *
 * Live mode:   driven by onEvent(AilEvent) during a run.
 * Review mode: opened via StagePanel.openReview(context, runRecord).
 *
 * The webview HTML handles both modes: the host sends an `init` message that
 * sets the initial mode and optionally pre-populates steps from a RunRecord.
 *
 * Features vs. ExecutionPanel:
 *   - Telemetry chips on step headers: ⬆100 ⬇50 · $0.003 · 3.2s
 *   - "Inspected Payload" collapsible showing resolved_prompt per step
 *   - Auto-collapse finished steps; expand final step on pipeline completion
 *   - Review mode: populate from RunRecord.steps (TurnEntry array)
 *   - retainContextWhenHidden: true
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

export class StagePanel {
  private static readonly viewType = 'ail.stagePanel';
  private readonly _panel: vscode.WebviewPanel;
  private _disposables: vscode.Disposable[] = [];
  private _totalCost = 0;
  private readonly _stepStartTimes = new Map<string, number>();
  private _writeStdin: ((message: object) => void) | undefined;

  private constructor(panel: vscode.WebviewPanel, writeStdin?: (message: object) => void) {
    this._panel = panel;
    this._writeStdin = writeStdin;
    this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
    this._panel.webview.html = getStagePanelHtml();
    // Route webview → stdin messages
    this._panel.webview.onDidReceiveMessage(
      (msg: { type: string; stepId?: string; text?: string; allowed?: boolean; reason?: string }) => {
        if (!this._writeStdin) return;
        switch (msg.type) {
          case 'hitl_response':
            this._writeStdin({ type: 'hitl_response', step_id: msg.stepId, text: msg.text ?? '' });
            break;
          case 'permission_response':
            this._writeStdin({ type: 'permission_response', allowed: msg.allowed ?? false, reason: msg.reason ?? '' });
            break;
        }
      },
      null,
      this._disposables
    );
  }

  // ── Factory methods ─────────────────────────────────────────────────────────

  /** Create a new panel for a live run. */
  static create(context: vscode.ExtensionContext, writeStdin?: (message: object) => void): StagePanel {
    const panel = vscode.window.createWebviewPanel(
      StagePanel.viewType,
      'ail: Execution Monitor',
      { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true },
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [vscode.Uri.joinPath(context.extensionUri, 'out')],
      }
    );
    const instance = new StagePanel(panel, writeStdin);
    instance._post({ cmd: 'init', mode: 'live' });
    return instance;
  }

  /** Open a panel in review mode for a historical RunRecord. */
  static openReview(context: vscode.ExtensionContext, record: RunRecord): StagePanel {
    const prompt = record.invocationPrompt
      ? truncate(record.invocationPrompt, 40)
      : record.runId.slice(0, 8);
    const panel = vscode.window.createWebviewPanel(
      StagePanel.viewType,
      `ail: ${prompt}`,
      { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true },
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [vscode.Uri.joinPath(context.extensionUri, 'out')],
      }
    );
    const instance = new StagePanel(panel);
    instance._post({
      cmd: 'init',
      mode: 'review',
      runId: record.runId,
      pipelineSource: record.pipelineSource,
      outcome: record.outcome,
      totalCostUsd: record.totalCostUsd,
      invocationPrompt: record.invocationPrompt,
      steps: record.steps,
    });
    return instance;
  }

  // ── Live event feed ─────────────────────────────────────────────────────────

  onEvent(event: AilEvent): void {
    switch (event.type) {
      case 'run_started': {
        const e = event as RunStartedEvent;
        this._totalCost = 0;
        this._stepStartTimes.clear();
        this._panel.title = 'ail: Running';
        this._post({
          cmd: 'runStarted',
          runId: e.run_id,
          totalSteps: e.total_steps,
          pipelineSource: e.pipeline_source ?? 'unknown',
        });
        break;
      }
      case 'step_started': {
        const e = event as StepStartedEvent;
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
        if (e.cost_usd != null) this._totalCost += e.cost_usd;
        const startTime = this._stepStartTimes.get(e.step_id);
        const latencyMs = startTime != null ? Date.now() - startTime : null;
        this._post({
          cmd: 'stepCompleted',
          stepId: e.step_id,
          costUsd: e.cost_usd,
          inputTokens: e.input_tokens,
          outputTokens: e.output_tokens,
          latencyMs,
          totalCost: this._totalCost,
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
        } else if (re.type === 'tool_use') {
          this._post({ cmd: 'toolUse', toolName: re.tool_name });
        } else if (re.type === 'tool_result') {
          this._post({ cmd: 'toolResult', toolName: re.tool_name });
        } else if (re.type === 'cost_update') {
          this._totalCost = re.cost_usd;
          this._post({ cmd: 'costUpdate', costUsd: re.cost_usd, totalCost: this._totalCost });
        } else if (re.type === 'thinking') {
          this._post({ cmd: 'thinking', text: re.text });
        } else if (re.type === 'permission_requested') {
          this._post({
            cmd: 'permissionRequested',
            displayName: re.display_name,
            displayDetail: re.display_detail,
          });
        }
        break;
      }
      case 'pipeline_completed':
        this._panel.title = 'ail: Completed';
        this._post({
          cmd: 'pipelineCompleted',
          outcome: event.outcome,
          totalCost: this._totalCost,
        });
        break;
      case 'pipeline_error':
        this._panel.title = 'ail: Error';
        this._post({ cmd: 'pipelineError', error: event.error, errorType: event.error_type });
        break;
    }
  }

  private _post(message: object): void {
    void this._panel.webview.postMessage(message);
  }

  dispose(): void {
    for (const d of this._disposables) d.dispose();
    this._disposables = [];
  }
}

// ── Helper ────────────────────────────────────────────────────────────────────

function truncate(s: string, maxLen: number): string {
  if (s.length <= maxLen) return s;
  return s.slice(0, maxLen - 1) + '…';
}

// ── WebView HTML ──────────────────────────────────────────────────────────────

function getStagePanelHtml(): string {
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>ail Stage</title>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    font-family: var(--vscode-editor-font-family, monospace);
    font-size: var(--vscode-editor-font-size, 13px);
    background: var(--vscode-editor-background);
    color: var(--vscode-editor-foreground);
    height: 100vh;
    display: flex;
    flex-direction: column;
  }

  /* ── Header ── */
  #header {
    padding: 8px 12px;
    background: var(--vscode-sideBar-background);
    border-bottom: 1px solid var(--vscode-panel-border);
    font-size: 11px;
    color: var(--vscode-descriptionForeground);
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  #status-text { flex: 1; }
  #run-id { font-family: monospace; }
  .review-badge {
    background: var(--vscode-badge-background);
    color: var(--vscode-badge-foreground);
    padding: 1px 6px;
    border-radius: 3px;
    font-size: 10px;
  }

  /* ── Main layout ── */
  #main {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  /* ── Step list ── */
  #steps {
    width: 220px;
    min-width: 180px;
    border-right: 1px solid var(--vscode-panel-border);
    overflow-y: auto;
    padding: 8px 0;
  }
  .step {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    cursor: default;
    font-size: 12px;
    transition: background 0.1s;
  }
  .step.active {
    background: var(--vscode-list-activeSelectionBackground);
    color: var(--vscode-list-activeSelectionForeground);
  }
  .step-glyph { font-size: 14px; width: 16px; text-align: center; }
  .step-id { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .step-cost { font-size: 10px; color: var(--vscode-descriptionForeground); }

  /* Glyph states */
  .glyph-pending    { color: var(--vscode-descriptionForeground); }
  .glyph-running    { color: #3b9eff; }
  .glyph-completed  { color: #4ec994; }
  .glyph-failed     { color: #f48771; }
  .glyph-skipped    { color: var(--vscode-descriptionForeground); }
  .glyph-paused     { color: #e5c07b; }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
  .spinning { display: inline-block; animation: spin 1s linear infinite; }

  /* ── Output area ── */
  #output-container {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  #output {
    flex: 1;
    overflow-y: auto;
    padding: 12px 16px;
    font-family: var(--vscode-editor-font-family, monospace);
    font-size: var(--vscode-editor-font-size, 13px);
    line-height: 1.5;
  }

  /* ── Step sections ── */
  .step-section { margin-bottom: 4px; }
  .step-header {
    color: var(--vscode-descriptionForeground);
    font-size: 11px;
    margin-top: 12px;
    margin-bottom: 4px;
    border-top: 1px solid var(--vscode-panel-border);
    padding-top: 8px;
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .step-header-label { font-weight: bold; }
  .telemetry-chips {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .chip {
    background: var(--vscode-badge-background);
    color: var(--vscode-badge-foreground);
    padding: 1px 5px;
    border-radius: 3px;
    font-size: 10px;
    font-family: monospace;
  }

  /* ── Collapsible blocks ── */
  details.thinking-block,
  details.output-block,
  details.payload-block {
    margin: 4px 0 4px 12px;
    border-left: 2px solid var(--vscode-panel-border);
    padding-left: 8px;
  }
  details.payload-block {
    border-left-color: var(--vscode-editorInfo-foreground);
  }
  details summary {
    cursor: pointer;
    font-size: 11px;
    color: var(--vscode-descriptionForeground);
    padding: 2px 0;
    user-select: none;
    list-style: none;
  }
  details summary::before { content: '▶ '; font-size: 9px; }
  details[open] summary::before { content: '▼ '; font-size: 9px; }
  details summary:hover { color: var(--vscode-foreground); }
  .thinking-content,
  .output-content,
  .payload-content {
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--vscode-editor-font-family, monospace);
    font-size: var(--vscode-editor-font-size, 13px);
    line-height: 1.5;
    margin: 4px 0;
    max-height: 400px;
    overflow-y: auto;
  }
  .payload-content {
    color: var(--vscode-editorInfo-foreground);
    font-size: 11px;
  }
  .tool-badge {
    display: inline-block;
    background: var(--vscode-badge-background);
    color: var(--vscode-badge-foreground);
    padding: 1px 6px;
    border-radius: 3px;
    font-size: 11px;
    margin: 2px 0;
  }
  .error-text { color: var(--vscode-errorForeground); }
  .hitl-banner {
    background: var(--vscode-inputValidation-warningBackground);
    border: 1px solid var(--vscode-inputValidation-warningBorder);
    padding: 8px 12px;
    margin: 8px 0;
    border-radius: 4px;
  }
  .hitl-banner textarea {
    width: 100%;
    min-height: 60px;
    background: var(--vscode-input-background);
    color: var(--vscode-input-foreground);
    border: 1px solid var(--vscode-input-border);
    border-radius: 3px;
    padding: 4px 6px;
    font-family: inherit;
    font-size: 12px;
    resize: vertical;
    margin-top: 6px;
    box-sizing: border-box;
  }
  .hitl-banner .btn-row {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }
  .btn-approve, .btn-reject, .btn-allow, .btn-deny {
    padding: 3px 12px;
    border-radius: 3px;
    border: none;
    cursor: pointer;
    font-size: 12px;
  }
  .btn-approve, .btn-allow {
    background: var(--vscode-button-background);
    color: var(--vscode-button-foreground);
  }
  .btn-approve:hover, .btn-allow:hover {
    background: var(--vscode-button-hoverBackground);
  }
  .btn-reject, .btn-deny {
    background: var(--vscode-button-secondaryBackground);
    color: var(--vscode-button-secondaryForeground);
  }
  .btn-reject:hover, .btn-deny:hover {
    background: var(--vscode-button-secondaryHoverBackground);
  }
  .permission-banner {
    background: var(--vscode-inputValidation-infoBackground);
    border: 1px solid var(--vscode-inputValidation-infoBorder);
    padding: 6px 8px;
    margin: 4px 0;
    border-radius: 3px;
    font-size: 11px;
    word-break: break-all;
  }
  .permission-banner .btn-row {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }

  /* ── Cost bar ── */
  #cost-bar {
    padding: 4px 12px;
    background: var(--vscode-sideBar-background);
    border-top: 1px solid var(--vscode-panel-border);
    font-size: 11px;
    color: var(--vscode-descriptionForeground);
    display: flex;
    gap: 16px;
  }
</style>
</head>
<body>

<div id="header">
  <span id="status-text">Waiting for pipeline...</span>
  <span id="run-id"></span>
</div>

<div id="main">
  <div id="steps">
    <div style="padding: 8px 12px; color: var(--vscode-descriptionForeground); font-size: 11px;">
      No steps yet
    </div>
  </div>
  <div id="output-container">
    <div id="output"></div>
  </div>
</div>

<div id="cost-bar">
  <span id="cost-display">Cost: —</span>
  <span id="step-display">Steps: —</span>
</div>

<script>
  const vscode = acquireVsCodeApi();

  // Step state map: stepId -> { el, glyphEl, tokenEl }
  const steps = new Map();
  let completedSteps = 0;
  let totalSteps = 0;
  let currentStepId = null;
  let isReviewMode = false;

  const stepsContainer = document.getElementById('steps');
  const output = document.getElementById('output');
  const statusText = document.getElementById('status-text');
  const runIdEl = document.getElementById('run-id');
  const costDisplay = document.getElementById('cost-display');
  const stepDisplay = document.getElementById('step-display');

  // ── Step list helpers ──────────────────────────────────────────────────────

  function createStepEl(stepId) {
    const el = document.createElement('div');
    el.className = 'step';
    el.id = 'step-' + stepId;

    const glyph = document.createElement('span');
    glyph.className = 'step-glyph glyph-pending';
    glyph.textContent = '○';

    const label = document.createElement('span');
    label.className = 'step-id';
    label.textContent = stepId;
    label.title = stepId;

    const token = document.createElement('span');
    token.className = 'step-cost';

    el.appendChild(glyph);
    el.appendChild(label);
    el.appendChild(token);

    steps.set(stepId, { el, glyphEl: glyph, tokenEl: token });
    return el;
  }

  function ensureStep(stepId) {
    if (!steps.has(stepId)) {
      const el = createStepEl(stepId);
      if (stepsContainer.querySelector('.step') === null) {
        stepsContainer.innerHTML = '';
      }
      stepsContainer.appendChild(el);
    }
    return steps.get(stepId);
  }

  function setGlyph(stepId, symbol, cssClass) {
    const s = steps.get(stepId);
    if (!s) return;
    s.glyphEl.className = 'step-glyph ' + cssClass;
    if (cssClass === 'glyph-running') {
      s.glyphEl.innerHTML = '<span class="spinning">◌</span>';
    } else {
      s.glyphEl.textContent = symbol;
    }
  }

  // ── Output section helpers ─────────────────────────────────────────────────

  function createStepSection(stepId, resolvedPrompt) {
    const section = document.createElement('div');
    section.className = 'step-section';
    section.id = 'section-' + stepId;

    const header = document.createElement('div');
    header.className = 'step-header';

    const label = document.createElement('span');
    label.className = 'step-header-label';
    label.textContent = '── ' + stepId + ' ──';
    header.appendChild(label);

    const chips = document.createElement('span');
    chips.className = 'telemetry-chips';
    chips.id = 'chips-' + stepId;
    header.appendChild(chips);

    // Payload block (resolved prompt) — only if we have one
    if (resolvedPrompt) {
      const payloadDetails = document.createElement('details');
      payloadDetails.className = 'payload-block';
      const payloadSummary = document.createElement('summary');
      payloadSummary.textContent = 'Inspected Payload';
      const payloadPre = document.createElement('pre');
      payloadPre.className = 'payload-content';
      payloadPre.textContent = resolvedPrompt;
      payloadDetails.appendChild(payloadSummary);
      payloadDetails.appendChild(payloadPre);
      section.appendChild(header);
      section.appendChild(payloadDetails);
    } else {
      section.appendChild(header);
    }

    const thinkingDetails = document.createElement('details');
    thinkingDetails.className = 'thinking-block';
    const thinkingSummary = document.createElement('summary');
    thinkingSummary.textContent = 'Thinking';
    const thinkingPre = document.createElement('pre');
    thinkingPre.className = 'thinking-content';
    thinkingDetails.appendChild(thinkingSummary);
    thinkingDetails.appendChild(thinkingPre);

    const outputDetails = document.createElement('details');
    outputDetails.className = 'output-block';
    const outputSummary = document.createElement('summary');
    outputSummary.textContent = 'Output';
    const outputPre = document.createElement('pre');
    outputPre.className = 'output-content';
    outputDetails.appendChild(outputSummary);
    outputDetails.appendChild(outputPre);

    section.appendChild(thinkingDetails);
    section.appendChild(outputDetails);
    return section;
  }

  function getCurrentSection() {
    return currentStepId ? document.getElementById('section-' + currentStepId) : null;
  }

  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;');
  }

  function setTelemetryChips(stepId, inputTokens, outputTokens, costUsd, latencyMs) {
    const chips = document.getElementById('chips-' + stepId);
    if (!chips) return;
    const parts = [];
    if (inputTokens != null || outputTokens != null) {
      parts.push('<span class="chip">⬆' + (inputTokens || 0) + ' ⬇' + (outputTokens || 0) + '</span>');
    }
    if (costUsd != null && costUsd > 0) {
      parts.push('<span class="chip">$' + costUsd.toFixed(4) + '</span>');
    }
    if (latencyMs != null) {
      const latStr = latencyMs >= 1000
        ? (latencyMs / 1000).toFixed(1) + 's'
        : latencyMs + 'ms';
      parts.push('<span class="chip">' + latStr + '</span>');
    }
    chips.innerHTML = parts.join('');
  }

  // ── Review mode: populate from RunRecord ────────────────────────────────────

  function populateReview(data) {
    stepsContainer.innerHTML = '';
    steps.clear();
    output.innerHTML = '';

    statusText.textContent = data.outcome === 'completed'
      ? '✓ Completed (review)'
      : data.outcome === 'failed'
        ? '✗ Failed (review)'
        : 'Review';

    runIdEl.innerHTML = '<span class="review-badge">review</span> ' + (data.runId || '').slice(0, 8) + '...';

    totalSteps = (data.steps || []).length;
    completedSteps = totalSteps;

    let totalCost = 0;
    for (const entry of (data.steps || [])) {
      const sid = entry.step_id;
      ensureStep(sid);

      // Determine glyph based on outcome or presence of response
      setGlyph(sid, '✓', 'glyph-completed');

      if (entry.cost_usd != null) totalCost += entry.cost_usd;

      const section = createStepSection(sid, entry.prompt);
      output.appendChild(section);

      if (entry.response) {
        const outBlock = section.querySelector('.output-block');
        const outPre = section.querySelector('.output-content');
        if (outBlock && outPre) {
          outPre.textContent = entry.response;
          outBlock.open = true;
        }
      }

      setTelemetryChips(
        sid,
        entry.input_tokens,
        entry.output_tokens,
        entry.cost_usd,
        null
      );
    }

    costDisplay.textContent = 'Cost: $' + totalCost.toFixed(4);
    stepDisplay.textContent = 'Steps: ' + totalSteps;
  }

  // ── HITL / Permission response helpers ──────────────────────────────────

  function submitHitl(stepId, approved) {
    const textEl = document.getElementById('hitl-text-' + stepId);
    const text = textEl ? textEl.value : '';
    const banner = document.getElementById('hitl-' + stepId);
    if (banner) banner.innerHTML = approved
      ? '<span style="color:#4ec994">✓ Approved' + (text ? ': ' + escapeHtml(text) : '') + '</span>'
      : '<span style="color:#f48771">✗ Rejected' + (text ? ': ' + escapeHtml(text) : '') + '</span>';
    vscode.postMessage({ type: 'hitl_response', stepId, text: approved ? text : null });
  }

  function submitPermission(bannerId, allowed) {
    const banner = document.getElementById(bannerId);
    if (banner) {
      const btnRow = banner.querySelector('.btn-row');
      if (btnRow) btnRow.remove();
      banner.insertAdjacentHTML('beforeend', allowed
        ? '<div style="color:#4ec994;margin-top:4px">✓ Allowed</div>'
        : '<div style="color:#f48771;margin-top:4px">✗ Denied</div>');
    }
    vscode.postMessage({ type: 'permission_response', allowed });
  }

  // ── Message handler ──────────────────────────────────────────────────────

  window.addEventListener('message', (event) => {
    const msg = event.data;

    switch (msg.cmd) {

      case 'init': {
        if (msg.mode === 'review') {
          isReviewMode = true;
          populateReview(msg);
        } else {
          isReviewMode = false;
          statusText.textContent = 'Waiting for pipeline...';
        }
        break;
      }

      case 'runStarted': {
        stepsContainer.innerHTML = '';
        steps.clear();
        output.innerHTML = '';
        completedSteps = 0;
        totalSteps = msg.totalSteps;
        currentStepId = null;
        runIdEl.textContent = (msg.runId || '').slice(0, 8) + '...';
        statusText.textContent = 'Running...';
        costDisplay.textContent = 'Cost: —';
        stepDisplay.textContent = 'Steps: 0/' + totalSteps;
        break;
      }

      case 'stepStarted': {
        currentStepId = msg.stepId;
        // Auto-collapse the previous section
        if (output.lastElementChild) {
          const prevOutBlock = output.lastElementChild.querySelector('.output-block');
          if (prevOutBlock && prevOutBlock.open) {
            // keep it open — user may be reading
          }
        }
        const s = ensureStep(msg.stepId);
        document.querySelectorAll('.step.active').forEach(e => e.classList.remove('active'));
        s.el.classList.add('active');
        s.el.scrollIntoView({ block: 'nearest' });
        setGlyph(msg.stepId, '◌', 'glyph-running');
        statusText.textContent = 'Running: ' + msg.stepId;
        stepDisplay.textContent = 'Steps: ' + (msg.stepIndex + 1) + '/' + msg.totalSteps;
        const section = createStepSection(msg.stepId, msg.resolvedPrompt);
        output.appendChild(section);
        output.scrollTop = output.scrollHeight;
        break;
      }

      case 'stepCompleted': {
        const sc = steps.get(msg.stepId);
        if (sc) {
          sc.el.classList.remove('active');
          setGlyph(msg.stepId, '✓', 'glyph-completed');
        }
        setTelemetryChips(msg.stepId, msg.inputTokens, msg.outputTokens, msg.costUsd, msg.latencyMs);
        completedSteps++;
        costDisplay.textContent = 'Cost: $' + ((msg.totalCost || 0).toFixed(4));
        stepDisplay.textContent = 'Steps: ' + completedSteps + '/' + totalSteps;
        break;
      }

      case 'stepSkipped': {
        const ss = steps.get(msg.stepId);
        if (ss) {
          ss.el.classList.remove('active');
          setGlyph(msg.stepId, '⊘', 'glyph-skipped');
        }
        break;
      }

      case 'stepFailed': {
        const sf = steps.get(msg.stepId);
        if (sf) {
          sf.el.classList.remove('active');
          setGlyph(msg.stepId, '✗', 'glyph-failed');
        }
        const failSection = document.getElementById('section-' + msg.stepId);
        if (failSection) {
          const outPre = failSection.querySelector('.output-content');
          if (outPre) outPre.insertAdjacentHTML('beforeend', '<span class="error-text">✗ ' + escapeHtml(msg.error) + '</span>');
          const outBlock = failSection.querySelector('.output-block');
          if (outBlock) outBlock.open = true;
        }
        break;
      }

      case 'hitlGate': {
        const sh = steps.get(msg.stepId);
        if (sh) setGlyph(msg.stepId, '⏸', 'glyph-paused');
        const gateSection = document.getElementById('section-' + msg.stepId);
        if (gateSection) {
          const outPre = gateSection.querySelector('.output-content');
          if (outPre) {
            const bannerId = 'hitl-' + msg.stepId;
            outPre.insertAdjacentHTML('beforeend',
              '<div class="hitl-banner" id="' + bannerId + '">' +
              '<strong>⏸ HITL Gate</strong> — step paused for human review.' +
              '<textarea id="hitl-text-' + msg.stepId + '" placeholder="Optional guidance text..."></textarea>' +
              '<div class="btn-row">' +
              '<button class="btn-approve" onclick="submitHitl(\'' + msg.stepId + '\', true)">Approve</button>' +
              '<button class="btn-reject" onclick="submitHitl(\'' + msg.stepId + '\', false)">Reject</button>' +
              '</div></div>'
            );
          }
          const outBlock = gateSection.querySelector('.output-block');
          if (outBlock) outBlock.open = true;
        }
        statusText.textContent = 'Paused — waiting for approval';
        break;
      }

      case 'streamDelta': {
        const sd = getCurrentSection();
        if (sd) {
          const outBlock = sd.querySelector('.output-block');
          if (outBlock && !outBlock.open) outBlock.open = true;
          const outPre = sd.querySelector('.output-content');
          if (outPre) outPre.insertAdjacentText('beforeend', msg.text);
          output.scrollTop = output.scrollHeight;
        }
        break;
      }

      case 'thinking': {
        const td = getCurrentSection();
        if (td) {
          const thinkBlock = td.querySelector('.thinking-block');
          if (thinkBlock && !thinkBlock.open) thinkBlock.open = true;
          const thinkPre = td.querySelector('.thinking-content');
          if (thinkPre) thinkPre.insertAdjacentText('beforeend', msg.text);
        }
        break;
      }

      case 'toolUse': {
        const tu = getCurrentSection();
        if (tu) {
          const thinkPre = tu.querySelector('.thinking-content');
          if (thinkPre) thinkPre.insertAdjacentHTML('beforeend', '<div><span class="tool-badge">→ ' + escapeHtml(msg.toolName) + '</span></div>');
        }
        break;
      }

      case 'toolResult': {
        const tr = getCurrentSection();
        if (tr) {
          const thinkPre = tr.querySelector('.thinking-content');
          if (thinkPre) thinkPre.insertAdjacentHTML('beforeend', '<div><span class="tool-badge">← ' + escapeHtml(msg.toolName) + '</span></div>');
        }
        break;
      }

      case 'permissionRequested': {
        const pr = getCurrentSection();
        if (pr) {
          const thinkBlock = pr.querySelector('.thinking-block');
          if (thinkBlock && !thinkBlock.open) thinkBlock.open = true;
          const thinkPre = pr.querySelector('.thinking-content');
          if (thinkPre) {
            const permId = 'perm-' + Date.now();
            thinkPre.insertAdjacentHTML('beforeend',
              '<div class="permission-banner" id="' + permId + '">' +
              '🔐 <strong>' + escapeHtml(msg.displayName) + '</strong>: ' + escapeHtml(msg.displayDetail) +
              '<div class="btn-row">' +
              '<button class="btn-allow" onclick="submitPermission(\'' + permId + '\', true)">Allow</button>' +
              '<button class="btn-deny" onclick="submitPermission(\'' + permId + '\', false)">Deny</button>' +
              '</div></div>'
            );
          }
        }
        statusText.textContent = 'Waiting for permission...';
        break;
      }

      case 'costUpdate':
        costDisplay.textContent = 'Cost: $' + ((msg.totalCost || 0).toFixed(4));
        break;

      case 'pipelineCompleted': {
        document.querySelectorAll('.step.active').forEach(e => e.classList.remove('active'));
        statusText.textContent = msg.outcome === 'break'
          ? 'Completed (break)'
          : 'Completed ✓';
        costDisplay.textContent = 'Cost: $' + ((msg.totalCost || 0).toFixed(4));
        // Expand final step's output block
        const sections = document.querySelectorAll('.step-section');
        if (sections.length > 0) {
          const lastSection = sections[sections.length - 1];
          const lastOutBlock = lastSection.querySelector('.output-block');
          if (lastOutBlock) lastOutBlock.open = true;
        }
        break;
      }

      case 'pipelineError': {
        statusText.textContent = '✗ Pipeline error';
        const errSection = getCurrentSection();
        if (errSection) {
          const outPre = errSection.querySelector('.output-content');
          if (outPre) outPre.insertAdjacentHTML('beforeend', '<span class="error-text">[error] ' + escapeHtml(msg.error) + '</span>');
          const errOutBlock = errSection.querySelector('.output-block');
          if (errOutBlock) errOutBlock.open = true;
        }
        break;
      }
    }
  });
</script>
</body>
</html>`;
}
