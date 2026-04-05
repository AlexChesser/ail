/**
 * AilLogStream — subprocess wrapper for `ail log --follow` streaming.
 *
 * Spawns `ail log --follow <runId>` as a child process and emits line-by-line
 * updates via a callback. Auto-stops when the process exits (binary exits after
 * final step completes). Handles process cleanup via dispose().
 *
 * The Rust binary handles all polling and retries; this class is a passive consumer
 * of stdout.
 */

import { spawn, ChildProcess } from 'child_process';

export class AilLogStream {
  private readonly _runId: string;
  private readonly _binaryPath: string;
  private readonly _cwd: string | undefined;
  private readonly _onNewLine: (line: string) => void;
  private readonly _spawnFn: typeof spawn;
  private _process: ChildProcess | undefined;

  /**
   * Create a new AilLogStream.
   *
   * @param runId Run ID to follow
   * @param binaryPath Full path to `ail` binary
   * @param onNewLine Callback fired for each new line of output
   * @param cwd Optional working directory for the spawned process
   * @param spawnFn Optional spawn function (for testing); defaults to child_process.spawn
   */
  constructor(
    runId: string,
    binaryPath: string,
    onNewLine: (line: string) => void,
    cwd?: string,
    spawnFn: typeof spawn = spawn
  ) {
    this._runId = runId;
    this._binaryPath = binaryPath;
    this._onNewLine = onNewLine;
    this._cwd = cwd;
    this._spawnFn = spawnFn;
  }

  /**
   * Start the follow stream.
   *
   * Spawns `ail log --follow <runId> --format markdown` and begins reading
   * lines from stdout. Calls onNewLine() for each line as it arrives.
   *
   * Resolves when the process exits cleanly (code 0 = success, code 1+ = error).
   * Rejects on spawn failure or stream error.
   *
   * @returns Promise that resolves when process exits
   */
  async start(): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      try {
        this._process = this._spawnFn(this._binaryPath, ['log', '--follow', this._runId, '--format', 'markdown'], {
          cwd: this._cwd,
        });

        // Line-buffer stdout manually (mirrors ndjson.ts; avoids readline.Interface dependency)
        let buf = '';
        this._process.stdout?.setEncoding?.('utf-8');
        this._process.stdout?.on('data', (chunk: Buffer | string) => {
          buf += typeof chunk === 'string' ? chunk : chunk.toString('utf-8');
          const lines = buf.split('\n');
          buf = lines.pop() ?? '';
          for (const line of lines) {
            this._onNewLine(line);
          }
        });

        // Consume stderr silently (errors are logged to stderr by the binary)
        this._process.stderr?.resume();

        // Handle process exit
        this._process.on('close', (code: number | null) => {
          // Flush any remaining buffered content
          if (buf) {
            this._onNewLine(buf);
            buf = '';
          }
          if (code === 1) {
            // Error exit code from binary (e.g., run not found, DB error)
            reject(new Error(`ail log --follow exited with code ${code}`));
          } else if (code !== 0 && code !== null) {
            reject(new Error(`ail log --follow exited with code ${code}`));
          } else {
            // Code 0 or null (success or process was killed by dispose)
            resolve();
          }
        });

        // Handle spawn error
        this._process.on('error', (err: Error) => {
          reject(new Error(`Failed to spawn ail log --follow: ${err.message}`));
        });
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        reject(new Error(`Failed to start AilLogStream: ${message}`));
      }
    });
  }

  /**
   * Dispose of this stream and kill the child process.
   *
   * Safe to call multiple times or after process has already exited.
   */
  dispose(): void {
    if (this._process) {
      this._process.kill('SIGTERM');
      // Hard kill after 2 seconds if still running
      const timeout = setTimeout(() => {
        if (this._process) {
          this._process.kill('SIGKILL');
        }
      }, 2000);
      this._process.once('close', () => {
        clearTimeout(timeout);
      });
    }
  }
}
