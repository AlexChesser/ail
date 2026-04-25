import { describe, it, expect } from 'vitest';

describe('PosixProcessKiller', () => {
  it.skipIf(process.platform === 'win32')('sends SIGTERM then SIGKILL after timeout', async () => {
    const { PosixProcessKiller } = await import('../../../src/process/posix/killer');
    const { spawn } = await import('child_process');

    const proc = spawn('sleep', ['30']);
    const killer = new PosixProcessKiller();

    const start = Date.now();
    await killer.kill(proc);
    const elapsed = Date.now() - start;

    expect(proc.killed || proc.exitCode !== null).toBe(true);
    expect(elapsed).toBeLessThan(4000);
  });
});
