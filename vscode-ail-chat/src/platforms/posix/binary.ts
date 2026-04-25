import { BinaryResolver } from '../types';

export class PosixBinaryResolver implements BinaryResolver {
  bundledBinaryName(triple: string): string {
    return `ail-${triple}`;
  }
  pathBinaryName(): string {
    return 'ail';
  }
}
