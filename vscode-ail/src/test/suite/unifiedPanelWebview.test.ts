/**
 * UnifiedPanel webview DOM tests.
 *
 * Uses jsdom to parse the full webview HTML (including inline <script>),
 * injects a mock acquireVsCodeApi(), then drives the webview through
 * postMessage calls and asserts DOM state.
 *
 * These tests catch bugs in the inline JavaScript that handles 15+ message
 * types — code that cannot be tested by the host-side UnifiedPanel tests.
 *
 * Setup pattern:
 *   1. JSDOM with runScripts:'dangerously' + beforeParse to inject acquireVsCodeApi
 *   2. document.write(html) triggers script evaluation
 *   3. Dispatch MessageEvent to simulate host→webview postMessage
 *   4. Assert DOM via querySelector / textContent / classList
 */

// eslint-disable-next-line @typescript-eslint/no-require-imports
const { JSDOM } = require('jsdom');
import * as assert from 'assert';
import { getUnifiedPanelHtml } from '../../panels/unifiedPanelHtml';

// ── Test helpers ──────────────────────────────────────────────────────────────

interface WebviewHarness {
  document: Document;
  sent: object[];
  post(msg: object): void;
}

function createWebview(): WebviewHarness {
  const sent: object[] = [];
  // Pass the HTML directly to the JSDOM constructor so that inline <script> blocks
  // are evaluated during parsing (after beforeParse injects acquireVsCodeApi).
  // Using document.write() after construction does NOT reliably execute scripts.
  const dom = new JSDOM(getUnifiedPanelHtml(), {
    runScripts: 'dangerously',
    beforeParse(win: { acquireVsCodeApi?: () => unknown }) {
      win.acquireVsCodeApi = () => ({
        postMessage: (msg: object) => { sent.push(msg); },
        getState:    () => ({}),
        setState:    () => { /* no-op */ },
      });
    },
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  }) as any;

  function post(msg: object): void {
    dom.window.dispatchEvent(
      new dom.window.MessageEvent('message', { data: msg })
    );
  }

  return { document: dom.window.document as Document, sent, post };
}

/** Run a standard live-run preamble: liveRunStarted → stepStarted.
 *  Returns the harness so callers can continue driving it. */
