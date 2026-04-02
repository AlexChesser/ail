/**
 * UnifiedPanel tests.
 *
 * UnifiedPanel has a static _createWebviewPanel factory that we override in
 * setup() to return a mock panel — this avoids module-cache complications when
 * the vscode module was first loaded by a different test file.
 *
 * We access private statics via bracket notation for test purposes only.
 */

// ── vscode stub — must come before any import that transitively loads vscode ──

// eslint-disable-next-line @typescript-eslint/no-require-imports
const Module = require('module');
const _origLoad = Module._load.bind(Module);
Module._load = function (request: string, ...args: unknown[]) {
  if (request === 'vscode') {
    return {
      window: {
        showInformationMessage: () => Promise.resolve(undefined),
        showErrorMessage:       () => Promise.resolve(undefined),
        showWarningMessage:     () => Promise.resolve(undefined),
        createWebviewPanel:     () => { throw new Error('use UnifiedPanel._createWebviewPanel mock'); },
      },
      ViewColumn: { Beside: 2, One: 1, Two: 2, Active: -1 },
      Uri: { joinPath: (..._a: unknown[]) => ({ fsPath: '/fake/out' }) },
    };
  }
  return _origLoad(request, ...args);
};

// ── Imports (after stub) ─────────────────────────────────────────────────────

import * as assert from 'assert';
import { UnifiedPanel } from '../../panels/UnifiedPanel';
import { AilEvent } from '../../types';
import { getUnifiedPanelHtml } from '../../panels/unifiedPanelHtml';

// ── Mock panel factory ───────────────────────────────────────────────────────

interface MockWebview {
  html: string;
  messages: object[];
  messageListeners: ((msg: object) => void)[];
  postMessage(msg: object): void;
  onDidReceiveMessage(handler: (msg: object) => void): { dispose(): void };
}

function makeMockWebview(): MockWebview {
  const listeners: ((msg: object) => void)[] = [];
  return {
    html: '',
    messages: [],
    messageListeners: listeners,
    postMessage(msg: object) { this.messages.push(msg); },
    onDidReceiveMessage(handler: (msg: object) => void) {
      listeners.push(handler);
      return { dispose: () => { /* no-op */ } };
    },
  };
}

interface MockPanel {
  title: string;
  webview: MockWebview;
  revealCalled: number;
  disposeListeners: (() => void)[];
  reveal(): void;
  onDidDispose(cb: () => void): { dispose(): void };
  dispose(): void;
}

function makeMockPanel(): MockPanel {
  const disposeListeners: (() => void)[] = [];
  return {
    title: '',
    webview: makeMockWebview(),
    revealCalled: 0,
    disposeListeners,
    reveal() { this.revealCalled++; },
    onDidDispose(cb: () => void) {
      disposeListeners.push(cb);
      return { dispose: () => { /* no-op */ } };
    },
    dispose() {
      for (const cb of this.disposeListeners) cb();
    },
  };
}

// ── Test helpers ─────────────────────────────────────────────────────────────

function panel(): MockPanel {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (UnifiedPanel as any)._instance?._panel as MockPanel;
}

function signalReady(): void {
  const p = panel();
  if (!p) return;
  for (const l of p.webview.messageListeners) l({ type: 'ready' });
}

function getMessages(cmd: string): object[] {
  const p = panel();
  if (!p) return [];
  return p.webview.messages.filter((m: unknown) => (m as { cmd: string }).cmd === cmd);
}

function fakeCtx() {
  return { extensionUri: { fsPath: '/fake' } } as never;
}

// ── Suite ─────────────────────────────────────────────────────────────────────

