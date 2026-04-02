/**
 * Wire format contract tests.
 *
 * Each golden fixture in spec/fixtures/events/ is a canonical JSON object
 * representing one AilEvent variant as emitted by the Rust executor.
 *
 * These tests verify:
 *   1. Every fixture parses as a valid AilEvent (type discriminator present)
 *   2. Required field names and types match the TypeScript interface
 *   3. mapAilEvent() produces the expected RunnerEvent (or undefined)
 *
 * If a Rust field is renamed, the corresponding Rust golden-fixture test
 * (s23_structured_output.rs) fails first, prompting an update to the fixture.
 * Once the fixture is updated, these TypeScript tests also fail unless
 * types.ts and mapEvent.ts are updated to match.
 *
 * This creates a complete cross-language contract guard.
 */

import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import { AilEvent } from '../../types';
import { mapAilEvent } from '../../application/mapEvent';

// ── Fixture loader ─────────────────────────────────────────────────────────

// The compiled test lives at vscode-ail/out/src/test/suite/wireContract.test.js
// Walking up 5 levels reaches the workspace root (ail/).
function fixturesDir(): string {
  return path.resolve(__dirname, '..', '..', '..', '..', '..', 'spec', 'fixtures', 'events');
}

function loadFixture(name: string): unknown {
  const filePath = path.join(fixturesDir(), name);
  const content = fs.readFileSync(filePath, 'utf8');
  return JSON.parse(content);
}

function asAilEvent(name: string): AilEvent {
  const obj = loadFixture(name) as AilEvent;
  assert.ok(obj && typeof obj === 'object', `${name}: must be an object`);
  assert.ok(typeof (obj as { type?: unknown }).type === 'string', `${name}: must have a string 'type' field`);
  return obj;
}

// ── run_started ───────────────────────────────────────────────────────────

suite('wireContract: run_started', () => {
  test('run_started fixture has required fields', () => {
    const ev = loadFixture('run_started.json') as Record<string, unknown>;
    assert.strictEqual(ev['type'], 'run_started');
    assert.strictEqual(typeof ev['run_id'], 'string');
    assert.ok(ev['total_steps'] !== undefined, 'total_steps must be present');
    // pipeline_source may be string or null
    assert.ok(ev['pipeline_source'] === null || typeof ev['pipeline_source'] === 'string',
      'pipeline_source must be string | null');
  });

  test('run_started is not mapped (mapAilEvent returns undefined)', () => {
    const ev = asAilEvent('run_started.json');
    assert.strictEqual(mapAilEvent(ev), undefined);
  });
});

// ── step_started ──────────────────────────────────────────────────────────

suite('wireContract: step_started', () => {
  test('step_started fixture has required fields', () => {
    const ev = loadFixture('step_started.json') as Record<string, unknown>;
    assert.strictEqual(ev['type'], 'step_started');
    assert.strictEqual(typeof ev['step_id'], 'string');
    assert.strictEqual(typeof ev['step_index'], 'number');
    assert.strictEqual(typeof ev['total_steps'], 'number');
    assert.strictEqual(typeof ev['resolved_prompt'], 'string');
  });

  test('step_started no_prompt: resolved_prompt is null', () => {
    const ev = loadFixture('step_started_no_prompt.json') as Record<string, unknown>;
    assert.strictEqual(ev['type'], 'step_started');
    assert.strictEqual(ev['resolved_prompt'], null);
  });

  test('step_started maps to RunnerEvent step_started with camelCase ids', () => {
    const ev = asAilEvent('step_started.json');
    const mapped = mapAilEvent(ev);
    assert.ok(mapped !== undefined);
    assert.strictEqual(mapped!.type, 'step_started');
    if (mapped!.type === 'step_started') {
      assert.strictEqual(mapped.stepId, 'review');
      assert.strictEqual(mapped.stepIndex, 0);
      assert.strictEqual(mapped.totalSteps, 3);
    }
  });
});

// ── step_completed ────────────────────────────────────────────────────────

suite('wireContract: step_completed', () => {
  test('step_completed fixture has required fields', () => {
    const ev = loadFixture('step_completed.json') as Record<string, unknown>;
    assert.strictEqual(ev['type'], 'step_completed');
    assert.strictEqual(typeof ev['step_id'], 'string');
    assert.strictEqual(typeof ev['cost_usd'], 'number');
    assert.strictEqual(typeof ev['input_tokens'], 'number');
    assert.strictEqual(typeof ev['output_tokens'], 'number');
    assert.ok(ev['response'] === null || typeof ev['response'] === 'string');
  });

  test('step_completed no_cost: cost_usd and response are null', () => {
    const ev = loadFixture('step_completed_no_cost.json') as Record<string, unknown>;
    assert.strictEqual(ev['cost_usd'], null);
    assert.strictEqual(ev['response'], null);
  });

  test('step_completed maps to RunnerEvent step_completed', () => {
    const ev = asAilEvent('step_completed.json');
    const mapped = mapAilEvent(ev);
    assert.ok(mapped !== undefined);
    assert.strictEqual(mapped!.type, 'step_completed');
    if (mapped!.type === 'step_completed') {
      assert.strictEqual(mapped.stepId, 'review');
    }
  });
});

