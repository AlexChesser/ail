import * as fs from 'fs';
import * as path from 'path';
import { homedir } from 'os';
import { BinaryInstaller } from '../types';

export class PosixBinaryInstaller implements BinaryInstaller {
  readonly targetLabel = '~/.local/bin';

  async install(bundledBinaryPath: string): Promise<{ path: string; message: string }> {
    const binDir = path.join(homedir(), '.local', 'bin');
    const targetPath = path.join(binDir, 'ail');

    // Create ~/.local/bin if it doesn't exist
    if (!fs.existsSync(binDir)) {
      fs.mkdirSync(binDir, { recursive: true, mode: 0o755 });
    }

    // Copy bundled binary to target
    fs.copyFileSync(bundledBinaryPath, targetPath);

    // Make it executable
    fs.chmodSync(targetPath, 0o755);

    const message =
      `ail installed to ${targetPath}\n\n` +
      `Add ~/.local/bin to your PATH if needed:\n` +
      `export PATH="$HOME/.local/bin:$PATH"`;

    return { path: targetPath, message };
  }
}
