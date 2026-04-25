import { ProcessKiller, BinaryResolver, BinaryInstaller } from './types';
import { PosixProcessKiller } from './posix/killer';
import { PosixBinaryResolver } from './posix/binary';
import { PosixBinaryInstaller } from './posix/installer';
import { WindowsProcessKiller } from './win/killer';
import { WindowsBinaryResolver } from './win/binary';
import { WindowsBinaryInstaller } from './win/installer';

export type { ProcessKiller, BinaryResolver, BinaryInstaller };
export { PosixProcessKiller, WindowsProcessKiller };

export interface Platform {
  killer: ProcessKiller;
  binary: BinaryResolver;
  installer: BinaryInstaller;
}

export function createPlatform(): Platform {
  if (process.platform === 'win32') {
    return {
      killer: new WindowsProcessKiller(),
      binary: new WindowsBinaryResolver(),
      installer: new WindowsBinaryInstaller(),
    };
  }
  return {
    killer: new PosixProcessKiller(),
    binary: new PosixBinaryResolver(),
    installer: new PosixBinaryInstaller(),
  };
}

export function createProcessKiller(): ProcessKiller {
  return createPlatform().killer;
}

export function createBinaryInstaller(): BinaryInstaller {
  return createPlatform().installer;
}
