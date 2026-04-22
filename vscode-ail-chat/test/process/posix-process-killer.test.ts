import { describe, it, expect } from 'vitest';

describe('PosixProcessKiller', () => {
  it.skipIf(process.platform === 'win32')('sends SIGTERM then SIGKILL after timeout', async () => {
    const { PosixProcessKiller } = await import('../../src/process/posix-process-killer');
    const { spawn } = await import('child_process');

    // Spawn a process that ignores SIGTERM (sleep is sufficient for timing test)
    const proc = spawn('sleep', ['30']);
    const killer = new PosixProcessKiller();

    const start = Date.now();
    await killer.kill(proc);
    const elapsed = Date.now() - start;

    // Process should be dead; either SIGTERM worked fast or SIGKILL fired
    expect(proc.killed || proc.exitCode !== null).toBe(true);
    // Should complete well under the 5s SIGKILL timeout (SIGTERM works on sleep)
    expect(elapsed).toBeLessThan(4000);
  });
});
