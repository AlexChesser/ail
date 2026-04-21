/**
 * HistoryService JSONL parsing tests.
 *
 * Tests the exported parseRunFileContent() pure function. Each .jsonl run file
 * is a mix of two line types:
 *   - Lines WITH a `type` field: executor events (step_started, pipeline_completed, etc.)
 *   - Lines WITHOUT a `type` field: TurnEntry records (completed step results)
 *
 * This is the critical parsing seam between the Rust turn log format and the
 * TypeScript history viewer.
 */

import * as assert from 'assert';
import { parseRunFileContent, RunRecord } from '../../application/parseRunFile';

// ── Helpers ───────────────────────────────────────────────────────────────────

function lines(...entries: object[]): string[] {
  return entries.map((e) => JSON.stringify(e));
}

function turnEntry(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    step_id: 'review',
    prompt: 'Fix the bug',
    response: 'Done!',
    cost_usd: 0.001,
    input_tokens: 50,
    output_tokens: 20,
    runner_session_id: 'ses_1',
    stdout: null,
    stderr: null,
    exit_code: null,
    thinking: null,
    ...overrides,
  };
}

// ── Suite ─────────────────────────────────────────────────────────────────────

suite('historyParsing: parseRunFileContent', () => {

  test('returns null for empty lines array', () => {
    const result = parseRunFileContent([], 'run-1', Date.now());
    assert.strictEqual(result, null);
  });

  test('returns null when only executor event lines present (no TurnEntries)', () => {
    const result = parseRunFileContent(
      lines(
        { type: 'step_started', step_id: 'review', step_index: 0, total_steps: 1, resolved_prompt: null },
        { type: 'pipeline_completed', outcome: 'completed' },
      ),
      'run-1',
      1000,
    );
    assert.strictEqual(result, null);
  });

  test('parses a valid JSONL with one TurnEntry', () => {
    const result = parseRunFileContent(
      lines(turnEntry()),
      'run-1',
      1000,
    ) as RunRecord;
    assert.ok(result !== null);
    assert.strictEqual(result.runId, 'run-1');
    assert.strictEqual(result.timestamp, 1000);
    assert.strictEqual(result.steps.length, 1);
    assert.strictEqual(result.steps[0].step_id, 'review');
    assert.strictEqual(result.steps[0].response, 'Done!');
  });

  test('outcome is completed when pipeline_completed event is present', () => {
    const result = parseRunFileContent(
      lines(
        { type: 'pipeline_completed', outcome: 'completed' },
        turnEntry(),
      ),
      'run-1',
      1000,
    ) as RunRecord;
    assert.strictEqual(result.outcome, 'completed');
  });

  test('outcome is failed when pipeline_error event is present', () => {
    const result = parseRunFileContent(
      lines(
        turnEntry(),
        { type: 'pipeline_error', error: 'oops', error_type: 'ail:runner/invocation-failed' },
      ),
      'run-1',
      1000,
    ) as RunRecord;
    assert.strictEqual(result.outcome, 'failed');
  });

  test('outcome defaults to completed when TurnEntries exist but no pipeline event', () => {
    const result = parseRunFileContent(
      lines(turnEntry()),
      'run-1',
      1000,
    ) as RunRecord;
    assert.strictEqual(result.outcome, 'completed');
  });

  test('malformed JSON lines are skipped gracefully', () => {
    const rawLines = [
      'not-valid-json',
      JSON.stringify(turnEntry()),
      '{broken',
    ];
    const result = parseRunFileContent(rawLines, 'run-1', 1000) as RunRecord;
    assert.ok(result !== null);
    assert.strictEqual(result.steps.length, 1);
  });

  test('TurnEntries without step_id are skipped', () => {
    const result = parseRunFileContent(
      lines(
        { prompt: 'no id here', response: 'x', cost_usd: 0 }, // no step_id
        turnEntry({ step_id: 'valid' }),
      ),
      'run-1',
      1000,
    ) as RunRecord;
    assert.strictEqual(result.steps.length, 1);
    assert.strictEqual(result.steps[0].step_id, 'valid');
  });

  test('invocation step_id populates invocationPrompt', () => {
    const result = parseRunFileContent(
      lines(
        turnEntry({ step_id: 'invocation', prompt: 'fix the auth bug' }),
        turnEntry({ step_id: 'review', prompt: 'review the fix' }),
      ),
      'run-1',
      1000,
    ) as RunRecord;
    assert.strictEqual(result.invocationPrompt, 'fix the auth bug');
  });

  test('invocationPrompt is empty string when no invocation step', () => {
    const result = parseRunFileContent(
      lines(turnEntry({ step_id: 'review' })),
      'run-1',
      1000,
    ) as RunRecord;
    assert.strictEqual(result.invocationPrompt, '');
  });

  test('totalCostUsd sums cost_usd across all TurnEntries', () => {
    const result = parseRunFileContent(
      lines(
        turnEntry({ step_id: 'step_a', cost_usd: 0.001 }),
        turnEntry({ step_id: 'step_b', cost_usd: 0.002 }),
        turnEntry({ step_id: 'step_c', cost_usd: null }),
      ),
      'run-1',
      1000,
    ) as RunRecord;
    assert.ok(Math.abs(result.totalCostUsd - 0.003) < 1e-9);
  });

  test('pipelineSource is extracted from step_started event', () => {
    const result = parseRunFileContent(
      lines(
        { type: 'step_started', step_id: 'review', step_index: 0, total_steps: 1,
          resolved_prompt: null, pipeline_source: 'demo/.ail.yaml' },
        turnEntry(),
      ),
      'run-1',
      1000,
    ) as RunRecord;
    assert.strictEqual(result.pipelineSource, 'demo/.ail.yaml');
  });

  test('pipelineSource defaults to unknown when no step_started event', () => {
    const result = parseRunFileContent(
      lines(turnEntry()),
      'run-1',
      1000,
    ) as RunRecord;
    assert.strictEqual(result.pipelineSource, 'unknown');
  });

  test('executor event lines (with type field) are not counted as TurnEntries', () => {
    const result = parseRunFileContent(
      lines(
        { type: 'step_started', step_id: 'review', step_index: 0, total_steps: 1, resolved_prompt: null },
        turnEntry({ step_id: 'review' }),
        { type: 'pipeline_completed', outcome: 'completed' },
      ),
      'run-1',
      1000,
    ) as RunRecord;
    assert.strictEqual(result.steps.length, 1, 'only TurnEntries should be in steps');
  });

  test('thinking field is preserved from TurnEntry', () => {
    const result = parseRunFileContent(
      lines(turnEntry({ thinking: 'I think therefore...' })),
      'run-1',
      1000,
    ) as RunRecord;
    assert.strictEqual(result.steps[0].thinking, 'I think therefore...');
  });
});
