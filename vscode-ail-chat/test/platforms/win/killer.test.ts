import { describe, it, expect } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';

describe('WindowsProcessKiller', () => {
  it('calls taskkill with /F /T /PID <pid>', () => {
    const source = fs.readFileSync(
      path.join(__dirname, '../../../src/platforms/win/killer.ts'),
      'utf-8'
    );
    expect(source).toContain("'taskkill'");
    expect(source).toContain("'/F'");
    expect(source).toContain("'/T'");
    expect(source).toContain("'/PID'");
    expect(source).toContain('String(proc.pid)');
  });
});
