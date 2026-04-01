/**
 * Execution Monitor WebView panel.
 *
 * Shows a visual pipeline execution state:
 *   - Step list (left) with live status glyphs
 *   - Streaming response text (right)
 *   - Cost bar (bottom)
 *
 * Receives AilEvents via postMessage from the run command.
 */

import * as vscode from "vscode";
import {
  AilEvent,
  StepStartedEvent,
  StepCompletedEvent,
  StepFailedEvent,
  RunStartedEvent,
} from "../types";

export class ExecutionPanel {
  private static readonly viewType = "ail.executionMonitor";
  private readonly _panel: vscode.WebviewPanel;
  private _disposables: vscode.Disposable[] = [];
  private _stepCount = 0;
  private _totalCost = 0;

  private constructor(panel: vscode.WebviewPanel) {
    this._panel = panel;
    this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
    this._panel.webview.html = getWebviewHtml(this._panel.webview);
  }

  /** Create or reveal the panel. */
  static create(context: vscode.ExtensionContext): ExecutionPanel {
    const panel = vscode.window.createWebviewPanel(
      ExecutionPanel.viewType,
      "ail: Execution Monitor",
      { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true },
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [
          vscode.Uri.joinPath(context.extensionUri, "out"),
        ],
      }
    );

    return new ExecutionPanel(panel);
  }

  /** Feed an event into the panel. */
  onEvent(event: AilEvent): void {
    switch (event.type) {
      case "run_started": {
        const e = event as RunStartedEvent;
        this._stepCount = e.total_steps;
        this._totalCost = 0;
        this._panel.title = "ail: Running";
        this._post({ cmd: "runStarted", runId: e.run_id, totalSteps: e.total_steps });
        break;
      }
      case "step_started": {
        const e = event as StepStartedEvent;
        this._post({
          cmd: "stepStarted",
          stepId: e.step_id,
          stepIndex: e.step_index,
          totalSteps: e.total_steps,
        });
        break;
      }
      case "step_completed": {
        const e = event as StepCompletedEvent;
        if (e.cost_usd != null) this._totalCost += e.cost_usd;
        this._post({
          cmd: "stepCompleted",
          stepId: e.step_id,
          costUsd: e.cost_usd,
          inputTokens: e.input_tokens,
          outputTokens: e.output_tokens,
          totalCost: this._totalCost,
        });
        break;
      }
      case "step_skipped":
        this._post({ cmd: "stepSkipped", stepId: event.step_id });
        break;
      case "step_failed": {
        const e = event as StepFailedEvent;
        this._post({ cmd: "stepFailed", stepId: e.step_id, error: e.error });
        break;
      }
      case "hitl_gate_reached":
        this._panel.title = "ail: Waiting for approval";
        this._post({ cmd: "hitlGate", stepId: event.step_id });
        break;
      case "runner_event": {
        const re = event.event;
        if (re.type === "stream_delta") {
          this._post({ cmd: "streamDelta", text: re.text });
        } else if (re.type === "tool_use") {
          this._post({ cmd: "toolUse", toolName: re.tool_name });
        } else if (re.type === "tool_result") {
          this._post({ cmd: "toolResult", toolName: re.tool_name });
        } else if (re.type === "cost_update") {
          this._totalCost = re.cost_usd;
          this._post({ cmd: "costUpdate", costUsd: re.cost_usd, totalCost: this._totalCost });
        } else if (re.type === "thinking") {
          this._post({ cmd: "thinking", text: re.text });
        }
        break;
      }
      case "pipeline_completed":
        this._panel.title = "ail: Completed";
        this._post({ cmd: "pipelineCompleted", outcome: event.outcome, totalCost: this._totalCost });
        break;
      case "pipeline_error":
        this._panel.title = "ail: Error";
        this._post({ cmd: "pipelineError", error: event.error, errorType: event.error_type });
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

// ── WebView HTML ──────────────────────────────────────────────────────────────

function getWebviewHtml(_webview: vscode.Webview): string {
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>ail Execution Monitor</title>
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
  }
  #run-id { font-family: monospace; }

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
  .tool-badge {
    display: inline-block;
    background: var(--vscode-badge-background);
    color: var(--vscode-badge-foreground);
    padding: 1px 6px;
    border-radius: 3px;
    font-size: 11px;
    margin: 2px 0;
  }
  .step-header {
    color: var(--vscode-descriptionForeground);
    font-size: 11px;
    margin-top: 12px;
    margin-bottom: 4px;
    border-top: 1px solid var(--vscode-panel-border);
    padding-top: 8px;
  }
  .step-section { margin-bottom: 4px; }
  details.thinking-block,
  details.output-block {
    margin: 4px 0 4px 12px;
    border-left: 2px solid var(--vscode-panel-border);
    padding-left: 8px;
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
  .output-content {
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--vscode-editor-font-family, monospace);
    font-size: var(--vscode-editor-font-size, 13px);
    line-height: 1.5;
    margin: 4px 0;
    max-height: 400px;
    overflow-y: auto;
  }
  .error-text { color: var(--vscode-errorForeground); }
  .hitl-banner {
    background: var(--vscode-inputValidation-warningBackground);
    border: 1px solid var(--vscode-inputValidation-warningBorder);
    padding: 8px 12px;
    margin: 8px 0;
    border-radius: 4px;
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

  const stepsContainer = document.getElementById('steps');
  const output = document.getElementById('output');
  const statusText = document.getElementById('status-text');
  const runIdEl = document.getElementById('run-id');
  const costDisplay = document.getElementById('cost-display');
  const stepDisplay = document.getElementById('step-display');

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

  function getOrCreateStep(stepId) {
    if (!steps.has(stepId)) {
      const el = createStepEl(stepId);
      stepsContainer.innerHTML = '';
      steps.forEach(s => stepsContainer.appendChild(s.el));
      stepsContainer.appendChild(el);
      steps.set(stepId, { el, glyphEl: el.querySelector('.step-glyph'), tokenEl: el.querySelector('.step-cost') });
    }
    return steps.get(stepId);
  }

  function setGlyph(stepId, symbol, cssClass) {
    const s = steps.get(stepId);
    if (!s) return;
    s.glyphEl.textContent = symbol;
    s.glyphEl.className = 'step-glyph ' + cssClass;
    if (cssClass === 'glyph-running') {
      s.glyphEl.innerHTML = '<span class="spinning">◌</span>';
    }
  }

  function createStepSection(stepId) {
    const section = document.createElement('div');
    section.className = 'step-section';
    section.id = 'section-' + stepId;

    const header = document.createElement('div');
    header.className = 'step-header';
    header.textContent = '── ' + stepId + ' ──';

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

    section.appendChild(header);
    section.appendChild(thinkingDetails);
    section.appendChild(outputDetails);
    return section;
  }

  function getCurrentSection() {
    return currentStepId ? document.getElementById('section-' + currentStepId) : null;
  }

  function escapeHtml(s) {
    return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
  }

  // ── Message handler ──────────────────────────────────────────────────────

  window.addEventListener('message', (event) => {
    const msg = event.data;

    switch (msg.cmd) {
      case 'runStarted': {
        stepsContainer.innerHTML = '';
        steps.clear();
        output.innerHTML = '';
        completedSteps = 0;
        totalSteps = msg.totalSteps;
        currentStepId = null;
        runIdEl.textContent = msg.runId.slice(0, 8) + '...';
        statusText.textContent = 'Running...';
        costDisplay.textContent = 'Cost: —';
        stepDisplay.textContent = \`Steps: 0/\${totalSteps}\`;
        break;
      }

      case 'stepStarted': {
        currentStepId = msg.stepId;
        const s = getOrCreateStep(msg.stepId);
        document.querySelectorAll('.step.active').forEach(e => e.classList.remove('active'));
        s.el.classList.add('active');
        s.el.scrollIntoView({ block: 'nearest' });
        setGlyph(msg.stepId, '◌', 'glyph-running');
        statusText.textContent = \`Running: \${msg.stepId}\`;
        stepDisplay.textContent = \`Steps: \${msg.stepIndex + 1}/\${msg.totalSteps}\`;
        const section = createStepSection(msg.stepId);
        output.appendChild(section);
        output.scrollTop = output.scrollHeight;
        break;
      }

      case 'stepCompleted': {
        const s = steps.get(msg.stepId);
        if (s) {
          s.el.classList.remove('active');
          setGlyph(msg.stepId, '✓', 'glyph-completed');
          s.tokenEl.textContent = \`\${msg.inputTokens} in / \${msg.outputTokens} out\`;
        }
        completedSteps++;
        costDisplay.textContent = \`Cost: $\${(msg.totalCost || 0).toFixed(4)}\`;
        stepDisplay.textContent = \`Steps: \${completedSteps}/\${totalSteps}\`;
        break;
      }

      case 'stepSkipped': {
        const s = steps.get(msg.stepId);
        if (s) {
          s.el.classList.remove('active');
          setGlyph(msg.stepId, '⊘', 'glyph-skipped');
        }
        break;
      }

      case 'stepFailed': {
        const s = steps.get(msg.stepId);
        if (s) {
          s.el.classList.remove('active');
          setGlyph(msg.stepId, '✗', 'glyph-failed');
        }
        const section = getCurrentSection();
        if (section) {
          const outPre = section.querySelector('.output-content');
          outPre.insertAdjacentHTML('beforeend', \`<span class="error-text">✗ \${escapeHtml(msg.error)}</span>\`);
          section.querySelector('.output-block').open = true;
        }
        break;
      }

      case 'hitlGate': {
        const s = steps.get(msg.stepId);
        if (s) setGlyph(msg.stepId, '⏸', 'glyph-paused');
        const section = document.getElementById('section-' + msg.stepId);
        if (section) {
          const outPre = section.querySelector('.output-content');
          outPre.insertAdjacentHTML('beforeend', \`<div class="hitl-banner">⏸ Awaiting approval...</div>\`);
          section.querySelector('.output-block').open = true;
        }
        statusText.textContent = 'Paused — waiting for approval';
        break;
      }

      case 'streamDelta': {
        const section = getCurrentSection();
        if (section) {
          const outPre = section.querySelector('.output-content');
          outPre.insertAdjacentText('beforeend', msg.text);
          output.scrollTop = output.scrollHeight;
        }
        break;
      }

      case 'thinking': {
        const section = getCurrentSection();
        if (section) {
          const thinkPre = section.querySelector('.thinking-content');
          thinkPre.insertAdjacentText('beforeend', msg.text);
        }
        break;
      }

      case 'toolUse': {
        const section = getCurrentSection();
        if (section) {
          const thinkPre = section.querySelector('.thinking-content');
          thinkPre.insertAdjacentHTML('beforeend', \`<div><span class="tool-badge">→ \${escapeHtml(msg.toolName)}</span></div>\`);
        }
        break;
      }

      case 'toolResult': {
        const section = getCurrentSection();
        if (section) {
          const thinkPre = section.querySelector('.thinking-content');
          thinkPre.insertAdjacentHTML('beforeend', \`<div><span class="tool-badge">← \${escapeHtml(msg.toolName)}</span></div>\`);
        }
        break;
      }

      case 'costUpdate':
        costDisplay.textContent = \`Cost: $\${(msg.totalCost || 0).toFixed(4)}\`;
        break;

      case 'pipelineCompleted': {
        document.querySelectorAll('.step.active').forEach(e => e.classList.remove('active'));
        statusText.textContent = msg.outcome === 'break'
          ? \`Completed (break)\`
          : 'Completed ✓';
        costDisplay.textContent = \`Cost: $\${(msg.totalCost || 0).toFixed(4)}\`;
        // Auto-expand the last step's output block
        const sections = document.querySelectorAll('.step-section');
        if (sections.length > 0) {
          const lastSection = sections[sections.length - 1];
          const outputDetails = lastSection.querySelector('.output-block');
          if (outputDetails) outputDetails.open = true;
        }
        break;
      }

      case 'pipelineError':
        statusText.textContent = '✗ Pipeline error';
        const errSection = getCurrentSection();
        if (errSection) {
          const outPre = errSection.querySelector('.output-content');
          outPre.insertAdjacentHTML('beforeend', \`<span class="error-text">[error] \${escapeHtml(msg.error)}</span>\`);
          errSection.querySelector('.output-block').open = true;
        }
        break;
    }
  });
</script>
</body>
</html>`;
}