// ── step_skipped ──────────────────────────────────────────────────────────

suite('wireContract: step_skipped', () => {
  test('step_skipped fixture has required fields', () => {
    const ev = loadFixture('step_skipped.json') as Record<string, unknown>;
    assert.strictEqual(ev['type'], 'step_skipped');
    assert.strictEqual(typeof ev['step_id'], 'string');
  });

  test('step_skipped maps correctly', () => {
    const ev = asAilEvent('step_skipped.json');
    const mapped = mapAilEvent(ev);
    assert.ok(mapped !== undefined);
    assert.strictEqual(mapped!.type, 'step_skipped');
    if (mapped!.type === 'step_skipped') {
      assert.strictEqual(mapped.stepId, 'optional_step');
    }
  });
});

// ── step_failed ───────────────────────────────────────────────────────────

suite('wireContract: step_failed', () => {
  test('step_failed fixture has required fields', () => {
    const ev = loadFixture('step_failed.json') as Record<string, unknown>;
    assert.strictEqual(ev['type'], 'step_failed');
    assert.strictEqual(typeof ev['step_id'], 'string');
    assert.strictEqual(typeof ev['error'], 'string');
  });

  test('step_failed maps to RunnerEvent step_failed', () => {
    const ev = asAilEvent('step_failed.json');
    const mapped = mapAilEvent(ev);
    assert.ok(mapped !== undefined);
    assert.strictEqual(mapped!.type, 'step_failed');
    if (mapped!.type === 'step_failed') {
      assert.strictEqual(mapped.stepId, 'review');
      assert.strictEqual(typeof mapped.error, 'string');
    }
  });
});

// ── hitl_gate_reached ─────────────────────────────────────────────────────

suite('wireContract: hitl_gate_reached', () => {
  test('hitl_gate_reached fixture has required fields', () => {
    const ev = loadFixture('hitl_gate_reached.json') as Record<string, unknown>;
    assert.strictEqual(ev['type'], 'hitl_gate_reached');
    assert.strictEqual(typeof ev['step_id'], 'string');
  });

  test('hitl_gate_reached maps correctly', () => {
    const ev = asAilEvent('hitl_gate_reached.json');
    const mapped = mapAilEvent(ev);
    assert.ok(mapped !== undefined);
    assert.strictEqual(mapped!.type, 'hitl_gate_reached');
    if (mapped!.type === 'hitl_gate_reached') {
      assert.strictEqual(mapped.stepId, 'approval_gate');
    }
  });
});

// ── runner_event wrappers ─────────────────────────────────────────────────

