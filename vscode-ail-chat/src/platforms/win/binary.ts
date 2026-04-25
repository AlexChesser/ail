import { BinaryResolver } from '../types';

export class WindowsBinaryResolver implements BinaryResolver {
  bundledBinaryName(triple: string): string {
    return `ail-${triple}.exe`;
  }
  pathBinaryName(): string {
    return 'ail.exe';
  }
}
