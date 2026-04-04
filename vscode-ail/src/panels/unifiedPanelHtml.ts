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
 *   stepFilesChanged Update a step's file diff list (from git diff).
 *
 * Webview → host messages:
 *   ready            Webview script has loaded and is ready for messages.
 *   selectRun        User clicked a historical run in column 1.
 *   hitl_response    User approved/rejected a HITL gate.
 *   permission_response User allowed/denied a permission request.
 *   viewDiff         User clicked "View Diff" for a file in a step.
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

  #col-history { width: 240px; border-right: 1px solid var(--vscode-panel-border); }
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
  .run-pipeline {
    font-size: 10px;
    color: var(--vscode-descriptionForeground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin-top: 1px;
  }
  .run-item.selected .run-pipeline { color: inherit; opacity: 0.6; }
  .run-cost { font-size: 10px; color: var(--vscode-descriptionForeground); margin-left: auto; }
  .run-item.selected .run-cost { color: inherit; opacity: 0.75; }
  .run-prompt-primary {
    font-size: 12px;
    font-weight: 600;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    margin-top: 2px;
    line-height: 1.35;
  }
  .run-no-prompt {
    font-size: 11px;
    color: var(--vscode-descriptionForeground);
    font-style: italic;
    margin-top: 2px;
  }
  .run-item.selected .run-no-prompt { color: inherit; opacity: 0.6; }

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

  /* ── Result badges (col 2) ── */
  .result-badge { padding: 1px 4px; border-radius: 3px; font-size: 9px; font-weight: bold; text-transform: uppercase; }
  .result-completed { background: rgba(78, 201, 148, 0.2); color: #4ec994; }
  .result-skipped   { background: rgba(229, 192, 123, 0.2); color: #e5c07b; }
  .result-break     { background: rgba(229, 192, 123, 0.2); color: #e5c07b; }
  .result-abort     { background: rgba(244, 135, 113, 0.2); color: #f48771; }
  .result-error     { background: rgba(244, 135, 113, 0.2); color: #f48771; }

  /* ── Step meta row (latency + tokens + badge) ── */
  .step-meta { display: flex; gap: 4px; align-items: center; font-size: 10px; margin-left: auto; color: var(--vscode-descriptionForeground); }
  .step-item.selected .step-meta { color: inherit; opacity: 0.8; }
  .step-meta-tokens { font-size: 10px; }

  /* ── Collapsible details block in col 3 ── */
  details.meta-block {
    margin: 4px 0;
    border-left: 2px solid var(--vscode-panel-border);
    padding-left: 8px;
  }
  .meta-table { font-size: 11px; border-collapse: collapse; width: 100%; }
  .meta-table td { padding: 2px 6px 2px 0; vertical-align: top; }
  .meta-table td:first-child { color: var(--vscode-descriptionForeground); white-space: nowrap; }

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

  /* ── File diff section (col 3) ── */
  .diff-file-item { display: flex; align-items: center; gap: 8px; padding: 2px 0; font-size: 12px; }
  .diff-file-path { flex: 1; font-family: monospace; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .diff-change-type { font-size: 10px; padding: 1px 4px; border-radius: 3px; flex-shrink: 0; }
  .diff-added    { background: rgba(78, 201, 148, 0.2); color: #4ec994; }
  .diff-modified { background: rgba(79, 193, 255, 0.2); color: #4fc3ff; }
  .diff-deleted  { background: rgba(244, 135, 113, 0.2); color: #f48771; }
  .diff-link { cursor: pointer; color: var(--vscode-textLink-foreground); text-decoration: underline; font-size: 11px; flex-shrink: 0; }
  .diff-count-badge { font-size: 10px; padding: 1px 4px; border-radius: 3px; background: rgba(79, 193, 255, 0.2); color: #4fc3ff; margin-left: auto; }

  /* ── Step sections (unified log, col 3) ── */
  .step-section {
    border-top: 1px solid var(--vscode-panel-border);
    padding-bottom: 8px;
  }
  .step-section:first-child { border-top: none; }
  .step-section-header {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--vscode-editor-background);
    padding: 6px 14px 4px;
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    border-bottom: 1px solid var(--vscode-panel-border);
  }
  .step-section-header.active { background: var(--vscode-editor-selectionBackground, rgba(0,122,204,0.15)); }
  .section-step-label { font-weight: bold; font-size: 12px; }
  .section-glyph { width: 16px; text-align: center; font-size: 13px; }
  .step-section .payload-block,
  .step-section .thinking-block,
  .step-section .output-block,
  .step-section .meta-block,
  .step-section .diff-section {
    margin-left: 14px;
    margin-right: 14px;
  }

  /* ── Expandable tool badges (Issue 6b) ── */
  details.tool-detail {
    display: inline-block;
    margin: 2px 0;
  }
  details.tool-detail summary {
    list-style: none;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
  }
  details.tool-detail summary::before { content: none; }
  details.tool-detail[open] .tool-badge { border-radius: 3px 3px 0 0; }
  .tool-content {
    display: block;
    background: var(--vscode-textCodeBlock-background, rgba(0,0,0,0.2));
    border: 1px solid var(--vscode-panel-border);
    border-top: none;
    border-radius: 0 0 3px 3px;
    padding: 4px 8px;
    font-family: var(--vscode-editor-font-family, monospace);
    font-size: 11px;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 200px;
    overflow-y: auto;
    margin-bottom: 2px;
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
  // StepEntry: { status, thinking, output, tools, hitlHtml, permHtmls, telemetry, resultCode, rawEventData, files, resolvedPrompt }
  const runs = new Map();

  let selectedRunId    = null;  // shown in col 2+3
  let selectedStepId   = null;  // highlighted in col 2, scrolled to in col 3
  let liveRunId        = null;  // the run currently receiving events
  let liveCurrentStepId = null; // the step currently streaming

  // Unified log: tracks which run's sections are currently rendered in col 3
  let renderedRunId = null;
  // Map<stepId, { thinkPre, outPre }> — cached DOM refs per section
  const sectionRefs = new Map();

  // Live DOM references for efficient streaming (set on stepStarted, cleared on stepCompleted/Failed)
  let liveThinkingPre  = null;
  let liveOutputPre    = null;

  // Auto-scroll state: paused when the user scrolls up; reset on each new step start.
  let autoScrollPaused = false;
  detailBody.addEventListener('scroll', () => {
    const atBottom = detailBody.scrollHeight - detailBody.scrollTop - detailBody.clientHeight < 40;
    autoScrollPaused = !atBottom;
  });

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
    return { status: 'pending', thinking: '', output: '', tools: [], hitlHtml: null, permHtmls: [], telemetry: null, resultCode: null, rawEventData: null, files: [], resolvedPrompt: null };
  }

  // Render a tool badge. If detail content available, wraps in <details> for expansion.
  function renderToolBadgeHtml(t) {
    const label = esc(t.dir + ' ' + t.name);
    const badge = '<span class="tool-badge">' + label + '</span>';
    if (t.detail) {
      return '<details class="tool-detail"><summary>' + badge + '</summary><pre class="tool-content">' + esc(t.detail) + '</pre></details>';
    }
    return '<div>' + badge + '</div>';
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
      const label = pipelineLabel(s.pipelineSource);

      // Row 1: glyph + time + cost
      // Row 2: prompt (primary)
      // Row 3: pipeline name (secondary, omit if unknown)
      el.innerHTML =
        '<div class="run-meta">' +
          '<span class="run-glyph">' + outcomeGlyph(s.outcome, s.isLive) + '</span>' +
          '<span class="run-time">' + relTime(s.timestamp) + '</span>' +
          (costStr ? '<span class="run-cost">' + esc(costStr) + '</span>' : '') +
        '</div>' +
        (s.invocationPrompt
          ? '<div class="run-prompt-primary">' + esc(s.invocationPrompt) + '</div>'
          : '<div class="run-no-prompt">No prompt</div>') +
        (label && label !== 'unknown'
          ? '<div class="run-pipeline">' + esc(label) + '</div>'
          : '');

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

  function resultBadgeHtml(resultCode) {
    if (!resultCode) return '';
    const cls = resultCode === 'completed' ? 'result-completed'
      : resultCode === 'skipped' ? 'result-skipped'
      : resultCode === 'break' ? 'result-break'
      : resultCode === 'abort_pipeline' ? 'result-abort'
      : 'result-error';
    const label = resultCode === 'abort_pipeline' ? 'abort' : resultCode;
    return '<span class="result-badge ' + cls + '">' + esc(label) + '</span>';
  }

  function formatLatency(latencyMs) {
    if (latencyMs == null) return null;
    return latencyMs >= 1000
      ? (latencyMs / 1000).toFixed(1) + 's'
      : latencyMs + 'ms';
  }

  function stepMetaHtml(step) {
    const parts = [];
    const lat = formatLatency(step.telemetry?.latencyMs ?? null);
    if (lat) parts.push('<span>' + esc(lat) + '</span>');
    if (step.telemetry?.inputTokens != null || step.telemetry?.outputTokens != null) {
      const inT = step.telemetry.inputTokens != null ? step.telemetry.inputTokens : '—';
      const outT = step.telemetry.outputTokens != null ? step.telemetry.outputTokens : '—';
      parts.push('<span class="step-meta-tokens">↑' + inT + ' ↓' + outT + '</span>');
    }
    if (step.resultCode) parts.push(resultBadgeHtml(step.resultCode));
    if (step.files?.length) parts.push('<span class="diff-count-badge">📄' + step.files.length + '</span>');
    if (!parts.length) return '';
    return '<span class="step-meta">' + parts.join('') + '</span>';
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

    const diffBadge = step.files && step.files.length > 0
      ? '<span class="diff-count-badge">📄 ' + step.files.length + '</span>'
      : '';

    el.innerHTML =
      '<span class="step-glyph ' + glyphClass + '">' + glyphSymbol + '</span>' +
      '<span class="step-label" title="' + esc(stepId) + '">' + esc(stepId) + '</span>' +
      stepMetaHtml(step);

    el.addEventListener('click', () => selectStep(runId, stepId));
    stepsList.appendChild(el);
  }

  function updateStepItemMeta(stepId, step) {
    const el = document.getElementById('step-item-' + stepId);
    if (!el) return;
    // Remove old meta span
    const old = el.querySelector('.step-meta');
    if (old) old.remove();
    // Remove old step-cost span (legacy — replaced by step-meta)
    const oldCost = el.querySelector('.step-cost');
    if (oldCost) oldCost.remove();
    const html = stepMetaHtml(step);
    if (html) el.insertAdjacentHTML('beforeend', html);
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


  function updateStepItemDiffBadge(stepId, fileCount) {
    const el = document.getElementById('step-item-' + stepId);
    if (!el) return;
    let badge = el.querySelector('.diff-count-badge');
    if (fileCount > 0) {
      if (!badge) {
        badge = document.createElement('span');
        badge.className = 'diff-count-badge';
        el.appendChild(badge);
      }
      badge.textContent = '📄 ' + fileCount;
    } else if (badge) {
      badge.remove();
    }
  }

  // ── Column 3: Unified scrollable log ─────────────────────────────────────────

  /**
   * Build a full section DOM element for a step. Does not append to detailBody.
   */
  function createStepSection(stepId, step) {
    const section = document.createElement('div');
    section.className = 'step-section';
    section.id = 'section-' + stepId;

    // Sticky header
    const header = document.createElement('div');
    header.className = 'step-section-header';
    header.id = 'section-header-' + stepId;
    const glyphClass = glyphCssClass(step.status);
    const glyphSymbol = step.status === 'running'
      ? '<span class="spinning">◌</span>'
      : glyphChar(step.status);
    header.innerHTML =
      '<span class="section-glyph ' + glyphClass + '">' + glyphSymbol + '</span>' +
      '<strong class="section-step-label">' + esc(stepId) + '</strong>' +
      '<span class="section-telemetry telemetry-chips"></span>';
    section.appendChild(header);

    // Payload block
    if (step.resolvedPrompt) {
      const d = makeDetailsBlock('Inspected Payload', 'payload-block', step.resolvedPrompt, 'payload-content', false);
      section.appendChild(d);
    }

    // Thinking block
    const thinkDetails = document.createElement('details');
    thinkDetails.className = 'thinking-block';
    const thinkSummary = document.createElement('summary');
    thinkSummary.textContent = 'Thinking';
    const thinkPre = document.createElement('pre');
    thinkPre.className = 'block-content thinking-content';
    thinkPre.textContent = step.thinking;
    for (const t of step.tools) {
      thinkPre.insertAdjacentHTML('beforeend', renderToolBadgeHtml(t));
    }
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
    section.appendChild(thinkDetails);

    // Output block
    const outDetails = document.createElement('details');
    outDetails.className = 'output-block';
    const outSummary = document.createElement('summary');
    outSummary.textContent = 'Output';
    const outPre = document.createElement('pre');
    outPre.className = 'block-content output-content';
    outPre.textContent = step.output;
    if (step.hitlHtml) {
      const wrapper = document.createElement('div');
      wrapper.innerHTML = step.hitlHtml;
      outPre.appendChild(wrapper);
    }
    outDetails.appendChild(outSummary);
    outDetails.appendChild(outPre);
    if (step.output || step.hitlHtml) outDetails.open = true;
    section.appendChild(outDetails);

    // Meta block
    const metaDetails = document.createElement('details');
    metaDetails.className = 'meta-block';
    const metaSummary = document.createElement('summary');
    metaSummary.textContent = 'Details';
    metaDetails.appendChild(metaSummary);
    const table = document.createElement('table');
    table.className = 'meta-table';
    function addRow(lbl, val, key) {
      const tr = document.createElement('tr');
      const td1 = document.createElement('td'); td1.textContent = lbl;
      const td2 = document.createElement('td');
      if (key) td2.id = 'meta-' + stepId + '-' + key;
      td2.textContent = val;
      tr.appendChild(td1); tr.appendChild(td2); table.appendChild(tr);
    }
    addRow('Result code', step.resultCode ?? '—', 'result-code');
    const lat = formatLatency(step.telemetry?.latencyMs ?? null);
    addRow('Latency', lat ?? '—', 'latency');
    addRow('Input tokens', step.telemetry?.inputTokens != null ? String(step.telemetry.inputTokens) : '—', 'input-tokens');
    addRow('Output tokens', step.telemetry?.outputTokens != null ? String(step.telemetry.outputTokens) : '—', 'output-tokens');
    addRow('Cost', step.telemetry?.costUsd != null && step.telemetry.costUsd > 0 ? '$' + step.telemetry.costUsd.toFixed(4) : '—', 'cost');
    addRow('Model', '—', 'model');
    metaDetails.appendChild(table);
    if (step.rawEventData) {
      const rawPre = document.createElement('pre');
      rawPre.className = 'block-content payload-content';
      rawPre.textContent = typeof step.rawEventData === 'string'
        ? step.rawEventData : JSON.stringify(step.rawEventData, null, 2);
      metaDetails.appendChild(rawPre);
    }
    section.appendChild(metaDetails);

    // Diff section
    const diffEl = renderDiffSection(stepId, step.files);
    if (diffEl) section.appendChild(diffEl);

    return section;
  }

  /**
   * Append a new section to detailBody and cache its streaming refs.
   */
  function appendStepSection(stepId, step) {
    const section = createStepSection(stepId, step);
    detailBody.appendChild(section);
    const thinkPre = section.querySelector('.thinking-content');
    const outPre   = section.querySelector('.output-content');
    sectionRefs.set(stepId, { thinkPre, outPre });
  }

  /**
   * Ensure the unified log sections for runId are rendered in detailBody.
   * Idempotent: does nothing if already rendered for this run.
   */
  function ensureRunSections(runId) {
    if (renderedRunId === runId) return;
    const entry = runs.get(runId);
    if (!entry) return;

    detailBody.innerHTML = '';
    sectionRefs.clear();
    renderedRunId = runId;

    for (const [stepId, step] of entry.steps) {
      appendStepSection(stepId, step);
    }
  }

  /**
   * Update the sticky section header glyph for a step.
   */
  function updateSectionGlyph(stepId, status) {
    const header = document.getElementById('section-header-' + stepId);
    if (!header) return;
    const glyphEl = header.querySelector('.section-glyph');
    if (!glyphEl) return;
    glyphEl.className = 'section-glyph ' + glyphCssClass(status);
    glyphEl.innerHTML = status === 'running'
      ? '<span class="spinning">◌</span>'
      : glyphChar(status);
  }

  /**
   * Update the DETAILS meta-table rows for a step.
   */
  function updateSectionMeta(stepId, resultCode, telemetry) {
    const set = (key, val) => {
      const el = document.getElementById('meta-' + stepId + '-' + key);
      if (el) el.textContent = val;
    };
    if (resultCode != null) set('result-code', resultCode);
    if (telemetry) {
      const lat = formatLatency(telemetry.latencyMs ?? null);
      if (lat) set('latency', lat);
      if (telemetry.inputTokens != null)  set('input-tokens', String(telemetry.inputTokens));
      if (telemetry.outputTokens != null) set('output-tokens', String(telemetry.outputTokens));
      if (telemetry.costUsd != null && telemetry.costUsd > 0)
        set('cost', '$' + telemetry.costUsd.toFixed(4));
      if (telemetry.model) set('model', telemetry.model);
    }
  }

  /**
   * Update the telemetry chips in the section header.
   */
  function updateSectionTelemetry(stepId, telemetry) {
    const header = document.getElementById('section-header-' + stepId);
    if (!header || !telemetry) return;
    const chips = header.querySelector('.section-telemetry');
    if (!chips) return;
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
    chips.innerHTML = parts.join('');
  }

  /**
   * Select a step: highlight in col 2, scroll to its section in col 3.
   */
  function selectStep(runId, stepId) {
    selectedStepId = stepId;

    // Update col 2 highlight
    stepsList.querySelectorAll('.step-item').forEach((el) => {
      el.classList.toggle('selected', el.dataset.stepId === stepId);
    });

    const entry = runs.get(runId);
    const step  = entry?.steps.get(stepId);
    if (!step) { clearDetailPanel('Select a step'); return; }

    // Update global header to show focused step
    detailLabel.textContent = '── ' + stepId + ' ──';
    renderTelemetryChips(step.telemetry);

    // Ensure unified log sections exist
    ensureRunSections(runId);

    // Highlight the active section header
    detailBody.querySelectorAll('.step-section-header').forEach((el) => {
      el.classList.toggle('active', el.id === 'section-header-' + stepId);
    });

    // Scroll to section (guard: scrollIntoView may not be available in all environments)
    const section = document.getElementById('section-' + stepId);
    if (section && typeof section.scrollIntoView === 'function') {
      try { section.scrollIntoView({ behavior: 'smooth', block: 'start' }); } catch { /* no-op */ }
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
    renderedRunId   = null;
    sectionRefs.clear();
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

  function viewDiff(stepId, filePath) {
    vscode.postMessage({ type: 'viewDiff', stepId, filePath });
  }

  function renderDiffSection(stepId, files) {
    if (!files || files.length === 0) return null;

    const details = document.createElement('details');
    details.className = 'payload-block diff-section';
    details.open = true;

    const summary = document.createElement('summary');
    summary.textContent = 'Files Changed (' + files.length + ')';
    details.appendChild(summary);

    for (const f of files) {
      const item = document.createElement('div');
      item.className = 'diff-file-item';

      const typeClass = 'diff-' + (f.changeType || 'modified');
      const typeLabel = f.changeType === 'added' ? 'A' : f.changeType === 'deleted' ? 'D' : 'M';

      item.innerHTML =
        '<span class="diff-change-type ' + typeClass + '">' + typeLabel + '</span>' +
        '<span class="diff-file-path" title="' + esc(f.relativePath) + '">' + esc(f.relativePath) + '</span>' +
        '<span class="diff-link" onclick="viewDiff(' + JSON.stringify(stepId) + ',' + JSON.stringify(f.relativePath) + ')">View Diff</span>';

      details.appendChild(item);
    }

    return details;
  }

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
        renderedRunId     = null;
        sectionRefs.clear();
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
        renderedRunId     = null;
        sectionRefs.clear();

        const summary = {
          runId: msg.runId,
          timestamp: Date.now(),
          pipelineSource: msg.pipelineSource || 'unknown',
          outcome: 'running',
          totalCostUsd: 0,
          invocationPrompt: msg.invocationPrompt || '',
          isLive: true,
        };
        const entry = { summary, steps: new Map() };

        // Pre-populate steps from manifest (Issue 4)
        if (msg.stepManifest && msg.stepManifest.length > 0) {
          for (const s of msg.stepManifest) {
            entry.steps.set(s.id, makeStepEntry());
          }
        }

        runs.set(msg.runId, entry);
        renderHistoryList();

        // Auto-select this run (renders col 2 + col 3 sections)
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
        autoScrollPaused  = false; // Resume auto-scroll for new step

        let step = entry.steps.get(msg.stepId);
        if (step) {
          // Step was pre-populated from manifest — update in place
          step.status = 'running';
          step.resolvedPrompt = msg.resolvedPrompt || null;
          updateStepItemGlyph(msg.stepId, 'running');
          updateSectionGlyph(msg.stepId, 'running');
          // Add payload block to section if we now have a prompt
          if (step.resolvedPrompt && renderedRunId === liveRunId) {
            const section = document.getElementById('section-' + msg.stepId);
            const header = section?.querySelector('.step-section-header');
            const existingPayload = section?.querySelector('.payload-block');
            if (section && header && !existingPayload) {
              const d = makeDetailsBlock('Inspected Payload', 'payload-block', step.resolvedPrompt, 'payload-content', false);
              header.insertAdjacentElement('afterend', d);
            }
          }
        } else {
          // New step not in manifest — create entry
          step = makeStepEntry();
          step.status = 'running';
          step.resolvedPrompt = msg.resolvedPrompt || null;
          entry.steps.set(msg.stepId, step);

          if (selectedRunId === liveRunId) {
            // Add to col 2
            if (stepsList.querySelector('.empty-state')) stepsList.innerHTML = '';
            renderStepItem(msg.stepId, step, liveRunId);
          }
          // Add section to col 3 if sections are already rendered for this run
          if (renderedRunId === liveRunId) {
            appendStepSection(msg.stepId, step);
          }
        }

        stepDisplay.textContent = 'Steps: ' + (msg.stepIndex + 1) + '/' + msg.totalSteps;

        if (selectedRunId === liveRunId) {
          // selectStep calls ensureRunSections which populates sectionRefs
          selectStep(liveRunId, msg.stepId);
        }

        // Set live streaming refs AFTER selectStep (ensureRunSections may have just created them)
        const refs = sectionRefs.get(msg.stepId);
        liveThinkingPre = refs?.thinkPre || null;
        liveOutputPre   = refs?.outPre || null;
        break;
      }

      case 'stepCompleted': {
        const entry = runs.get(liveRunId);
        if (!entry) break;
        const step = entry.steps.get(msg.stepId);
        if (step) {
          step.status = 'completed';
          step.resultCode = 'completed';
          step.telemetry = {
            inputTokens: msg.inputTokens,
            outputTokens: msg.outputTokens,
            costUsd: msg.costUsd,
            latencyMs: msg.latencyMs,
            model: msg.model ?? null,
          };
        }
        if (entry.summary) {
          entry.summary.totalCostUsd = msg.totalCost || 0;
        }
        updateStepItemGlyph(msg.stepId, 'completed');
        updateStepItemMeta(msg.stepId, step || makeStepEntry());
        updateSectionGlyph(msg.stepId, 'completed');
        updateSectionMeta(msg.stepId, 'completed', step?.telemetry || null);
        updateSectionTelemetry(msg.stepId, step?.telemetry || null);
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
          step.resultCode = 'error';
          step.output += (step.output ? '\\n' : '') + '✗ ' + (msg.error || '');
        }
        updateStepItemGlyph(msg.stepId, 'failed');
        updateStepItemMeta(msg.stepId, step || makeStepEntry());
        updateSectionGlyph(msg.stepId, 'failed');
        updateSectionMeta(msg.stepId, 'error', null);
        if (liveOutputPre) {
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
        if (step) {
          step.status = 'skipped';
          step.resultCode = 'skipped';
        }
        updateStepItemGlyph(msg.stepId, 'skipped');
        updateStepItemMeta(msg.stepId, step || makeStepEntry());
        updateSectionGlyph(msg.stepId, 'skipped');
        updateSectionMeta(msg.stepId, 'skipped', null);
        break;
      }

      // ─── Result code override (break / abort_pipeline from pipeline events) ──────
      case 'stepResultCode': {
        const entry = runs.get(liveRunId);
        if (!entry) break;
        const step = entry.steps.get(msg.stepId);
        if (step) {
          step.resultCode = msg.resultCode;
        }
        updateStepItemMeta(msg.stepId, step || makeStepEntry());
        break;
      }

      case 'stepFilesChanged': {
        const entry = runs.get(liveRunId);
        if (!entry) break;
        const step = entry.steps.get(msg.stepId);
        if (step) {
          step.files = msg.files || [];
          updateStepItemDiffBadge(msg.stepId, step.files.length);
          // Update diff section inside this step's section div
          const section = document.getElementById('section-' + msg.stepId);
          if (section) {
            const existingDiff = section.querySelector('details.diff-section');
            if (existingDiff) existingDiff.remove();
            const diffEl = renderDiffSection(msg.stepId, step.files);
            if (diffEl) section.appendChild(diffEl);
          }
        }
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
          if (!autoScrollPaused) {
            detailBody.scrollTop = detailBody.scrollHeight;
          }
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
        const detail = msg.input != null ? JSON.stringify(msg.input, null, 2) : null;
        const t = { dir: '→', name: msg.toolName, detail };
        if (step) step.tools.push(t);
        if (liveThinkingPre) {
          liveThinkingPre.insertAdjacentHTML('beforeend', renderToolBadgeHtml(t));
        }
        break;
      }

      case 'toolResult': {
        const entry = runs.get(liveRunId);
        if (!entry) break;
        const step = entry.steps.get(liveCurrentStepId);
        const detail = msg.content != null ? String(msg.content) : null;
        const t = { dir: '←', name: msg.toolName, detail };
        if (step) step.tools.push(t);
        if (liveThinkingPre) {
          liveThinkingPre.insertAdjacentHTML('beforeend', renderToolBadgeHtml(t));
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
        const messageHtml = msg.message
          ? '<div style="margin:4px 0;color:var(--vscode-foreground)">' + esc(msg.message) + '</div>'
          : '';
        const hitlHtml =
          '<div class="hitl-banner" id="' + bannerId + '">' +
          '<strong>⏸ HITL Gate</strong> — step paused for human review.' +
          messageHtml +
          '<textarea id="hitl-text-' + msg.stepId + '" placeholder="Optional guidance text..."></textarea>' +
          '<div class="btn-row">' +
          '<button class="btn-approve" onclick="submitHitl(\\'' + msg.stepId + '\\', true)">Approve</button>' +
          '<button class="btn-reject" onclick="submitHitl(\\'' + msg.stepId + '\\', false)">Reject</button>' +
          '</div></div>';
        step.hitlHtml = hitlHtml;
        updateStepItemGlyph(msg.stepId, 'paused');
        updateSectionGlyph(msg.stepId, 'paused');
        if (liveOutputPre) {
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

        // Rebuild steps from TurnEntry records.
        // Each step may have multiple rows (step_started + step_completed).
        // Aggregate by step_id, preferring data from step_completed rows.
        entry.steps.clear();
        const stepRowsMap = new Map();
        for (const te of (msg.steps || [])) {
          if (!te.step_id) continue;
          if (!stepRowsMap.has(te.step_id)) {
            stepRowsMap.set(te.step_id, []);
          }
          stepRowsMap.get(te.step_id).push(te);
        }
        for (const [stepId, rows] of stepRowsMap) {
          // Find the richest row: prefer step_completed, then step_failed, then step_skipped, then any
          const completedRow = rows.find(r => r.event_type === 'step_completed');
          const failedRow    = rows.find(r => r.event_type === 'step_failed');
          const skippedRow   = rows.find(r => r.event_type === 'step_skipped');
          const primaryRow   = completedRow || failedRow || skippedRow || rows[rows.length - 1];

          // Determine result code from event_type
          let resultCode = null;
          if (completedRow)     resultCode = 'completed';
          else if (failedRow)   resultCode = 'error';
          else if (skippedRow)  resultCode = 'skipped';

          // Determine status for glyph
          let status = 'completed';
          if (failedRow && !completedRow)  status = 'failed';
          else if (skippedRow && !completedRow && !failedRow) status = 'skipped';

          // Aggregate data: find prompt from step_started row if available
          const startedRow = rows.find(r => r.event_type === 'step_started');

          const step = makeStepEntry();
          step.status       = status;
          step.resultCode   = resultCode;
          step.output       = primaryRow.response || '';
          step.thinking     = primaryRow.thinking || '';
          step.resolvedPrompt = (startedRow || primaryRow).prompt || null;
          step.telemetry    = {
            inputTokens:  primaryRow.input_tokens ?? null,
            outputTokens: primaryRow.output_tokens ?? null,
            costUsd:      primaryRow.cost_usd ?? null,
            latencyMs:    completedRow?.latency_ms ?? null,
          };
          entry.steps.set(stepId, step);
        }

        // Reset rendered state so ensureRunSections rebuilds for this run
        if (renderedRunId === msg.runId) {
          renderedRunId = null;
          sectionRefs.clear();
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
