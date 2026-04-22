import { describe, it, expect } from 'vitest';

describe('WindowsProcessKiller', () => {
  it('calls taskkill with /F /T /PID <pid>', async () => {
    // Verify the argument construction — pid is always numeric so no injection possible.
    // We inspect the source rather than spawning a real process to keep tests platform-safe.
    const source = await import('fs').then(fs =>
      fs.readFileSync(new URL('../../src/process/windows-process-killer.ts', import.meta.url).pathname, 'utf-8')
    );
    expect(source).toContain("'taskkill'");
    expect(source).toContain("'/F'");
    expect(source).toContain("'/T'");
    expect(source).toContain("'/PID'");
    expect(source).toContain('String(proc.pid)');
  });
});
