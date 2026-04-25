import { execFile } from 'child_process';
import { promisify } from 'util';

const execFileAsync = promisify(execFile);

let cached: boolean | undefined;

export async function isAilOnPath(): Promise<boolean> {
  if (cached !== undefined) return cached;
  try {
    const cmd = process.platform === 'win32' ? 'where' : 'which';
    const arg = process.platform === 'win32' ? 'ail.exe' : 'ail';
    const { stdout } = await execFileAsync(cmd, [arg]);
    cached = stdout.trim().length > 0;
  } catch {
    cached = false;
  }
  return cached;
}

export function clearPathCache(): void {
  cached = undefined;
}
