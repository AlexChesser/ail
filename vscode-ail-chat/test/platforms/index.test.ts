import { describe, it, expect } from 'vitest';
import { createProcessKiller } from '../../src/platforms/index';
import { PosixProcessKiller } from '../../src/platforms/posix/killer';
import { WindowsProcessKiller } from '../../src/platforms/win/killer';

describe('createProcessKiller', () => {
  it('returns WindowsProcessKiller on win32', () => {
    const original = process.platform;
    Object.defineProperty(process, 'platform', { value: 'win32', configurable: true });
    const killer = createProcessKiller();
    expect(killer).toBeInstanceOf(WindowsProcessKiller);
    Object.defineProperty(process, 'platform', { value: original, configurable: true });
  });

  it('returns PosixProcessKiller on linux', () => {
    const original = process.platform;
    Object.defineProperty(process, 'platform', { value: 'linux', configurable: true });
    const killer = createProcessKiller();
    expect(killer).toBeInstanceOf(PosixProcessKiller);
    Object.defineProperty(process, 'platform', { value: original, configurable: true });
  });
});
