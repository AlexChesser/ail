import { ChildProcess } from 'child_process';

export interface ProcessKiller {
  kill(proc: ChildProcess): Promise<void>;
}

export interface BinaryResolver {
  bundledBinaryName(triple: string): string;
  pathBinaryName(): string;
}
