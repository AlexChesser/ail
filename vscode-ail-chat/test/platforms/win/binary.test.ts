import { describe, it, expect } from 'vitest';
import { WindowsBinaryResolver } from '../../../src/platforms/win/binary';

describe('WindowsBinaryResolver', () => {
  it('returns bundled binary name with .exe suffix', () => {
    const resolver = new WindowsBinaryResolver();
    expect(resolver.bundledBinaryName('x86_64-pc-windows-msvc')).toBe('ail-x86_64-pc-windows-msvc.exe');
  });

  it('returns ail.exe as path binary name', () => {
    const resolver = new WindowsBinaryResolver();
    expect(resolver.pathBinaryName()).toBe('ail.exe');
  });
});