function startLiveRun(
  harness: WebviewHarness,
  runId = 'run-1',
  stepId = 's1',
): WebviewHarness {
  harness.post({ cmd: 'liveRunStarted', runId, totalSteps: 2, pipelineSource: 'test.ail.yaml' });
  harness.post({ cmd: 'stepStarted', stepId, stepIndex: 0, totalSteps: 2, resolvedPrompt: null });
  return harness;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

suite('unifiedPanelWebview: init', () => {
  test('webview sends ready signal after script loads', () => {
    const { sent } = createWebview();
    assert.ok(sent.some((m) => (m as { type?: string }).type === 'ready'),
      'webview should post { type: "ready" } on load');
  });

  test('init message clears state — history shows "No runs yet"', () => {
    const h = createWebview();
    h.post({ cmd: 'liveRunStarted', runId: 'r1', totalSteps: 1, pipelineSource: 'x.ail.yaml' });
    h.post({ cmd: 'init' });
    const historyList = h.document.getElementById('history-list');
    assert.ok(historyList?.textContent?.includes('No runs yet'));
  });

  test('cost-display and step-display start at defaults', () => {
    const { document } = createWebview();
    assert.strictEqual(document.getElementById('cost-display')?.textContent, 'Cost: —');
    assert.strictEqual(document.getElementById('step-display')?.textContent, 'Steps: —');
  });
});

suite('unifiedPanelWebview: liveRunStarted', () => {
  test('adds a run-item to history list', () => {
    const h = createWebview();
    h.post({ cmd: 'liveRunStarted', runId: 'run-1', totalSteps: 3, pipelineSource: 'demo.ail.yaml' });
    const items = h.document.querySelectorAll('.run-item');
    assert.strictEqual(items.length, 1);
    assert.strictEqual((items[0] as HTMLElement).dataset.runId, 'run-1');
  });

  test('updates step counter to Steps: 0/N', () => {
    const h = createWebview();
    h.post({ cmd: 'liveRunStarted', runId: 'run-1', totalSteps: 5, pipelineSource: 'x.ail.yaml' });
    assert.strictEqual(h.document.getElementById('step-display')?.textContent, 'Steps: 0/5');
  });

  test('second liveRunStarted reuses history panel (run-item appears for both)', () => {
    const h = createWebview();
    h.post({ cmd: 'liveRunStarted', runId: 'run-1', totalSteps: 1, pipelineSource: 'x.ail.yaml' });
    h.post({ cmd: 'pipelineCompleted', outcome: 'completed', totalCost: 0 });
    h.post({ cmd: 'liveRunStarted', runId: 'run-2', totalSteps: 1, pipelineSource: 'x.ail.yaml' });
    const items = h.document.querySelectorAll('.run-item');
    assert.strictEqual(items.length, 2);
  });
});

suite('unifiedPanelWebview: stepStarted', () => {
  test('adds a step-item to steps list', () => {
    const h = startLiveRun(createWebview());
    const items = h.document.querySelectorAll('.step-item');
    assert.strictEqual(items.length, 1);
    assert.strictEqual((items[0] as HTMLElement).dataset.stepId, 's1');
  });

  test('selected step appears in detail header', () => {
    const h = startLiveRun(createWebview());
    const label = h.document.getElementById('detail-step-label');
    assert.ok(label?.textContent?.includes('s1'));
  });

  test('step display counter increments', () => {
    const h = startLiveRun(createWebview());
    assert.strictEqual(h.document.getElementById('step-display')?.textContent, 'Steps: 1/2');
  });

  test('step-item has glyph-running class while running', () => {
    const h = startLiveRun(createWebview());
    const stepItem = h.document.getElementById('step-item-s1');
    const glyph = stepItem?.querySelector('.step-glyph');
    assert.ok(glyph?.classList.contains('glyph-running'));
  });
});

suite('unifiedPanelWebview: streamDelta', () => {
  test('appends text to output-block pre', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'streamDelta', text: 'Hello ' });
    h.post({ cmd: 'streamDelta', text: 'world!' });
    const outputPre = h.document.querySelector('.output-block .block-content') as HTMLElement;
    assert.ok(outputPre?.textContent?.includes('Hello world!'),
      `Expected "Hello world!" in output, got "${outputPre?.textContent}"`);
  });

  test('opens the output-block details on first delta', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'streamDelta', text: 'x' });
    const outBlock = h.document.querySelector('.output-block') as HTMLDetailsElement;
    assert.ok(outBlock?.open, 'output-block should be open after streamDelta');
  });
});

suite('unifiedPanelWebview: thinking', () => {
  test('appends text to thinking-block pre', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'thinking', text: 'Step 1: analyze...' });
    const thinkPre = h.document.querySelector('.thinking-block .block-content') as HTMLElement;
    assert.ok(thinkPre?.textContent?.includes('Step 1: analyze...'));
  });

  test('opens the thinking-block details', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'thinking', text: '...' });
    const thinkBlock = h.document.querySelector('.thinking-block') as HTMLDetailsElement;
    assert.ok(thinkBlock?.open);
  });
});

suite('unifiedPanelWebview: toolUse and toolResult', () => {
  test('toolUse inserts a → tool-badge in thinking area', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'toolUse', toolName: 'Bash' });
    const thinkPre = h.document.querySelector('.thinking-block .block-content') as HTMLElement;
    assert.ok(thinkPre?.innerHTML?.includes('→ Bash'));
  });

  test('toolResult inserts a ← tool-badge in thinking area', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'toolResult', toolName: 'Read' });
    const thinkPre = h.document.querySelector('.thinking-block .block-content') as HTMLElement;
    assert.ok(thinkPre?.innerHTML?.includes('← Read'));
  });
});

