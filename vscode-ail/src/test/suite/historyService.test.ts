/**
 * HistoryService unit tests — getRecentAverageCost().
 *
 * We stub the 'vscode' module before requiring HistoryService so that
 * VS Code API calls don't throw. The binary subprocess is bypassed by
 * subclassing HistoryService and overriding _fetchLogs.
 */

// ── vscode stub (must come before any require of HistoryService) ─────────────

// eslint-disable-next-line @typescript-eslint/no-require-imports
const Module = require('module');
const _origLoad = Module._load.bind(Module);
Module._load = function (request: string, ...args: unknown[]) {
  if (request === 'vscode') {
    return {
      workspace: {
        getConfiguration: () => ({ get: (_key: string, def: unknown) => def }),
      },
      window: {
        showWarningMessage: () => Promise.resolve(undefined),
      },
    };
  }
  return _origLoad(request, ...args);
};

// ── Imports (after stub) ──────────────────────────────────────────────────────

import * as assert from 'assert';
import { HistoryService } from '../../application/HistoryService';
import { RunRecord } from '../../application/parseRunFile';

// ── Subclass that bypasses the binary subprocess ──────────────────────────────

class TestableHistoryService extends HistoryService {
  private _records: RunRecord[] = [];

  setRecords(records: RunRecord[]): void {
    this._records = records;
  }

  // Override the private method by casting (for testing only).
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  protected async _fetchLogs(_opts: object): Promise<RunRecord[]> {
    return this._records;
  }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeRecord(overrides: Partial<RunRecord> = {}): RunRecord {
  return {
    runId: 'run-' + Math.random().toString(36).slice(2),
    timestamp: Date.now(),
    pipelineSource: 'demo/.ail.yaml',
    outcome: 'completed',
    totalCostUsd: 0.1,
    invocationPrompt: 'hello',
    steps: [{ step_id: 'invocation', event_type: null, prompt: 'hello', response: 'hi', cost_usd: 0.1,
      input_tokens: 10, output_tokens: 5, latency_ms: null, runner_session_id: null,
      stdout: null, stderr: null, exit_code: null, thinking: null }],
    ...overrides,
  };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

suite('HistoryService: getRecentAverageCost', () => {

  function makeService(): TestableHistoryService {
    return new TestableHistoryService({} as never, '/fake/cwd', '/fake/ail');
  }

  test('returns 0 when there are no records', async () => {
    const svc = makeService();
    svc.setRecords([]);
    const avg = await svc.getRecentAverageCost(10);
    assert.strictEqual(avg, 0);
  });

  test('returns 0 when all records have zero cost', async () => {
    const svc = makeService();
    svc.setRecords([
      makeRecord({ totalCostUsd: 0 }),
      makeRecord({ totalCostUsd: 0 }),
    ]);
    const avg = await svc.getRecentAverageCost(10);
    assert.strictEqual(avg, 0);
  });

  test('returns 0 when all records are failed (even with positive cost)', async () => {
    const svc = makeService();
    svc.setRecords([
      makeRecord({ outcome: 'failed', totalCostUsd: 0.5 }),
      makeRecord({ outcome: 'failed', totalCostUsd: 0.3 }),
    ]);
    const avg = await svc.getRecentAverageCost(10);
    assert.strictEqual(avg, 0);
  });

  test('computes the average of completed runs with positive cost', async () => {
    const svc = makeService();
    svc.setRecords([
      makeRecord({ totalCostUsd: 0.2 }),
      makeRecord({ totalCostUsd: 0.4 }),
    ]);
    const avg = await svc.getRecentAverageCost(10);
    // average of 0.2 and 0.4 = 0.3
    assert.ok(Math.abs(avg - 0.3) < 1e-9, `Expected ~0.3, got ${avg}`);
  });

  test('skips failed runs when computing average', async () => {
    const svc = makeService();
    svc.setRecords([
      makeRecord({ totalCostUsd: 0.2 }),
      makeRecord({ outcome: 'failed', totalCostUsd: 1.0 }),
      makeRecord({ totalCostUsd: 0.4 }),
    ]);
    const avg = await svc.getRecentAverageCost(10);
    // Only the two completed runs: (0.2 + 0.4) / 2 = 0.3
    assert.ok(Math.abs(avg - 0.3) < 1e-9, `Expected ~0.3, got ${avg}`);
  });

  test('skips zero-cost runs when computing average', async () => {
    const svc = makeService();
    svc.setRecords([
      makeRecord({ totalCostUsd: 0.0 }),
      makeRecord({ totalCostUsd: 0.6 }),
    ]);
    const avg = await svc.getRecentAverageCost(10);
    // Only the non-zero run counts: 0.6 / 1 = 0.6
    assert.ok(Math.abs(avg - 0.6) < 1e-9, `Expected 0.6, got ${avg}`);
  });

  test('handles a single run correctly', async () => {
    const svc = makeService();
    svc.setRecords([makeRecord({ totalCostUsd: 0.75 })]);
    const avg = await svc.getRecentAverageCost(10);
    assert.ok(Math.abs(avg - 0.75) < 1e-9, `Expected 0.75, got ${avg}`);
  });
});
