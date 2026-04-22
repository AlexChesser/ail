/**
 * Tests for the AilProcessManager event mapping.
 * Tests mapAilEventToMessages (pure function) without spawning any process.
 */

import { describe, it, expect } from 'vitest';
import { mapAilEventToMessages, AilEventMapper, AilProcessManager } from '../src/ail-process-manager';
import { AilEvent } from '../src/types';
import * as fs from 'fs';
import * as path from 'path';
import { Readable } from 'stream';
import { parseNdjsonStream } from '../src/ndjson';

function loadFixture(name: string): AilEvent[] {
  const content = fs.readFileSync(path.join(__dirname, 'fixtures', name), 'utf-8');
  const events: AilEvent[] = [];
  // Parse synchronously line by line
  for (const line of content.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    try {
      events.push(JSON.parse(trimmed) as AilEvent);
    } catch {
      // skip malformed
    }
  }
  return events;
}

function fixtureToMessages(name: string) {
  const events = loadFixture(name);
  return events.flatMap(mapAilEventToMessages);
}

describe('mapAilEventToMessages', () => {
  it('maps run_started → runStarted', () => {
    const event: AilEvent = { type: 'run_started', run_id: 'r1', pipeline_source: null, total_steps: 2 };
    const msgs = mapAilEventToMessages(event);
    expect(msgs).toHaveLength(1);
    expect(msgs[0]).toEqual({ type: 'runStarted', runId: 'r1', totalSteps: 2 });
  });

  it('maps step_started → stepStarted', () => {
    const event: AilEvent = {
      type: 'step_started', step_id: 'invocation', step_index: 0, total_steps: 3,
      resolved_prompt: 'hello',
    };
    const msgs = mapAilEventToMessages(event);
    expect(msgs[0]).toMatchObject({ type: 'stepStarted', stepId: 'invocation', stepIndex: 0, totalSteps: 3 });
  });

  it('maps step_completed → stepCompleted with cost and tokens', () => {
    const event: AilEvent = {
      type: 'step_completed', step_id: 'invocation', cost_usd: 0.0012,
      input_tokens: 10, output_tokens: 8, response: 'hi',
    };
    const msgs = mapAilEventToMessages(event);
    expect(msgs[0]).toEqual({
      type: 'stepCompleted', stepId: 'invocation',
      costUsd: 0.0012, inputTokens: 10, outputTokens: 8, response: 'hi',
    });
  });

  it('maps step_skipped → stepSkipped', () => {
    const event: AilEvent = { type: 'step_skipped', step_id: 'check' };
    expect(mapAilEventToMessages(event)).toEqual([{ type: 'stepSkipped', stepId: 'check' }]);
  });

  it('maps step_failed → stepFailed', () => {
    const event: AilEvent = { type: 'step_failed', step_id: 'check', error: 'oops' };
    expect(mapAilEventToMessages(event)).toEqual([{ type: 'stepFailed', stepId: 'check', error: 'oops' }]);
  });

  it('maps hitl_gate_reached → hitlGate', () => {
    const event: AilEvent = { type: 'hitl_gate_reached', step_id: 'review', message: 'Please confirm' };
    const msgs = mapAilEventToMessages(event);
    expect(msgs[0]).toEqual({ type: 'hitlGate', stepId: 'review', message: 'Please confirm' });
  });

  it('maps pipeline_completed → pipelineCompleted', () => {
    const event: AilEvent = { type: 'pipeline_completed', outcome: 'completed' };
    expect(mapAilEventToMessages(event)).toEqual([{ type: 'pipelineCompleted' }]);
  });

  it('maps pipeline_error → pipelineError', () => {
    const event: AilEvent = { type: 'pipeline_error', error: 'not found', error_type: 'PIPELINE_NOT_FOUND' };
    expect(mapAilEventToMessages(event)).toEqual([{ type: 'pipelineError', error: 'not found' }]);
  });

  it('maps runner_event/stream_delta → streamDelta', () => {
    const event: AilEvent = { type: 'runner_event', event: { type: 'stream_delta', text: 'hello' } };
    expect(mapAilEventToMessages(event)).toEqual([{ type: 'streamDelta', text: 'hello' }]);
  });

  it('maps runner_event/thinking → thinking', () => {
    const event: AilEvent = { type: 'runner_event', event: { type: 'thinking', text: 'I think...' } };
    expect(mapAilEventToMessages(event)).toEqual([{ type: 'thinking', text: 'I think...' }]);
  });

  it('maps runner_event/tool_use → toolUse', () => {
    const event: AilEvent = {
      type: 'runner_event',
      event: { type: 'tool_use', tool_name: 'bash', tool_use_id: 'tu-1', input: { command: 'ls' } },
    };
    const msgs = mapAilEventToMessages(event);
    expect(msgs[0]).toMatchObject({ type: 'toolUse', toolName: 'bash', toolUseId: 'tu-1' });
    expect((msgs[0] as { input: unknown }).input).toEqual({ command: 'ls' });
  });

  it('maps runner_event/tool_result → toolResult', () => {
    const event: AilEvent = {
      type: 'runner_event',
      event: { type: 'tool_result', tool_name: 'bash', tool_use_id: 'tu-1', content: 'ok', is_error: false },
    };
    const msgs = mapAilEventToMessages(event);
    expect(msgs[0]).toEqual({ type: 'toolResult', toolUseId: 'tu-1', content: 'ok', isError: false });
  });

  it('maps runner_event/permission_requested → permissionRequested', () => {
    const event: AilEvent = {
      type: 'runner_event',
      event: { type: 'permission_requested', display_name: 'Delete files', display_detail: 'rm /tmp/foo' },
    };
    const msgs = mapAilEventToMessages(event);
    expect(msgs[0]).toEqual({
      type: 'permissionRequested', displayName: 'Delete files', displayDetail: 'rm /tmp/foo',
    });
  });

  it('returns empty array for unknown runner_event subtypes', () => {
    const event = {
      type: 'runner_event',
      event: { type: 'cost_update', cost_usd: 0.001, input_tokens: 5, output_tokens: 3 },
    } as AilEvent;
    expect(mapAilEventToMessages(event)).toHaveLength(0);
  });

  describe('fixture round-trips', () => {
    it('simple-prompt: produces runStarted, streamDelta, stepCompleted, pipelineCompleted', () => {
      const msgs = fixtureToMessages('simple-prompt.ndjson');
      const types = msgs.map((m) => m.type);
      expect(types).toContain('runStarted');
      expect(types).toContain('streamDelta');
      expect(types).toContain('stepCompleted');
      expect(types).toContain('pipelineCompleted');
    });

    it('tool-calls: produces toolUse and toolResult messages', () => {
      const msgs = fixtureToMessages('tool-calls.ndjson');
      expect(msgs.some((m) => m.type === 'toolUse')).toBe(true);
      expect(msgs.some((m) => m.type === 'toolResult')).toBe(true);
    });

    it('thinking-blocks: produces thinking messages', () => {
      const msgs = fixtureToMessages('thinking-blocks.ndjson');
      expect(msgs.some((m) => m.type === 'thinking')).toBe(true);
    });

    it('hitl-gate: produces hitlGate message', () => {
      const msgs = fixtureToMessages('hitl-gate.ndjson');
      expect(msgs.some((m) => m.type === 'hitlGate')).toBe(true);
    });

    it('permission-request: produces permissionRequested message', () => {
      const msgs = fixtureToMessages('permission-request.ndjson');
      expect(msgs.some((m) => m.type === 'permissionRequested')).toBe(true);
    });

    it('step-failure: produces stepFailed message', () => {
      const msgs = fixtureToMessages('step-failure.ndjson');
      expect(msgs.some((m) => m.type === 'stepFailed')).toBe(true);
    });

    it('pipeline-error: produces pipelineError message', () => {
      const msgs = fixtureToMessages('pipeline-error.ndjson');
      expect(msgs.some((m) => m.type === 'pipelineError')).toBe(true);
    });

    it('multi-step: produces 3 stepStarted and 3 stepCompleted messages', () => {
      const msgs = fixtureToMessages('multi-step-pipeline.ndjson');
      expect(msgs.filter((m) => m.type === 'stepStarted')).toHaveLength(3);
      expect(msgs.filter((m) => m.type === 'stepCompleted')).toHaveLength(3);
    });

    it('multi-step-realistic: handles interleaved tool calls and thinking blocks', () => {
      const msgs = fixtureToMessages('multi-step-realistic.ndjson');
      const stepStarteds = msgs.filter(m => m.type === 'stepStarted');
      expect(stepStarteds).toHaveLength(2);
      expect(msgs.filter(m => m.type === 'thinking')).toHaveLength(2);
      expect(msgs.some(m => m.type === 'toolUse')).toBe(true);
      expect(msgs.some(m => m.type === 'toolResult')).toBe(true);
      const deltas = msgs.filter(m => m.type === 'streamDelta');
      expect(deltas.length).toBeGreaterThanOrEqual(7);
    });
  });

  describe('CLAUDECODE env removal', () => {
    it('AilProcessManager does not pass CLAUDECODE to spawned process', () => {
      // We verify the logic statically: the start() method deletes CLAUDECODE.
      // Read the source and confirm the delete is present.
      const source = fs.readFileSync(
        path.join(__dirname, '..', 'src', 'ail-process-manager.ts'),
        'utf-8'
      );
      expect(source).toContain("delete env['CLAUDECODE']");
    });
  });
});