suite('unifiedPanelWebview: stepCompleted', () => {
  test('updates step-item glyph to completed (✓)', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'stepCompleted', stepId: 's1', costUsd: 0.001, inputTokens: 50, outputTokens: 20, latencyMs: 1200, totalCost: 0.001 });
    const glyph = h.document.querySelector('#step-item-s1 .step-glyph') as HTMLElement;
    assert.ok(glyph?.classList.contains('glyph-completed'));
  });

  test('updates cost display', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'stepCompleted', stepId: 's1', costUsd: 0.0042, inputTokens: 100, outputTokens: 50, latencyMs: 500, totalCost: 0.0042 });
    assert.strictEqual(h.document.getElementById('cost-display')?.textContent, 'Cost: $0.0042');
  });
});

suite('unifiedPanelWebview: stepSkipped', () => {
  test('updates step-item glyph to skipped (⊘)', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'stepSkipped', stepId: 's1' });
    const glyph = h.document.querySelector('#step-item-s1 .step-glyph') as HTMLElement;
    assert.ok(glyph?.classList.contains('glyph-skipped'));
  });
});

suite('unifiedPanelWebview: stepFailed', () => {
  test('updates step-item glyph to failed (✗)', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'stepFailed', stepId: 's1', error: 'Runner crashed' });
    const glyph = h.document.querySelector('#step-item-s1 .step-glyph') as HTMLElement;
    assert.ok(glyph?.classList.contains('glyph-failed'));
  });

  test('inserts error text into output area', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'stepFailed', stepId: 's1', error: 'Runner crashed' });
    // After stepFailed the liveOutputPre is cleared, but output is stored in step state.
    // The output-block shows the error if the step was selected (it was, via stepStarted).
    const outputPre = h.document.querySelector('.output-block .block-content') as HTMLElement;
    assert.ok(outputPre?.innerHTML?.includes('Runner crashed'));
  });
});

suite('unifiedPanelWebview: hitlGate', () => {
  test('inserts HITL banner with Approve and Reject buttons', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'hitlGate', stepId: 's1' });
    const banner = h.document.querySelector('.hitl-banner') as HTMLElement;
    assert.ok(banner, 'HITL banner should be present');
    assert.ok(banner.querySelector('.btn-approve'), 'Approve button should be present');
    assert.ok(banner.querySelector('.btn-reject'), 'Reject button should be present');
  });

  test('updates step glyph to paused (⏸)', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'hitlGate', stepId: 's1' });
    const glyph = h.document.querySelector('#step-item-s1 .step-glyph') as HTMLElement;
    assert.ok(glyph?.classList.contains('glyph-paused'));
  });
});

suite('unifiedPanelWebview: permissionReq', () => {
  test('inserts permission banner with Allow and Deny buttons', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'permissionReq', displayName: 'Bash', displayDetail: 'ls /tmp' });
    const banner = h.document.querySelector('.permission-banner') as HTMLElement;
    assert.ok(banner, 'permission banner should be present');
    assert.ok(banner.querySelector('.btn-allow'), 'Allow button should be present');
    assert.ok(banner.querySelector('.btn-deny'), 'Deny button should be present');
  });

  test('banner includes displayName and displayDetail', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'permissionReq', displayName: 'Write', displayDetail: '/etc/hosts' });
    const banner = h.document.querySelector('.permission-banner') as HTMLElement;
    assert.ok(banner?.textContent?.includes('Write'));
    assert.ok(banner?.textContent?.includes('/etc/hosts'));
  });
});

suite('unifiedPanelWebview: costUpdate', () => {
  test('updates cost-display', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'costUpdate', totalCost: 0.0123 });
    assert.strictEqual(h.document.getElementById('cost-display')?.textContent, 'Cost: $0.0123');
  });
});

suite('unifiedPanelWebview: pipelineCompleted', () => {
  test('run-item glyph changes from spinning to completed', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'pipelineCompleted', outcome: 'completed', totalCost: 0.005 });
    const runItem = h.document.querySelector('[data-run-id="run-1"]') as HTMLElement;
    const glyph = runItem?.querySelector('.run-glyph') as HTMLElement;
    // The run-item re-renders; spinning class should be gone, glyph shows ✓
    assert.ok(glyph?.textContent?.includes('✓'), `Expected ✓ glyph, got: "${glyph?.innerHTML}"`);
  });

  test('updates cost-display to final cost', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'pipelineCompleted', outcome: 'completed', totalCost: 0.0099 });
    assert.strictEqual(h.document.getElementById('cost-display')?.textContent, 'Cost: $0.0099');
  });
});