suite('wireContract: runner_event wrapper format', () => {
  const wrapperFixtures = [
    'runner_event_stream_delta.json',
    'runner_event_thinking.json',
    'runner_event_tool_use.json',
    'runner_event_tool_result.json',
    'runner_event_cost_update.json',
    'runner_event_permission_requested.json',
    'runner_event_completed.json',
  ];

  for (const name of wrapperFixtures) {
    test(`${name}: outer type is runner_event, inner event has a type field`, () => {
      const ev = loadFixture(name) as Record<string, unknown>;
      assert.strictEqual(ev['type'], 'runner_event', `${name}: outer type must be runner_event`);
      const inner = ev['event'] as Record<string, unknown>;
      assert.ok(inner && typeof inner === 'object', `${name}: event field must be an object`);
      assert.strictEqual(typeof inner['type'], 'string', `${name}: inner event must have a string type`);
    });
  }

  test('runner_event(stream_delta): inner text is a string', () => {
    const ev = loadFixture('runner_event_stream_delta.json') as Record<string, unknown>;
    const inner = ev['event'] as Record<string, unknown>;
    assert.strictEqual(inner['type'], 'stream_delta');
    assert.strictEqual(typeof inner['text'], 'string');
  });

  test('runner_event(stream_delta) maps to RunnerEvent stream_delta', () => {
    const ev = asAilEvent('runner_event_stream_delta.json');
    const mapped = mapAilEvent(ev);
    assert.ok(mapped !== undefined);
    assert.strictEqual(mapped!.type, 'stream_delta');
    if (mapped!.type === 'stream_delta') {
      assert.strictEqual(mapped.text, 'Hello, world!');
    }
  });

  test('runner_event(thinking): inner text is a string', () => {
    const ev = loadFixture('runner_event_thinking.json') as Record<string, unknown>;
    const inner = ev['event'] as Record<string, unknown>;
    assert.strictEqual(inner['type'], 'thinking');
    assert.strictEqual(typeof inner['text'], 'string');
  });

  test('runner_event(thinking) is not mapped (application layer ignores it)', () => {
    const ev = asAilEvent('runner_event_thinking.json');
    assert.strictEqual(mapAilEvent(ev), undefined);
  });

  test('runner_event(tool_use): tool_name is a string', () => {
    const ev = loadFixture('runner_event_tool_use.json') as Record<string, unknown>;
    const inner = ev['event'] as Record<string, unknown>;
    assert.strictEqual(inner['type'], 'tool_use');
    assert.strictEqual(inner['tool_name'], 'Bash');
  });

  test('runner_event(tool_result): tool_name is a string', () => {
    const ev = loadFixture('runner_event_tool_result.json') as Record<string, unknown>;
    const inner = ev['event'] as Record<string, unknown>;
    assert.strictEqual(inner['type'], 'tool_result');
    assert.strictEqual(inner['tool_name'], 'Bash');
  });

  test('runner_event(cost_update): cost_usd, input_tokens, output_tokens are numbers', () => {
    const ev = loadFixture('runner_event_cost_update.json') as Record<string, unknown>;
    const inner = ev['event'] as Record<string, unknown>;
    assert.strictEqual(inner['type'], 'cost_update');
    assert.strictEqual(typeof inner['cost_usd'], 'number');
    assert.strictEqual(typeof inner['input_tokens'], 'number');
    assert.strictEqual(typeof inner['output_tokens'], 'number');
  });

  test('runner_event(permission_requested): display_name and display_detail are strings', () => {
    const ev = loadFixture('runner_event_permission_requested.json') as Record<string, unknown>;
    const inner = ev['event'] as Record<string, unknown>;
    assert.strictEqual(inner['type'], 'permission_requested');
    assert.strictEqual(inner['display_name'], 'Bash');
    assert.strictEqual(typeof inner['display_detail'], 'string');
  });

  test('runner_event(completed): required fields present with correct types', () => {
    const ev = loadFixture('runner_event_completed.json') as Record<string, unknown>;
    const inner = ev['event'] as Record<string, unknown>;
    assert.strictEqual(inner['type'], 'completed');
    assert.strictEqual(typeof inner['response'], 'string');
    assert.ok(inner['cost_usd'] === null || typeof inner['cost_usd'] === 'number');
    assert.ok(inner['session_id'] === null || typeof inner['session_id'] === 'string');
  });
});

// ── pipeline_completed ────────────────────────────────────────────────────

suite('wireContract: pipeline_completed', () => {
  test('pipeline_completed fixture has outcome field', () => {
    const ev = loadFixture('pipeline_completed.json') as Record<string, unknown>;
    assert.strictEqual(ev['type'], 'pipeline_completed');
    assert.strictEqual(ev['outcome'], 'completed');
  });

  test('pipeline_completed_break has outcome=break and step_id', () => {
    const ev = loadFixture('pipeline_completed_break.json') as Record<string, unknown>;
    assert.strictEqual(ev['type'], 'pipeline_completed');
    assert.strictEqual(ev['outcome'], 'break');
    assert.strictEqual(typeof ev['step_id'], 'string');
  });

  test('pipeline_completed maps to RunnerEvent pipeline_completed', () => {
    const ev = asAilEvent('pipeline_completed.json');
    const mapped = mapAilEvent(ev);
    assert.ok(mapped !== undefined);
    assert.strictEqual(mapped!.type, 'pipeline_completed');
  });
});

// ── pipeline_error ────────────────────────────────────────────────────────

suite('wireContract: pipeline_error', () => {
  test('pipeline_error fixture has error and error_type fields', () => {
    const ev = loadFixture('pipeline_error.json') as Record<string, unknown>;
    assert.strictEqual(ev['type'], 'pipeline_error');
    assert.strictEqual(typeof ev['error'], 'string');
    assert.strictEqual(typeof ev['error_type'], 'string');
  });

  test('pipeline_error maps to RunnerEvent error with message=error field', () => {
    const ev = asAilEvent('pipeline_error.json');
    const mapped = mapAilEvent(ev);
    assert.ok(mapped !== undefined);
    assert.strictEqual(mapped!.type, 'error');
    if (mapped!.type === 'error') {
      assert.strictEqual(typeof mapped.message, 'string');
    }
  });
});