describe('AilEventMapper', () => {
  it('includes stepId in streamDelta messages', () => {
    const mapper = new AilEventMapper();

    const started: AilEvent = { type: 'step_started', step_id: 'review', step_index: 0, total_steps: 1, resolved_prompt: null };
    mapper.map(started);

    const delta: AilEvent = { type: 'runner_event', event: { type: 'stream_delta', text: 'hello' } };
    const msgs = mapper.map(delta);

    expect(msgs).toHaveLength(1);
    expect(msgs[0]).toMatchObject({ type: 'streamDelta', text: 'hello', stepId: 'review' });
  });

  it('resets stepId on run_started', () => {
    const mapper = new AilEventMapper();

    mapper.map({ type: 'step_started', step_id: 'old', step_index: 0, total_steps: 1, resolved_prompt: null } as AilEvent);
    mapper.map({ type: 'run_started', run_id: 'r2', pipeline_source: null, total_steps: 1 } as AilEvent);

    const msgs = mapper.map({ type: 'runner_event', event: { type: 'stream_delta', text: 'x' } } as AilEvent);
    expect(msgs[0]).toEqual({ type: 'streamDelta', text: 'x' });
    expect((msgs[0] as any).stepId).toBeUndefined();
  });

  it('updates stepId when step changes', () => {
    const mapper = new AilEventMapper();

    mapper.map({ type: 'step_started', step_id: 'step1', step_index: 0, total_steps: 2, resolved_prompt: null } as AilEvent);
    let msgs = mapper.map({ type: 'runner_event', event: { type: 'stream_delta', text: 'a' } } as AilEvent);
    expect(msgs[0]).toMatchObject({ stepId: 'step1' });

    mapper.map({ type: 'step_started', step_id: 'step2', step_index: 1, total_steps: 2, resolved_prompt: null } as AilEvent);
    msgs = mapper.map({ type: 'runner_event', event: { type: 'stream_delta', text: 'b' } } as AilEvent);
    expect(msgs[0]).toMatchObject({ stepId: 'step2' });
  });
});

