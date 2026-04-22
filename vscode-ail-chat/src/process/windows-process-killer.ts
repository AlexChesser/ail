import { ChildProcess, spawn } from 'child_process';
import { ProcessKiller } from './process-killer';

export class WindowsProcessKiller implements ProcessKiller {
  kill(proc: ChildProcess): Promise<void> {
    return new Promise((resolve) => {
      if (proc.pid === undefined) { resolve(); return; }
      const killer = spawn('taskkill', ['/F', '/T', '/PID', String(proc.pid)]);
      killer.on('close', () => resolve());
      killer.on('error', () => resolve());
    });
  }
}
