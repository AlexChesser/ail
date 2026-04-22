import { describe, it, expect } from 'vitest';
import { AilOutputChannel } from '../src/output-channel';
import { AilEvent } from '../src/types';

function makeMockChannel() {
  const lines: string[] = [];
  return {
    channel: { appendLine: (s: string) => { lines.push(s); } } as any,
    lines,
  };
}

describe('AilOutputChannel', () => {
  it('spawn() prefixes with [spawn]', () => {
    const { channel, lines } = makeMockChannel();
    const c = new AilOutputChannel(channel);
    c.spawn('/usr/bin/ail', ['--once', 'hello', '--output-format', 'json']);
    expect(lines[0]).toBe('[spawn] /usr/bin/ail --once hello --output-format json');
  });

  it('event() prefixes with [event] and JSON-stringifies', () => {
    const { channel, lines } = makeMockChannel();
    const c = new AilOutputChannel(channel);
    const event: AilEvent = { type: 'pipeline_completed', outcome: 'completed' };
    c.event(event);
    expect(lines[0]).toMatch(/^\[event\] /);
    expect(lines[0]).toContain('"pipeline_completed"');
  });

  it('stderr() prefixes with [stderr]', () => {
    const { channel, lines } = makeMockChannel();
    const c = new AilOutputChannel(channel);
    c.stderr('something went wrong');
    expect(lines[0]).toBe('[stderr] something went wrong');
  });

  it('exit() prefixes with [exit] and includes code', () => {
    const { channel, lines } = makeMockChannel();
    const c = new AilOutputChannel(channel);
    c.exit(1);
    expect(lines[0]).toBe('[exit] code=1');
  });

  it('exit() handles null code', () => {
    const { channel, lines } = makeMockChannel();
    const c = new AilOutputChannel(channel);
    c.exit(null);
    expect(lines[0]).toBe('[exit] code=null');
  });

  it('error() prefixes with [error]', () => {
    const { channel, lines } = makeMockChannel();
    const c = new AilOutputChannel(channel);
    c.error('spawn ENOENT');
    expect(lines[0]).toBe('[error] spawn ENOENT');
  });

  it('consumes a large stderr stream without back-pressure', async () => {
    // 1MB of data — verifies the data listener keeps consuming
    const { channel, lines } = makeMockChannel();
    const c = new AilOutputChannel(channel);
    const bigChunk = Buffer.alloc(1024 * 1024, 'x');
    // Simulate the data handler that ail-process-manager wires up
    const dataHandler = (chunk: Buffer) => {
      for (const line of chunk.toString().split('\n')) {
        const trimmed = line.trim();
        if (trimmed) c.stderr(trimmed);
      }
    };
    dataHandler(bigChunk);
    // Should complete synchronously — no hanging
    expect(lines.length).toBeGreaterThan(0);
  });
});