suite('unifiedPanelWebview: pipelineError', () => {
  test('run-item glyph changes to failed (✗)', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'pipelineError', error: 'Template var missing', errorType: 'ail:template/unresolved-variable' });
    const runItem = h.document.querySelector('[data-run-id="run-1"]') as HTMLElement;
    const glyph = runItem?.querySelector('.run-glyph') as HTMLElement;
    assert.ok(glyph?.textContent?.includes('✗'), `Expected ✗ glyph, got: "${glyph?.innerHTML}"`);
  });
});

suite('unifiedPanelWebview: full sequence', () => {
  test('complete 2-step run updates all relevant DOM elements', () => {
    const h = createWebview();

    // Start run
    h.post({ cmd: 'liveRunStarted', runId: 'r1', totalSteps: 2, pipelineSource: 'demo.ail.yaml' });
    assert.strictEqual(h.document.querySelectorAll('.run-item').length, 1);

    // Step 1
    h.post({ cmd: 'stepStarted', stepId: 'review', stepIndex: 0, totalSteps: 2, resolvedPrompt: 'Review the code' });
    h.post({ cmd: 'streamDelta', text: 'LGTM' });
    h.post({ cmd: 'stepCompleted', stepId: 'review', costUsd: 0.001, inputTokens: 10, outputTokens: 5, latencyMs: 100, totalCost: 0.001 });

    // Step 2 auto-selects, so DOM switches to step 2 detail
    h.post({ cmd: 'stepStarted', stepId: 'summarize', stepIndex: 1, totalSteps: 2, resolvedPrompt: null });
    h.post({ cmd: 'streamDelta', text: 'Summary done' });
    h.post({ cmd: 'stepCompleted', stepId: 'summarize', costUsd: 0.002, inputTokens: 20, outputTokens: 10, latencyMs: 200, totalCost: 0.003 });

    // Pipeline done
    h.post({ cmd: 'pipelineCompleted', outcome: 'completed', totalCost: 0.003 });

    // Verify final state
    assert.strictEqual(h.document.querySelectorAll('.step-item').length, 2);
    assert.strictEqual(h.document.getElementById('cost-display')?.textContent, 'Cost: $0.0030');

    // Both steps should be marked completed
    const glyphs = h.document.querySelectorAll('.step-item .step-glyph');
    for (const g of glyphs) {
      assert.ok(
        g.classList.contains('glyph-completed'),
        `Expected glyph-completed, got ${g.className}`,
      );
    }
  });
});

suite('unifiedPanelWebview: HITL interaction', () => {
  test('clicking Approve sends hitl_response to host', () => {
    const h = startLiveRun(createWebview());
    h.post({ cmd: 'hitlGate', stepId: 's1' });
    const btn = h.document.querySelector('.btn-approve') as HTMLButtonElement;
    btn?.click();
    const hitlMsg = h.sent.find((m) => (m as { type?: string }).type === 'hitl_response');
    assert.ok(hitlMsg, 'Should have sent hitl_response');
    assert.strictEqual((hitlMsg as { stepId?: string }).stepId, 's1');
  });
});

suite('unifiedPanelWebview: reviewData', () => {
  test('populates step list from TurnEntry records', () => {
    const h = createWebview();
    h.post({
      cmd: 'reviewData',
      runId: 'hist-1',
      timestamp: Date.now() - 60000,
      pipelineSource: 'demo.ail.yaml',
      outcome: 'completed',
      totalCostUsd: 0.005,
      invocationPrompt: 'fix the bug',
      steps: [
        { step_id: 'review', prompt: 'Fix the bug', response: 'Done!', cost_usd: 0.005, input_tokens: 100, output_tokens: 50, runner_session_id: null, stdout: null, stderr: null, exit_code: null, thinking: null },
      ],
    });
    // reviewData requests the host to push the run to the panel, but the panel also auto-selects
    // the run via selectStep so the detail should be populated
    const stepItems = h.document.querySelectorAll('.step-item');
    assert.strictEqual(stepItems.length, 1);
    assert.strictEqual((stepItems[0] as HTMLElement).dataset.stepId, 'review');
  });
});
