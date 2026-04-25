import { describe, it, expect } from 'vitest';
import { PosixBinaryResolver } from '../../../src/platforms/posix/binary';

describe('PosixBinaryResolver', () => {
  it('returns bundled binary name without .exe', () => {
    const resolver = new PosixBinaryResolver();
    expect(resolver.bundledBinaryName('aarch64-apple-darwin')).toBe('ail-aarch64-apple-darwin');
    expect(resolver.bundledBinaryName('x86_64-unknown-linux-musl')).toBe('ail-x86_64-unknown-linux-musl');
  });

  it('returns ail as path binary name', () => {
    const resolver = new PosixBinaryResolver();
    expect(resolver.pathBinaryName()).toBe('ail');
  });
});
