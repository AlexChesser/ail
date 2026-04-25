import { ProcessKiller } from './killer';
import { PosixProcessKiller } from './posix/killer';
import { WindowsProcessKiller } from './win/killer';

export function createProcessKiller(): ProcessKiller {
  return process.platform === 'win32'
    ? new WindowsProcessKiller()
    : new PosixProcessKiller();
}
