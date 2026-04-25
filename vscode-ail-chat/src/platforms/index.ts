import { ProcessKiller, BinaryResolver } from './types';
import { PosixProcessKiller } from './posix/killer';
import { PosixBinaryResolver } from './posix/binary';
import { WindowsProcessKiller } from './win/killer';
import { WindowsBinaryResolver } from './win/binary';

export type { ProcessKiller, BinaryResolver };
export { PosixProcessKiller, WindowsProcessKiller };

export interface Platform {
  killer: ProcessKiller;
  binary: BinaryResolver;
}

export function createPlatform(): Platform {
  if (process.platform === 'win32') {
    return {
      killer: new WindowsProcessKiller(),
      binary: new WindowsBinaryResolver(),
    };
  }
  return {
    killer: new PosixProcessKiller(),
    binary: new PosixBinaryResolver(),
  };
}

export function createProcessKiller(): ProcessKiller {
  return createPlatform().killer;
}