describe('cancel() with StubProcessKiller', () => {
  it('clears _activeProcess immediately, before kill resolves', async () => {
    let killCalled = false;
    let resolveKill!: () => void;
    const stubKiller = {
      kill: (_proc: unknown) => {
        killCalled = true;
        return new Promise<void>(r => { resolveKill = r; });
      }
    };

    const manager = new AilProcessManager('/nonexistent', undefined, stubKiller as any);
    // Manually set _activeProcess via a cast to simulate a running process
    (manager as any)._activeProcess = { pid: 999, kill: () => {}, killed: false } as any;

    expect(manager.isRunning).toBe(true);
    manager.cancel();

    // Must be cleared immediately — before the kill promise resolves
    expect(manager.isRunning).toBe(false);
    expect(killCalled).toBe(true);

    // Resolve the kill — no error
    resolveKill();
  });

  it('start() succeeds after cancel() clears the pointer', async () => {
    const stubKiller = {
      kill: () => new Promise<void>(r => setTimeout(r, 100)), // slow kill
    };

    const manager = new AilProcessManager('/nonexistent', undefined, stubKiller as any);
    (manager as any)._activeProcess = { pid: 999, kill: () => {}, killed: false } as any;

    manager.cancel();
    expect(manager.isRunning).toBe(false);

    // start() should not throw "A pipeline is already running"
    // (it will fail for other reasons since /nonexistent doesn't exist, but not the guard)
    const startPromise = manager.start('hello');
    // The guard passes — error will be spawn error, not the guard
    await startPromise.catch((err: Error) => {
      expect(err.message).not.toContain('A pipeline is already running');
    });
  });
});
