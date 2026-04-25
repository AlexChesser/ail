import { ChildProcess } from 'child_process';

export interface ProcessKiller {
  kill(proc: ChildProcess): Promise<void>;
}

export interface BinaryResolver {
  bundledBinaryName(triple: string): string;
  pathBinaryName(): string;
}

export interface BinaryInstaller {
  install(bundledBinaryPath: string): Promise<{ path: string; message: string }>;
  readonly targetLabel: string;
}