suite('UnifiedPanel', () => {

  // Reset singleton and inject mock panel factory before each test
  setup(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (UnifiedPanel as any)._instance = undefined;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (UnifiedPanel as any)._historyService = undefined;

    // Override factory so _getOrCreate returns our mock
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (UnifiedPanel as any)._createWebviewPanel = () => makeMockPanel();
  });

  test('startLiveRun creates a panel on first call', () => {
    UnifiedPanel.startLiveRun(fakeCtx(), 'run-1', () => { /* no-op */ });
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    assert.ok((UnifiedPanel as any)._instance, 'singleton should be set');
  });

  test('singleton: second startLiveRun reuses the same instance', () => {
    UnifiedPanel.startLiveRun(fakeCtx(), 'run-1', () => { /* no-op */ });
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const first = (UnifiedPanel as any)._instance;

    UnifiedPanel.startLiveRun(fakeCtx(), 'run-2', () => { /* no-op */ });
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const second = (UnifiedPanel as any)._instance;

    assert.strictEqual(first, second, 'same instance should be reused');
  });

  test('singleton: panel.reveal is called on each startLiveRun', () => {
    UnifiedPanel.startLiveRun(fakeCtx(), 'run-1', () => { /* no-op */ });
    UnifiedPanel.startLiveRun(fakeCtx(), 'run-2', () => { /* no-op */ });
    assert.strictEqual(panel().revealCalled, 2);
  });

  test('init message is queued and delivered after ready signal', () => {
    UnifiedPanel.startLiveRun(fakeCtx(), 'run-1', () => { /* no-op */ });

    // Before ready: init should be buffered
    assert.strictEqual(getMessages('init').length, 0, 'init buffered before ready');

    signalReady();

    assert.strictEqual(getMessages('init').length, 1, 'init delivered after ready');
  });

  test('liveRunStarted is sent when run_started event arrives', () => {
    const instance = UnifiedPanel.startLiveRun(fakeCtx(), 'run-1', () => { /* no-op */ });
    signalReady();

    instance.onEvent({ type: 'run_started', run_id: 'run-1', pipeline_source: 'test.ail.yaml', total_steps: 2 });

    const msgs = getMessages('liveRunStarted');
    assert.strictEqual(msgs.length, 1);
    assert.strictEqual((msgs[0] as { runId: string }).runId, 'run-1');
    assert.strictEqual((msgs[0] as { totalSteps: number }).totalSteps, 2);
  });

  test('stepStarted, streamDelta, stepCompleted events are forwarded', () => {
    const instance = UnifiedPanel.startLiveRun(fakeCtx(), 'run-1', () => { /* no-op */ });
    signalReady();

    instance.onEvent({ type: 'step_started', step_id: 's1', step_index: 0, total_steps: 1, resolved_prompt: null });
    instance.onEvent({ type: 'runner_event', event: { type: 'stream_delta', text: 'hello ' } });
    instance.onEvent({ type: 'runner_event', event: { type: 'stream_delta', text: 'world' } });
    instance.onEvent({ type: 'step_completed', step_id: 's1', cost_usd: 0.001, input_tokens: 100, output_tokens: 50, response: null });

    assert.strictEqual(getMessages('stepStarted').length, 1);
    assert.strictEqual(getMessages('streamDelta').length, 2);
    assert.strictEqual(getMessages('stepCompleted').length, 1);
  });

  test('thinking, toolUse, toolResult, hitlGate, permissionReq events are forwarded', () => {
    const instance = UnifiedPanel.startLiveRun(fakeCtx(), 'run-1', () => { /* no-op */ });
    signalReady();

    instance.onEvent({ type: 'step_started', step_id: 's1', step_index: 0, total_steps: 1, resolved_prompt: null });
    instance.onEvent({ type: 'runner_event', event: { type: 'thinking', text: '...' } });
    instance.onEvent({ type: 'runner_event', event: { type: 'tool_use', tool_name: 'read_file' } });
    instance.onEvent({ type: 'runner_event', event: { type: 'tool_result', tool_name: 'read_file' } });
    instance.onEvent({ type: 'hitl_gate_reached', step_id: 's1' });
    instance.onEvent({ type: 'runner_event', event: { type: 'permission_requested', display_name: 'fs', display_detail: 'read /tmp' } });

    assert.strictEqual(getMessages('thinking').length, 1);
    assert.strictEqual(getMessages('toolUse').length, 1);
    assert.strictEqual(getMessages('toolResult').length, 1);
    assert.strictEqual(getMessages('hitlGate').length, 1);
    assert.strictEqual(getMessages('permissionReq').length, 1);
  });

  test('pipelineCompleted updates panel title and posts pipelineCompleted', () => {
    const instance = UnifiedPanel.startLiveRun(fakeCtx(), 'run-1', () => { /* no-op */ });
    signalReady();

    instance.onEvent({ type: 'pipeline_completed', outcome: 'completed' });

    assert.strictEqual(panel().title, 'ail: Completed');
    assert.strictEqual(getMessages('pipelineCompleted').length, 1);
  });

  test('pipelineError updates panel title and posts pipelineError', () => {
    const instance = UnifiedPanel.startLiveRun(fakeCtx(), 'run-1', () => { /* no-op */ });
    signalReady();

    instance.onEvent({ type: 'pipeline_error', error: 'oops', error_type: 'SOME_ERR' });

    assert.strictEqual(panel().title, 'ail: Error');
    assert.strictEqual(getMessages('pipelineError').length, 1);
  });

  test('HITL response is forwarded to the correct writeStdin callback', () => {
    const stdin1: object[] = [];
    const instance = UnifiedPanel.startLiveRun(fakeCtx(), 'run-1', (m) => stdin1.push(m));
    signalReady();

    instance.onEvent({ type: 'run_started', run_id: 'run-1', pipeline_source: null, total_steps: 1 });

    // Simulate webview sending hitl_response
    for (const l of panel().webview.messageListeners) {
      l({ type: 'hitl_response', stepId: 's1', text: 'go ahead' });
    }

    assert.strictEqual(stdin1.length, 1);
    assert.deepStrictEqual(stdin1[0], { type: 'hitl_response', step_id: 's1', text: 'go ahead' });
  });

  test('onRunComplete clears writeStdin; subsequent HITL has no effect', () => {
    const stdin1: object[] = [];
    const instance = UnifiedPanel.startLiveRun(fakeCtx(), 'run-1', (m) => stdin1.push(m));
    signalReady();

    instance.onEvent({ type: 'run_started', run_id: 'run-1', pipeline_source: null, total_steps: 1 });
    instance.onRunComplete('run-1');

    for (const l of panel().webview.messageListeners) {
      l({ type: 'hitl_response', stepId: 's1', text: 'too late' });
    }

    assert.strictEqual(stdin1.length, 0, 'no stdin messages after run complete');
  });

  test('panel disposal clears singleton; next startLiveRun creates fresh instance', () => {
    UnifiedPanel.startLiveRun(fakeCtx(), 'run-1', () => { /* no-op */ });
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const firstInstance = (UnifiedPanel as any)._instance;

    panel().dispose();   // triggers onDidDispose → clears _instance

    UnifiedPanel.startLiveRun(fakeCtx(), 'run-2', () => { /* no-op */ });
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const secondInstance = (UnifiedPanel as any)._instance;

    assert.ok(secondInstance, 'new instance created after disposal');
    assert.notStrictEqual(firstInstance, secondInstance, 'should be a different instance');
  });

  test('openReview reuses the singleton panel', () => {
    UnifiedPanel.startLiveRun(fakeCtx(), 'run-1', () => { /* no-op */ });
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const firstInstance = (UnifiedPanel as any)._instance;

    const mockRecord = {
      runId: 'hist-1',
      timestamp: Date.now() - 60000,
      pipelineSource: 'test.ail.yaml',
      outcome: 'completed' as const,
      totalCostUsd: 0.001,
      invocationPrompt: 'hello',
      steps: [],
    };
    UnifiedPanel.openReview(fakeCtx(), mockRecord);

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    assert.strictEqual((UnifiedPanel as any)._instance, firstInstance, 'openReview should reuse existing panel');
  });

  test('no steps yet regression: second run posts liveRunStarted to same panel', () => {
    const instance = UnifiedPanel.startLiveRun(fakeCtx(), 'run-1', () => { /* no-op */ });
    signalReady();

    instance.onEvent({ type: 'run_started', run_id: 'run-1', pipeline_source: null, total_steps: 1 });
    instance.onEvent({ type: 'step_started', step_id: 's1', step_index: 0, total_steps: 1, resolved_prompt: null });
    instance.onEvent({ type: 'pipeline_completed', outcome: 'completed' });
    instance.onRunComplete('run-1');

    const msgCountAfterFirst = panel().webview.messages.length;

    // Second run: reuses same panel
    UnifiedPanel.startLiveRun(fakeCtx(), 'run-2', () => { /* no-op */ });
    instance.onEvent({ type: 'run_started', run_id: 'run-2', pipeline_source: null, total_steps: 1 });

    assert.strictEqual(getMessages('liveRunStarted').length, 2, 'both runs post liveRunStarted');
    assert.ok(panel().webview.messages.length > msgCountAfterFirst,
      'second run adds new messages to the same panel');
  });

  test('AilEvent: run_started event type for testing (AilEvent)', () => {
    // Regression: ensure AilEvent discriminated union works with onEvent
    const runStarted: AilEvent = {
      type: 'run_started', run_id: 'x', pipeline_source: null, total_steps: 0,
    };
    const instance = UnifiedPanel.startLiveRun(fakeCtx(), 'x', () => { /* no-op */ });
    signalReady();
    // Should not throw
    instance.onEvent(runStarted);
    assert.ok(getMessages('liveRunStarted').length > 0);
  });

  test('webview script is syntactically valid JavaScript', () => {
    // Regression: escaped quotes in onclick handlers (e.g. \' inside a TS template
    // literal) produce invalid JS that silently prevents the ready signal from firing,
    // leaving the panel permanently empty. Catch it at test time, not at runtime.
    const html = getUnifiedPanelHtml();
    const scriptStart = html.indexOf('<script>');
    const scriptEnd   = html.indexOf('</script>');
    assert.ok(scriptStart !== -1, 'webview HTML must contain a <script> block');
    const js = html.slice(scriptStart + 8, scriptEnd);
    // new Function() parses the script body; throws SyntaxError if invalid
    assert.doesNotThrow(() => new Function(js), 'webview script must be valid JavaScript');
  });
});
