import * as assert from 'assert';
import { mapAilEvent } from '../../application/mapEvent';
import { AilEvent } from '../../types';

suite('mapAilEvent', () => {
  test('step_started maps to RunnerEvent with renamed fields', () => {
    const event: AilEvent = {
      type: 'step_started',
      step_id: 'review',
      step_index: 0,
      total_steps: 3,
      resolved_prompt: 'hello',
    };
    const result = mapAilEvent(event);
    assert.deepStrictEqual(result, {
      type: 'step_started',
      stepId: 'review',
      stepIndex: 0,
      totalSteps: 3,
    });
  });

  test('step_completed maps correctly', () => {
    const event: AilEvent = {
      type: 'step_completed',
      step_id: 'review',
      cost_usd: 0.01,
      input_tokens: 100,
      output_tokens: 50,
      response: null,
    };
    const result = mapAilEvent(event);
    assert.deepStrictEqual(result, { type: 'step_completed', stepId: 'review' });
  });

  test('step_skipped maps correctly', () => {
    const event: AilEvent = { type: 'step_skipped', step_id: 'optional' };
    assert.deepStrictEqual(mapAilEvent(event), { type: 'step_skipped', stepId: 'optional' });
  });

  test('step_failed maps with error string', () => {
    const event: AilEvent = { type: 'step_failed', step_id: 'deploy', error: 'timeout' };
    assert.deepStrictEqual(mapAilEvent(event), {
      type: 'step_failed',
      stepId: 'deploy',
      error: 'timeout',
    });
  });

  test('hitl_gate_reached maps correctly', () => {
    const event: AilEvent = { type: 'hitl_gate_reached', step_id: 'gate' };
    assert.deepStrictEqual(mapAilEvent(event), { type: 'hitl_gate_reached', stepId: 'gate' });
  });

  test('runner_event with stream_delta maps to stream_delta', () => {
    const event: AilEvent = {
      type: 'runner_event',
      event: { type: 'stream_delta', text: 'hello' },
    };
    assert.deepStrictEqual(mapAilEvent(event), { type: 'stream_delta', text: 'hello' });
  });

  test('runner_event with cost_update maps to cost_update', () => {
    const event: AilEvent = {
      type: 'runner_event',
      event: { type: 'cost_update', cost_usd: 0.05, input_tokens: 1000, output_tokens: 500 },
    };
    assert.deepStrictEqual(mapAilEvent(event), {
      type: 'cost_update',
      costUsd: 0.05,
      inputTokens: 1000,
      outputTokens: 500,
    });
  });

  test('runner_event with thinking returns undefined', () => {
    const event: AilEvent = {
      type: 'runner_event',
      event: { type: 'thinking', text: 'thinking...' },
    };
    assert.strictEqual(mapAilEvent(event), undefined);
  });

  test('runner_event with tool_use returns undefined', () => {
    const event: AilEvent = {
      type: 'runner_event',
      event: { type: 'tool_use', tool_name: 'Bash' },
    };
    assert.strictEqual(mapAilEvent(event), undefined);
  });

  test('pipeline_completed maps correctly', () => {
    const event: AilEvent = { type: 'pipeline_completed', outcome: 'completed' };
    assert.deepStrictEqual(mapAilEvent(event), { type: 'pipeline_completed' });
  });

  test('pipeline_error maps to error with message', () => {
    const event: AilEvent = {
      type: 'pipeline_error',
      error: 'Step failed',
      error_type: 'ail:pipeline/aborted',
    };
    assert.deepStrictEqual(mapAilEvent(event), { type: 'error', message: 'Step failed' });
  });

  test('run_started returns undefined', () => {
    const event: AilEvent = {
      type: 'run_started',
      run_id: 'abc',
      pipeline_source: '.ail.yaml',
      total_steps: 2,
    };
    assert.strictEqual(mapAilEvent(event), undefined);
  });
});
