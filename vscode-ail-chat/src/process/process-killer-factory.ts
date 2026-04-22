import { ProcessKiller } from './process-killer';
import { PosixProcessKiller } from './posix-process-killer';
import { WindowsProcessKiller } from './windows-process-killer';

export function createProcessKiller(): ProcessKiller {
  return process.platform === 'win32'
    ? new WindowsProcessKiller()
    : new PosixProcessKiller();
}
