import * as fs from 'fs';
import * as path from 'path';
import { BinaryInstaller } from '../types';

export class WindowsBinaryInstaller implements BinaryInstaller {
  readonly targetLabel = '%LOCALAPPDATA%\\ail\\bin';

  async install(bundledBinaryPath: string): Promise<{ path: string; message: string }> {
    // Use LOCALAPPDATA if available, otherwise fall back to user home
    const appDataPath = process.env.LOCALAPPDATA || path.join(process.env.USERPROFILE || '', 'AppData', 'Local');
    const binDir = path.join(appDataPath, 'ail', 'bin');
    const targetPath = path.join(binDir, 'ail.exe');

    // Create directory if it doesn't exist
    if (!fs.existsSync(binDir)) {
      fs.mkdirSync(binDir, { recursive: true });
    }

    // Copy bundled binary to target
    fs.copyFileSync(bundledBinaryPath, targetPath);

    const message =
      `ail installed to ${targetPath}\n\n` +
      `Add the following to your PATH environment variable:\n` +
      `${binDir}\n\n` +
      `Then restart your terminal to use 'ail' from the command line.`;

    return { path: targetPath, message };
  }
}
