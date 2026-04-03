/**
 * getUnifiedPanelHtml — returns the full self-contained HTML for the
 * 3-column UnifiedPanel webview.
 *
 * Layout:
 *   Column 1 (~200px): Run history (live run at top + historical runs)
 *   Column 2 (~220px): Steps in the selected run
 *   Column 3 (flex):   Detail for the selected step (thinking / output / tools / HITL)
 *
 * Host → webview messages:
 *   init             Reset everything.
 *   historyUpdated   Refresh column 1 from RunSummary[].
 *   liveRunStarted   Add/update the live entry in column 1, auto-select.
 *   stepStarted      Add step to column 2, auto-select, clear column 3.
 *   stepCompleted    Update step glyph; store telemetry.
 *   stepFailed       Update step glyph; append error to stored output.
 *   stepSkipped      Update step glyph.
 *   streamDelta      Append text to current live step's output.
 *   thinking         Append text to current live step's thinking.
 *   toolUse          Append badge to current live step's thinking.
 *   toolResult       Append badge to current live step's thinking.
 *   hitlGate         Render HITL banner in current live step's detail.
 *   permissionReq    Render permission banner in current live step's thinking.
 *   costUpdate       Update cost bar.
 *   pipelineCompleted Mark live run as complete in column 1.
 *   pipelineError    Mark live run as error in column 1.
 *   reviewData       Populate columns 2+3 for a historical run.
 *
 * Webview → host messages:
 *   ready            Webview script has loaded and is ready for messages.
 *   selectRun        User clicked a historical run in column 1.
 *   hitl_response    User approved/rejected a HITL gate.
 *   permission_response User allowed/denied a permission request.
 */
