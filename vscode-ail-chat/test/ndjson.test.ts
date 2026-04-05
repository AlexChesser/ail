/**
 * Tests for the NDJSON stream parser.
 */

import { describe, it, expect } from 'vitest';
import { Readable } from 'stream';
import { parseNdjsonStream } from '../src/ndjson';
import { AilEvent } from '../src/types';

function streamFrom(chunks: string[]): Readable {
  const r = new Readable({ read() {} });
  for (const c of chunks) r.push(c);
  r.push(null);
  return r;
}

function collect(chunks: string[]): Promise<AilEvent[]> {
  return new Promise((resolve) => {
    const events: AilEvent[] = [];
    const r = streamFrom(chunks);
    parseNdjsonStream(r, (e) => events.push(e));
    r.on('end', () => resolve(events));
  });
}

describe('parseNdjsonStream', () => {
  it('parses a single valid event', async () => {
    const events = await collect(['{"type":"run_started","run_id":"abc","pipeline_source":null,"total_steps":1}\n']);
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe('run_started');
  });

  it('parses multiple events in one chunk', async () => {
    const events = await collect([
      '{"type":"run_started","run_id":"x","pipeline_source":null,"total_steps":2}\n' +
      '{"type":"pipeline_completed","outcome":"completed"}\n',
    ]);
    expect(events).toHaveLength(2);
    expect(events[0].type).toBe('run_started');
    expect(events[1].type).toBe('pipeline_completed');
  });

  it('handles partial lines split across chunks', async () => {
    const events = await collect([
      '{"type":"run_sta',
      'rted","run_id":"abc","pipeline_source":null,"total_steps":0}\n',
    ]);
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe('run_started');
  });

  it('skips empty lines', async () => {
    const events = await collect(['\n\n{"type":"pipeline_completed","outcome":"completed"}\n\n']);
    expect(events).toHaveLength(1);
  });

  it('skips malformed JSON without throwing', async () => {
    const events = await collect(['not-json\n{"type":"pipeline_completed","outcome":"completed"}\n']);
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe('pipeline_completed');
  });

  it('parses all events from simple-prompt fixture', async () => {
    const fs = await import('fs');
    const path = await import('path');
    const fixturePath = path.join(__dirname, 'fixtures', 'simple-prompt.ndjson');
    const content = fs.readFileSync(fixturePath, 'utf-8');
    const events = await collect([content]);
    expect(events.length).toBeGreaterThan(0);
    expect(events[0].type).toBe('run_started');
    expect(events[events.length - 1].type).toBe('pipeline_completed');
  });

  it('parses multi-step fixture and emits correct step sequence', async () => {
    const fs = await import('fs');
    const path = await import('path');
    const fixturePath = path.join(__dirname, 'fixtures', 'multi-step-pipeline.ndjson');
    const content = fs.readFileSync(fixturePath, 'utf-8');
    const events = await collect([content]);
    const stepStarted = events.filter((e) => e.type === 'step_started');
    const stepCompleted = events.filter((e) => e.type === 'step_completed');
    expect(stepStarted).toHaveLength(3);
    expect(stepCompleted).toHaveLength(3);
  });

  it('parses malformed-lines fixture: skips 2 bad lines, still gets valid events', async () => {
    const fs = await import('fs');
    const path = await import('path');
    const fixturePath = path.join(__dirname, 'fixtures', 'malformed-lines.ndjson');
    const content = fs.readFileSync(fixturePath, 'utf-8');
    const events = await collect([content]);
    // 4 valid lines: run_started, step_started, runner_event(stream_delta), step_completed, pipeline_completed
    expect(events.filter((e) => e.type === 'run_started')).toHaveLength(1);
    expect(events.filter((e) => e.type === 'pipeline_completed')).toHaveLength(1);
  });
});
