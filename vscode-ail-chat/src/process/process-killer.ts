import { ChildProcess } from 'child_process';

export interface ProcessKiller {
  /** Terminate the process and its descendants. */
  kill(proc: ChildProcess): Promise<void>;
}
