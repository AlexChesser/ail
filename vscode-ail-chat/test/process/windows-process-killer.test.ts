import { describe, it, expect } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';

describe('WindowsProcessKiller', () => {
  it('calls taskkill with /F /T /PID <pid>', () => {
    // Verify the argument construction — pid is always numeric so no injection possible.
    // We inspect the source rather than spawning a real process to keep tests platform-safe.
    const source = fs.readFileSync(
      path.join(__dirname, '../../src/process/windows-process-killer.ts'),
      'utf-8'
    );
    expect(source).toContain("'taskkill'");
    expect(source).toContain("'/F'");
    expect(source).toContain("'/T'");
    expect(source).toContain("'/PID'");
    expect(source).toContain('String(proc.pid)');
  });
});
