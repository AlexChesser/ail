import { ChildProcess } from 'child_process';
import { ProcessKiller } from './process-killer';

export class PosixProcessKiller implements ProcessKiller {
  kill(proc: ChildProcess): Promise<void> {
    return new Promise((resolve) => {
      proc.kill('SIGTERM');
      const timeout = setTimeout(() => {
        if (!proc.killed) proc.kill('SIGKILL');
        resolve();
      }, 5000);
      proc.once('close', () => {
        clearTimeout(timeout);
        resolve();
      });
    });
  }
}