export function getUnifiedPanelHtml(): string {
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>ail Monitor</title>
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
    overflow: hidden;
  }

  /* ── Column container ── */
  #app {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  /* ── Shared column style ── */
  .col {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    flex-shrink: 0;
  }

  #col-history { width: 200px; border-right: 1px solid var(--vscode-panel-border); }
  #col-steps   { width: 220px; border-right: 1px solid var(--vscode-panel-border); }
  #col-detail  { flex: 1; flex-shrink: 1; min-width: 0; }

  .col-header {
    padding: 5px 10px;
    font-size: 10px;
    font-weight: bold;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--vscode-descriptionForeground);
    background: var(--vscode-sideBar-background);
    border-bottom: 1px solid var(--vscode-panel-border);
    flex-shrink: 0;
  }

  .col-body {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }

  /* ── Column 1: Run list ── */
  .run-item {
    padding: 6px 10px;
    cursor: pointer;
    border-left: 2px solid transparent;
    transition: background 0.1s;
  }
  .run-item:hover { background: var(--vscode-list-hoverBackground); }
  .run-item.selected {
    background: var(--vscode-list-activeSelectionBackground);
    color: var(--vscode-list-activeSelectionForeground);
    border-left-color: var(--vscode-focusBorder, #007acc);
  }
  .run-glyph {
    font-size: 12px;
    margin-right: 5px;
  }
  .run-meta {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
  }
  .run-time { color: var(--vscode-descriptionForeground); }
  .run-item.selected .run-time { color: inherit; opacity: 0.75; }
  .run-pipeline { font-weight: bold; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .run-cost { font-size: 10px; color: var(--vscode-descriptionForeground); margin-left: auto; }
  .run-item.selected .run-cost { color: inherit; opacity: 0.75; }
  .run-prompt {
    font-size: 10px;
    color: var(--vscode-descriptionForeground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin-top: 1px;
  }
  .run-item.selected .run-prompt { color: inherit; opacity: 0.75; }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
  .spinning { display: inline-block; animation: spin 1s linear infinite; }

  /* ── Column 2: Step list ── */
  .step-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    cursor: pointer;
    font-size: 12px;
    border-left: 2px solid transparent;
    transition: background 0.1s;
  }
  .step-item:hover { background: var(--vscode-list-hoverBackground); }
  .step-item.selected {
    background: var(--vscode-list-activeSelectionBackground);
    color: var(--vscode-list-activeSelectionForeground);
    border-left-color: var(--vscode-focusBorder, #007acc);
  }
  .step-glyph { width: 16px; text-align: center; font-size: 13px; }
  .step-label { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .step-cost  { font-size: 10px; color: var(--vscode-descriptionForeground); }
  .step-item.selected .step-cost { color: inherit; opacity: 0.75; }

  .glyph-pending   { color: var(--vscode-descriptionForeground); }
  .glyph-running   { color: #3b9eff; }
  .glyph-completed { color: #4ec994; }
  .glyph-failed    { color: #f48771; }
  .glyph-skipped   { color: var(--vscode-descriptionForeground); opacity: 0.6; }
  .glyph-paused    { color: #e5c07b; }

  /* ── Column 3: Step detail ── */
  #detail-body {
    flex: 1;
    overflow-y: auto;
    padding: 10px 14px;
    font-family: var(--vscode-editor-font-family, monospace);
    font-size: var(--vscode-editor-font-size, 13px);
    line-height: 1.5;
  }

  #detail-header-bar {
    padding: 5px 14px;
    font-size: 11px;
    color: var(--vscode-descriptionForeground);
    background: var(--vscode-sideBar-background);
    border-bottom: 1px solid var(--vscode-panel-border);
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }
  #detail-step-label { font-weight: bold; color: var(--vscode-foreground); }
  .telemetry-chips { display: flex; gap: 4px; flex-wrap: wrap; }
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
    margin: 4px 0;
    border-left: 2px solid var(--vscode-panel-border);
    padding-left: 8px;
  }
  details.payload-block { border-left-color: var(--vscode-editorInfo-foreground); }
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
  .block-content {
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

  /* ── HITL + permission banners ── */
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
  .btn-row { display: flex; gap: 8px; margin-top: 6px; }
  .btn-approve, .btn-allow {
    padding: 3px 12px; border-radius: 3px; border: none; cursor: pointer;
    font-size: 12px;
    background: var(--vscode-button-background);
    color: var(--vscode-button-foreground);
  }
  .btn-approve:hover, .btn-allow:hover { background: var(--vscode-button-hoverBackground); }
  .btn-reject, .btn-deny {
    padding: 3px 12px; border-radius: 3px; border: none; cursor: pointer;
    font-size: 12px;
    background: var(--vscode-button-secondaryBackground);
    color: var(--vscode-button-secondaryForeground);
  }
  .btn-reject:hover, .btn-deny:hover { background: var(--vscode-button-secondaryHoverBackground); }
  .permission-banner {
    background: var(--vscode-inputValidation-infoBackground);
    border: 1px solid var(--vscode-inputValidation-infoBorder);
    padding: 6px 8px;
    margin: 4px 0;
    border-radius: 3px;
    font-size: 11px;
    word-break: break-all;
  }

  /* ── Empty / placeholder states ── */
  .empty-state {
    padding: 12px 10px;
    font-size: 11px;
    color: var(--vscode-descriptionForeground);
    font-style: italic;
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
    flex-shrink: 0;
  }
</style>
</head>
<body>

<div id="app">

  <!-- Column 1: Run history -->
  <div id="col-history" class="col">
    <div class="col-header">Runs</div>
    <div id="history-list" class="col-body">
      <div class="empty-state">No runs yet</div>
    </div>
  </div>

  <!-- Column 2: Steps in selected run -->
  <div id="col-steps" class="col">
    <div class="col-header">Steps</div>
    <div id="steps-list" class="col-body">
      <div class="empty-state">Select a run</div>
    </div>
  </div>

  <!-- Column 3: Detail for selected step -->
  <div id="col-detail" class="col">
    <div id="detail-header-bar">
      <span id="detail-step-label">Select a step</span>
      <span class="telemetry-chips" id="detail-chips"></span>
    </div>
    <div id="detail-body"></div>
  </div>

</div>

<div id="cost-bar">
  <span id="cost-display">Cost: —</span>
  <span id="step-display">Steps: —</span>
</div>

<script>
  const vscode = acquireVsCodeApi();

  // ── DOM references ───────────────────────────────────────────────────────────
  const historyList   = document.getElementById('history-list');
  const stepsList     = document.getElementById('steps-list');
  const detailBody    = document.getElementById('detail-body');
  const detailLabel   = document.getElementById('detail-step-label');
  const detailChips   = document.getElementById('detail-chips');
  const costDisplay   = document.getElementById('cost-display');
  const stepDisplay   = document.getElementById('step-display');

  // ── State ────────────────────────────────────────────────────────────────────

  // Map<runId, RunEntry>
  // RunEntry: { summary: RunSummary, steps: Map<stepId, StepEntry> }
  // RunSummary: { runId, timestamp, pipelineSource, outcome, totalCostUsd, invocationPrompt, isLive }
  // StepEntry: { id, status, thinking, output, tools, hitlHtml, permHtmls, telemetry }
  const runs = new Map();

  let selectedRunId    = null;  // shown in col 2+3
  let selectedStepId   = null;  // shown in col 3
  let liveRunId        = null;  // the run currently receiving events
  let liveCurrentStepId = null; // the step currently streaming

  // Live DOM references for efficient streaming (null when not in live-streaming mode)
  let liveThinkingPre  = null;
  let liveOutputPre    = null;

  // ── Helpers ──────────────────────────────────────────────────────────────────

  function esc(s) {
    return String(s)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;');
  }

  function relTime(ts) {
    const diff = Math.floor((Date.now() - ts) / 1000);
    if (diff < 60)          return 'just now';
    if (diff < 3600)        return Math.floor(diff / 60) + 'm ago';
    if (diff < 86400)       return Math.floor(diff / 3600) + 'h ago';
    if (diff < 86400 * 7)   return Math.floor(diff / 86400) + 'd ago';
    return new Date(ts).toLocaleDateString();
  }

  function pipelineLabel(src) {
    if (!src || src === 'unknown') return 'unknown';
    const parts = src.replace(/\\\\/g, '/').split('/');
    return parts[parts.length - 1] || src;
  }

  function outcomeGlyph(outcome, isLive) {
    if (isLive) return '<span class="spinning">◌</span>';
    if (outcome === 'completed') return '✓';
    if (outcome === 'failed')    return '✗';
    return '●';
  }

  function makeStepEntry() {
    return { status: 'pending', thinking: '', output: '', tools: [], hitlHtml: null, permHtmls: [], telemetry: null };
  }

  // ── Column 1: Run list rendering ─────────────────────────────────────────────

  function renderHistoryList() {
    if (runs.size === 0) {
      historyList.innerHTML = '<div class="empty-state">No runs yet</div>';
      return;
    }
    historyList.innerHTML = '';

    // Sort: live run first, then by timestamp descending
    const sorted = [...runs.values()].sort((a, b) => {
      if (a.summary.isLive && !b.summary.isLive) return -1;
      if (!a.summary.isLive && b.summary.isLive) return 1;
      return b.summary.timestamp - a.summary.timestamp;
    });

    for (const entry of sorted) {
      const s = entry.summary;
      const el = document.createElement('div');
      el.className = 'run-item' + (s.runId === selectedRunId ? ' selected' : '');
      el.dataset.runId = s.runId;

      const costStr = s.totalCostUsd > 0 ? '$' + s.totalCostUsd.toFixed(4) : '';
      const promptStr = s.invocationPrompt ? s.invocationPrompt.slice(0, 55) : '';

      el.innerHTML =
        '<div class="run-meta">' +
          '<span class="run-glyph">' + outcomeGlyph(s.outcome, s.isLive) + '</span>' +
          '<span class="run-pipeline">' + esc(pipelineLabel(s.pipelineSource)) + '</span>' +
          (costStr ? '<span class="run-cost">' + esc(costStr) + '</span>' : '') +
        '</div>' +
        '<div class="run-meta" style="margin-top:1px">' +
          '<span class="run-time">' + relTime(s.timestamp) + '</span>' +
        '</div>' +
        (promptStr ? '<div class="run-prompt">' + esc(promptStr) + (s.invocationPrompt && s.invocationPrompt.length > 55 ? '…' : '') + '</div>' : '');

      el.addEventListener('click', () => onRunClick(s.runId));
      historyList.appendChild(el);
    }
  }

  function onRunClick(runId) {
    if (runId === selectedRunId) return;
    if (runId === liveRunId) {
      // Switch to live run — steps already in memory
      selectRun(runId, false);
    } else {
      // Request historical data from host
      vscode.postMessage({ type: 'selectRun', runId });
    }
  }

  // ── Column 1 highlight update (cheaper than full re-render) ──────────────────

  function updateRunHighlight() {
    historyList.querySelectorAll('.run-item').forEach((el) => {
      el.classList.toggle('selected', el.dataset.runId === selectedRunId);
    });
  }

  // ── Column 2: Step list rendering ────────────────────────────────────────────

  function selectRun(runId, autoSelectStep) {
    selectedRunId = runId;
    updateRunHighlight();
    renderStepsList(runId, autoSelectStep);
  }

  function renderStepsList(runId, autoSelectStep) {
    const entry = runs.get(runId);
    if (!entry || entry.steps.size === 0) {
      stepsList.innerHTML = '<div class="empty-state">No steps yet</div>';
      stepDisplay.textContent = 'Steps: —';
      clearDetailPanel('Select a step');
      selectedStepId = null;
      return;
    }

    stepsList.innerHTML = '';
    let firstStepId = null;
    for (const [stepId, step] of entry.steps) {
      if (!firstStepId) firstStepId = stepId;
      renderStepItem(stepId, step, runId);
    }

    const total = entry.steps.size;
    const done  = [...entry.steps.values()].filter(s => s.status === 'completed' || s.status === 'failed' || s.status === 'skipped').length;
    stepDisplay.textContent = 'Steps: ' + done + '/' + total;

    if (autoSelectStep) {
      // Auto-select last step (most recent)
      const lastStepId = [...entry.steps.keys()].at(-1);
      if (lastStepId) selectStep(runId, lastStepId);
    } else if (firstStepId) {
      selectStep(runId, firstStepId);
    }
  }

  function renderStepItem(stepId, step, runId) {
    const el = document.createElement('div');
    el.className = 'step-item' + (stepId === selectedStepId ? ' selected' : '');
    el.id = 'step-item-' + stepId;
    el.dataset.stepId = stepId;

    const glyphClass = glyphCssClass(step.status);
    const glyphSymbol = step.status === 'running'
      ? '<span class="spinning">◌</span>'
      : glyphChar(step.status);
    const costStr = step.telemetry?.costUsd ? '$' + step.telemetry.costUsd.toFixed(4) : '';

    el.innerHTML =
      '<span class="step-glyph ' + glyphClass + '">' + glyphSymbol + '</span>' +
      '<span class="step-label" title="' + esc(stepId) + '">' + esc(stepId) + '</span>' +
      (costStr ? '<span class="step-cost">' + esc(costStr) + '</span>' : '');

    el.addEventListener('click', () => selectStep(runId, stepId));
    stepsList.appendChild(el);
  }

  function glyphChar(status) {
    switch (status) {
      case 'completed': return '✓';
      case 'failed':    return '✗';
      case 'skipped':   return '⊘';
      case 'paused':    return '⏸';
      default:          return '○';
    }
  }

  function glyphCssClass(status) {
    switch (status) {
      case 'running':   return 'glyph-running';
      case 'completed': return 'glyph-completed';
      case 'failed':    return 'glyph-failed';
      case 'skipped':   return 'glyph-skipped';
      case 'paused':    return 'glyph-paused';
      default:          return 'glyph-pending';
    }
  }

  function updateStepItemGlyph(stepId, status) {
    const el = document.getElementById('step-item-' + stepId);
    if (!el) return;
    const glyphEl = el.querySelector('.step-glyph');
    if (!glyphEl) return;
    glyphEl.className = 'step-glyph ' + glyphCssClass(status);
    glyphEl.innerHTML = status === 'running'
      ? '<span class="spinning">◌</span>'
      : glyphChar(status);
  }

  function updateStepItemCost(stepId, costUsd) {
    const el = document.getElementById('step-item-' + stepId);
    if (!el) return;
    let costEl = el.querySelector('.step-cost');
    if (!costEl) {
      costEl = document.createElement('span');
      costEl.className = 'step-cost';
      el.appendChild(costEl);
    }
    costEl.textContent = '$' + costUsd.toFixed(4);
  }

  // ── Column 3: Step detail ─────────────────────────────────────────────────────

  function selectStep(runId, stepId) {
    selectedStepId = stepId;
    liveThinkingPre = null;
    liveOutputPre   = null;

    // Update col 2 highlight
    stepsList.querySelectorAll('.step-item').forEach((el) => {
      el.classList.toggle('selected', el.dataset.stepId === stepId);
    });

    const entry = runs.get(runId);
    const step  = entry?.steps.get(stepId);
    if (!step) { clearDetailPanel('Select a step'); return; }

    // Update header
    detailLabel.textContent = '── ' + stepId + ' ──';
    renderTelemetryChips(step.telemetry);

    // Render full stored content
    detailBody.innerHTML = '';

    // Payload (resolved prompt) — shown as collapsible
    if (step.resolvedPrompt) {
      const d = makeDetailsBlock('Inspected Payload', 'payload-block', step.resolvedPrompt, 'payload-content', false);
      detailBody.appendChild(d);
    }

    // Thinking block — if we have content
    const thinkDetails = document.createElement('details');
    thinkDetails.className = 'thinking-block';
    const thinkSummary = document.createElement('summary');
    thinkSummary.textContent = 'Thinking';
    const thinkPre = document.createElement('pre');
    thinkPre.className = 'block-content';
    thinkPre.textContent = step.thinking;
    // Append tool badges after thinking text
    for (const t of step.tools) {
      const badge = document.createElement('div');
      badge.innerHTML = '<span class="tool-badge">' + esc(t.dir + ' ' + t.name) + '</span>';
      thinkPre.appendChild(badge.firstChild);
    }
    // Append permission banners
    for (const html of step.permHtmls) {
      const wrapper = document.createElement('div');
      wrapper.innerHTML = html;
      thinkPre.appendChild(wrapper);
    }
    thinkDetails.appendChild(thinkSummary);
    thinkDetails.appendChild(thinkPre);
    if (step.thinking || step.tools.length > 0 || step.permHtmls.length > 0) {
      thinkDetails.open = true;
    }
    detailBody.appendChild(thinkDetails);

    // Output block
    const outDetails = document.createElement('details');
    outDetails.className = 'output-block';
    const outSummary = document.createElement('summary');
    outSummary.textContent = 'Output';
    const outPre = document.createElement('pre');
    outPre.className = 'block-content';
    outPre.textContent = step.output;
    // HITL banner (if any)
    if (step.hitlHtml) {
      const wrapper = document.createElement('div');
      wrapper.innerHTML = step.hitlHtml;
      outPre.appendChild(wrapper);
    }
    outDetails.appendChild(outSummary);
    outDetails.appendChild(outPre);
    if (step.output || step.hitlHtml) outDetails.open = true;
    detailBody.appendChild(outDetails);

    // If this is the currently live step, set up live DOM references
    if (runId === liveRunId && stepId === liveCurrentStepId) {
      liveThinkingPre = thinkPre;
      liveOutputPre   = outPre;
    }
  }

  function makeDetailsBlock(summaryText, detailsClass, contentText, contentClass, openDefault) {
    const d = document.createElement('details');
    d.className = detailsClass;
    if (openDefault) d.open = true;
    const s = document.createElement('summary');
    s.textContent = summaryText;
    const pre = document.createElement('pre');
    pre.className = 'block-content ' + contentClass;
    pre.textContent = contentText;
    d.appendChild(s);
    d.appendChild(pre);
    return d;
  }

  function clearDetailPanel(labelText) {
    detailLabel.textContent = labelText;
    detailChips.innerHTML   = '';
    detailBody.innerHTML    = '';
    liveThinkingPre = null;
    liveOutputPre   = null;
  }

  function renderTelemetryChips(telemetry) {
    if (!telemetry) { detailChips.innerHTML = ''; return; }
    const parts = [];
    if (telemetry.inputTokens != null || telemetry.outputTokens != null) {
      parts.push('<span class="chip">⬆' + (telemetry.inputTokens ?? '—') + ' ⬇' + (telemetry.outputTokens ?? '—') + '</span>');
    }
    if (telemetry.costUsd != null && telemetry.costUsd > 0) {
      parts.push('<span class="chip">$' + telemetry.costUsd.toFixed(4) + '</span>');
    }
    if (telemetry.latencyMs != null) {
      const s = telemetry.latencyMs >= 1000
        ? (telemetry.latencyMs / 1000).toFixed(1) + 's'
        : telemetry.latencyMs + 'ms';
      parts.push('<span class="chip">' + s + '</span>');
    }
    detailChips.innerHTML = parts.join('');
  }

  // ── HITL / Permission helpers ──────────────────────────────────────────────────

  function submitHitl(stepId, approved) {
    const textEl = document.getElementById('hitl-text-' + stepId);
    const text   = textEl ? textEl.value : '';
    const banner = document.getElementById('hitl-banner-' + stepId);
    if (banner) {
      banner.innerHTML = approved
        ? '<span style="color:#4ec994">✓ Approved' + (text ? ': ' + esc(text) : '') + '</span>'
        : '<span style="color:#f48771">✗ Rejected' + (text ? ': ' + escRaw(text) : '') + '</span>';
    }
    // Store final state
    const entry = runs.get(liveRunId);
    if (entry) {
      const step = entry.steps.get(stepId);
      if (step) {
        step.hitlHtml = banner ? banner.outerHTML : null;
      }
    }
    vscode.postMessage({ type: 'hitl_response', stepId, text: approved ? text : null });
  }

  function escRaw(s) { return esc(s); }

  function submitPermission(permId, stepId, allowed) {
    const banner = document.getElementById(permId);
    if (banner) {
      const btnRow = banner.querySelector('.btn-row');
      if (btnRow) btnRow.remove();
      const result = document.createElement('div');
      result.style.marginTop = '4px';
      result.style.color = allowed ? '#4ec994' : '#f48771';
      result.textContent = allowed ? '✓ Allowed' : '✗ Denied';
      banner.appendChild(result);
    }
    vscode.postMessage({ type: 'permission_response', allowed });
  }

  // ── Main message handler ──────────────────────────────────────────────────────

  window.addEventListener('message', (event) => {
    const msg = event.data;

    switch (msg.cmd) {

      // ─── Reset ───────────────────────────────────────────────────────────────
      case 'init': {
        runs.clear();
        selectedRunId     = null;
        selectedStepId    = null;
        liveRunId         = null;
        liveCurrentStepId = null;
        liveThinkingPre   = null;
        liveOutputPre     = null;
        renderHistoryList();
        stepsList.innerHTML  = '<div class="empty-state">Select a run</div>';
        clearDetailPanel('Select a step');
        costDisplay.textContent = 'Cost: —';
        stepDisplay.textContent = 'Steps: —';
        break;
      }

      // ─── Column 1: history ────────────────────────────────────────────────────
      case 'historyUpdated': {
        for (const s of (msg.runs || [])) {
          if (s.runId === liveRunId) continue; // don't overwrite live run entry
          if (!runs.has(s.runId)) {
            runs.set(s.runId, { summary: s, steps: new Map() });
          } else {
            runs.get(s.runId).summary = s;
          }
        }
        renderHistoryList();
        break;
      }

      // ─── Live run started ─────────────────────────────────────────────────────
      case 'liveRunStarted': {
        liveRunId = msg.runId;
        liveCurrentStepId = null;
        liveThinkingPre   = null;
        liveOutputPre     = null;

        const summary = {
          runId: msg.runId,
          timestamp: Date.now(),
          pipelineSource: msg.pipelineSource || 'unknown',
          outcome: 'running',
          totalCostUsd: 0,
          invocationPrompt: '',
          isLive: true,
        };
        runs.set(msg.runId, { summary, steps: new Map() });
        renderHistoryList();

        // Auto-select this run
        selectRun(msg.runId, false);
        costDisplay.textContent = 'Cost: —';
        stepDisplay.textContent = 'Steps: 0/' + (msg.totalSteps || '?');
        break;
      }

      // ─── Steps ────────────────────────────────────────────────────────────────
      case 'stepStarted': {
        const entry = runs.get(liveRunId);
        if (!entry) break;

        liveCurrentStepId = msg.stepId;
        liveThinkingPre   = null;
        liveOutputPre     = null;

        const step = makeStepEntry();
        step.status = 'running';
        step.resolvedPrompt = msg.resolvedPrompt || null;
        entry.steps.set(msg.stepId, step);

        stepDisplay.textContent = 'Steps: ' + (msg.stepIndex + 1) + '/' + msg.totalSteps;

        if (selectedRunId === liveRunId) {
          // Add to col 2 and auto-select
          if (stepsList.querySelector('.empty-state')) stepsList.innerHTML = '';
          renderStepItem(msg.stepId, step, liveRunId);
          selectStep(liveRunId, msg.stepId);
        }
        break;
      }

      case 'stepCompleted': {
        const entry = runs.get(liveRunId);
        if (!entry) break;
        const step = entry.steps.get(msg.stepId);
        if (step) {
          step.status = 'completed';
          step.telemetry = {
            inputTokens: msg.inputTokens,
            outputTokens: msg.outputTokens,
            costUsd: msg.costUsd,
            latencyMs: msg.latencyMs,
          };
        }
        if (entry.summary) {
          entry.summary.totalCostUsd = msg.totalCost || 0;
        }
        updateStepItemGlyph(msg.stepId, 'completed');
        if (msg.costUsd) updateStepItemCost(msg.stepId, msg.costUsd);
        costDisplay.textContent = 'Cost: $' + ((msg.totalCost || 0).toFixed(4));
        if (selectedRunId === liveRunId && selectedStepId === msg.stepId) {
          renderTelemetryChips(step?.telemetry || null);
        }
        // Clear live refs since step is done
        if (liveCurrentStepId === msg.stepId) {
          liveThinkingPre = null;
          liveOutputPre   = null;
        }
        break;
      }

      case 'stepFailed': {
        const entry = runs.get(liveRunId);
        if (!entry) break;
        const step = entry.steps.get(msg.stepId);
        if (step) {
          step.status = 'failed';
          step.output += (step.output ? '\\n' : '') + '✗ ' + (msg.error || '');
        }
        updateStepItemGlyph(msg.stepId, 'failed');
        if (selectedRunId === liveRunId && selectedStepId === msg.stepId && liveOutputPre) {
          liveOutputPre.insertAdjacentHTML('beforeend',
            '<span class="error-text">✗ ' + esc(msg.error || '') + '</span>');
          const outBlock = liveOutputPre.parentElement;
          if (outBlock) outBlock.open = true;
        }
        liveThinkingPre = null;
        liveOutputPre   = null;
        break;
      }

      case 'stepSkipped': {
        const entry = runs.get(liveRunId);
        if (!entry) break;
        const step = entry.steps.get(msg.stepId);
        if (step) step.status = 'skipped';
        updateStepItemGlyph(msg.stepId, 'skipped');
        break;
      }

      // ─── Streaming ────────────────────────────────────────────────────────────
      case 'streamDelta': {
        const entry = runs.get(liveRunId);
        if (!entry) break;
        const step = entry.steps.get(liveCurrentStepId);
        if (step) step.output += msg.text;
        if (liveOutputPre) {
          liveOutputPre.insertAdjacentText('beforeend', msg.text);
          detailBody.scrollTop = detailBody.scrollHeight;
          const outBlock = liveOutputPre.parentElement;
          if (outBlock && !outBlock.open) outBlock.open = true;
        }
        break;
      }

      case 'thinking': {
        const entry = runs.get(liveRunId);
        if (!entry) break;
        const step = entry.steps.get(liveCurrentStepId);
        if (step) step.thinking += msg.text;
        if (liveThinkingPre) {
          liveThinkingPre.insertAdjacentText('beforeend', msg.text);
          const thinkBlock = liveThinkingPre.parentElement;
          if (thinkBlock && !thinkBlock.open) thinkBlock.open = true;
        }
        break;
      }

      case 'toolUse': {
        const entry = runs.get(liveRunId);
        if (!entry) break;
        const step = entry.steps.get(liveCurrentStepId);
        if (step) step.tools.push({ dir: '→', name: msg.toolName });
        if (liveThinkingPre) {
          liveThinkingPre.insertAdjacentHTML('beforeend',
            '<div><span class="tool-badge">→ ' + esc(msg.toolName) + '</span></div>');
        }
        break;
      }

      case 'toolResult': {
        const entry = runs.get(liveRunId);
        if (!entry) break;
        const step = entry.steps.get(liveCurrentStepId);
        if (step) step.tools.push({ dir: '←', name: msg.toolName });
        if (liveThinkingPre) {
          liveThinkingPre.insertAdjacentHTML('beforeend',
            '<div><span class="tool-badge">← ' + esc(msg.toolName) + '</span></div>');
        }
        break;
      }

      case 'hitlGate': {
        const entry = runs.get(liveRunId);
        if (!entry) break;
        const step = entry.steps.get(msg.stepId);
        if (!step) break;
        step.status = 'paused';
        const bannerId   = 'hitl-banner-' + msg.stepId;
        const hitlHtml =
          '<div class="hitl-banner" id="' + bannerId + '">' +
          '<strong>⏸ HITL Gate</strong> — step paused for human review.' +
          '<textarea id="hitl-text-' + msg.stepId + '" placeholder="Optional guidance text..."></textarea>' +
          '<div class="btn-row">' +
          '<button class="btn-approve" onclick="submitHitl(\\'' + msg.stepId + '\\', true)">Approve</button>' +
          '<button class="btn-reject" onclick="submitHitl(\\'' + msg.stepId + '\\', false)">Reject</button>' +
          '</div></div>';
        step.hitlHtml = hitlHtml;
        updateStepItemGlyph(msg.stepId, 'paused');
        if (selectedRunId === liveRunId && selectedStepId === msg.stepId && liveOutputPre) {
          liveOutputPre.insertAdjacentHTML('beforeend', hitlHtml);
          const outBlock = liveOutputPre.parentElement;
          if (outBlock) outBlock.open = true;
        }
        break;
      }

      case 'permissionReq': {
        const entry = runs.get(liveRunId);
        if (!entry) break;
        const step = entry.steps.get(liveCurrentStepId);
        if (!step) break;
        const permId = 'perm-' + Date.now();
        const permHtml =
          '<div class="permission-banner" id="' + permId + '">' +
          '🔐 <strong>' + esc(msg.displayName) + '</strong>: ' + esc(msg.displayDetail) +
          '<div class="btn-row">' +
          '<button class="btn-allow" onclick="submitPermission(\\'' + permId + '\\',\\'' + liveCurrentStepId + '\\',true)">Allow</button>' +
          '<button class="btn-deny"  onclick="submitPermission(\\'' + permId + '\\',\\'' + liveCurrentStepId + '\\',false)">Deny</button>' +
          '</div></div>';
        step.permHtmls.push(permHtml);
        if (liveThinkingPre) {
          liveThinkingPre.insertAdjacentHTML('beforeend', permHtml);
          const thinkBlock = liveThinkingPre.parentElement;
          if (thinkBlock && !thinkBlock.open) thinkBlock.open = true;
        }
        break;
      }

      // ─── Cost + completion ────────────────────────────────────────────────────
      case 'costUpdate':
        costDisplay.textContent = 'Cost: $' + ((msg.totalCost || 0).toFixed(4));
        break;

      case 'pipelineCompleted': {
        const entry = runs.get(liveRunId);
        if (entry) {
          entry.summary.outcome  = 'completed';
          entry.summary.isLive   = false;
          entry.summary.totalCostUsd = msg.totalCost || 0;
        }
        costDisplay.textContent = 'Cost: $' + ((msg.totalCost || 0).toFixed(4));
        renderHistoryList();
        break;
      }

      case 'pipelineError': {
        const entry = runs.get(liveRunId);
        if (entry) {
          entry.summary.outcome = 'failed';
          entry.summary.isLive  = false;
        }
        renderHistoryList();
        break;
      }

      // ─── Historical review data ────────────────────────────────────────────────
      case 'reviewData': {
        // Populate or refresh an entry's steps from historical TurnEntry records
        let entry = runs.get(msg.runId);
        if (!entry) {
          const summary = {
            runId: msg.runId,
            timestamp: msg.timestamp || 0,
            pipelineSource: msg.pipelineSource || 'unknown',
            outcome: msg.outcome || 'unknown',
            totalCostUsd: msg.totalCostUsd || 0,
            invocationPrompt: msg.invocationPrompt || '',
            isLive: false,
          };
          entry = { summary, steps: new Map() };
          runs.set(msg.runId, entry);
        }

        // Rebuild steps from TurnEntry records
        entry.steps.clear();
        for (const te of (msg.steps || [])) {
          const step = makeStepEntry();
          step.status       = 'completed';
          step.output       = te.response || '';
          step.thinking     = te.thinking || '';
          step.resolvedPrompt = te.prompt || null;
          step.telemetry    = {
            inputTokens:  te.input_tokens,
            outputTokens: te.output_tokens,
            costUsd:      te.cost_usd,
            latencyMs:    null,
          };
          entry.steps.set(te.step_id, step);
        }

        // Select this run and show its first step
        selectRun(msg.runId, false);
        renderHistoryList();

        // Update cost bar with this run's totals
        costDisplay.textContent = 'Cost: $' + ((msg.totalCostUsd || 0).toFixed(4));
        stepDisplay.textContent = 'Steps: ' + entry.steps.size;
        break;
      }

    }
  });

  // Signal ready
  vscode.postMessage({ type: 'ready' });
</script>
</body>
</html>`;
}
