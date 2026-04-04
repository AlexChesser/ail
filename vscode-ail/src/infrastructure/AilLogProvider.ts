/**
 * AilLogProvider — virtual document provider for ail-log format.
 *
 * Implements vscode.TextDocumentContentProvider to serve virtual documents
 * with URI scheme 'ail-log://'. When a user opens an ail-log document,
 * VS Code calls provideTextDocumentContent() with the URI, which extracts
 * the run_id and spawns `ail log [run_id] --format markdown` to fetch
 * the formatted log output.
 *
 * For in-progress runs, uses AilLogStream to spawn `ail log --follow` subprocess
 * and streams new content incrementally via _onDidChange events.
 *
 * This is the single content provider for all ail-log virtual documents.
 *
 * **Design: Rust binary is the single source of truth (Issue #39 D1)**
 *
 * The extension has no TypeScript formatter. All formatting is delegated to the Rust
 * binary via AilProcess.log(), which spawns `ail log --format markdown`. This design:
 *
 * 1. Ensures consistent output across all consumers (CLI, extension, future TUIs)
 * 2. Reduces maintenance burden (no duplicate formatter logic)
 * 3. Keeps the extension thin: receive stdout, render it, display it
 * 4. Makes it easier to keep the spec and implementation in sync (spec/runner/r04)
 *
 * If the binary is unavailable (v1 edge case), _formatError() returns a fallback
 * error message; there is no fallback formatter. The user is directed to check
 * the binary installation.
 */

import * as vscode from 'vscode';
import { AilProcess } from './AilProcess';
import { AilLogStream } from './AilLogStream';

export class AilLogProvider implements vscode.TextDocumentContentProvider {
  private readonly _ailProcess: AilProcess;
  private readonly _onDidChange = new vscode.EventEmitter<vscode.Uri>();
  private readonly _streams = new Map<string, AilLogStream>();
  private readonly _contentCache = new Map<string, string[]>();
  private _binaryPath: string;
  private _cwd: string | undefined;

  readonly onDidChange = this._onDidChange.event;

  constructor(ailProcess: AilProcess, binaryPath = '', cwd?: string) {
    this._ailProcess = ailProcess;
    this._binaryPath = binaryPath;
    this._cwd = cwd;
  }

  async provideTextDocumentContent(uri: vscode.Uri): Promise<string> {
    const runId = this._extractRunId(uri);

    try {
      // Check if stream is already active for this run
      if (runId && this._streams.has(runId)) {
        // Stream is active; return cached content
        const lines = this._contentCache.get(runId) || [];
        return this._transformDirectivesToHtml(lines.join('\n'));
      }

      // Fetch initial content one-shot first (to determine if run is in-progress)
      const content = await this._ailProcess.log(runId);

      if (runId) {
        // Initialize content cache
        const lines = content.split('\n');
        this._contentCache.set(runId, lines);

        // Check if run appears to be in-progress (heuristic: no terminating "completed" indicator)
        // If the run is not marked as completed, start live tail
        if (!content.includes('## Turn') || this._isProbablyInProgress(content)) {
          this._startLiveStream(uri, runId);
        }
      }

      return this._transformDirectivesToHtml(content);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return this._formatError(message);
    }
  }

  /**
   * Heuristic to detect if a run is still in progress.
   * A completed run typically has multiple turns or explicit completion markers.
   * This is a simple check; in v2 we could add a status field to ail-log/1 format.
   */
  private _isProbablyInProgress(content: string): boolean {
    // If content contains a specific in-progress marker, use it
    // For now, a simple heuristic: if the last non-empty line doesn't look like
    // a completion marker, assume it's still running
    const lines = content.trim().split('\n').filter((l) => l.trim());
    if (lines.length === 0) {
      return false;
    }
    const lastLine = lines[lines.length - 1];
    // If the last line looks like a turn header or response content (not a cost line),
    // it might still be in progress. For now, we'll be conservative and check for
    // step_completed event or similar.
    return !lastLine.includes('_Cost:');
  }

  /**
   * Start a live tail stream for an in-progress run.
   * Creates an AilLogStream, wires its output to _onDidChange events,
   * and debounces updates to max 1 emission per 300ms.
   */
  private _startLiveStream(uri: vscode.Uri, runId: string): void {
    if (this._streams.has(runId)) {
      return; // Stream already active
    }

    let debounceTimeout: NodeJS.Timeout | undefined;

    const onNewLine = (line: string) => {
      // Append line to cache
      const lines = this._contentCache.get(runId) || [];
      lines.push(line);
      this._contentCache.set(runId, lines);

      // Debounce _onDidChange to max 1 fire per 300ms
      if (debounceTimeout) {
        clearTimeout(debounceTimeout);
      }
      debounceTimeout = setTimeout(() => {
        this._onDidChange.fire(uri);
        debounceTimeout = undefined;
      }, 300);
    };

    const stream = new AilLogStream(runId, this._binaryPath, onNewLine, this._cwd);

    // Start the stream and handle completion
    stream.start().then(
      () => {
        // Stream completed successfully (process exited code 0)
        this._streams.delete(runId);
        this._onDidChange.fire(uri);
      },
      (err) => {
        // Stream errored (process exited code 1+)
        console.error(`[ail-log-stream] error for run ${runId}: ${(err as Error).message}`);
        this._streams.delete(runId);
        this._onDidChange.fire(uri);
      }
    );

    this._streams.set(runId, stream);
  }

  /**
   * Transform ail-log directive blocks to HTML <details> for collapsible rendering.
   *
   * Converts :::directive syntax to <details><summary> tags so VS Code's
   * Markdown preview renders them as collapsible sections.
   *
   * Transforms:
   * - :::thinking → <details><summary>Thinking</summary>
   * - :::tool-call → <details><summary>Tool Call</summary>
   * - :::tool-result → <details><summary>Tool Result</summary>
   * - :::stdio → <details><summary>Stdio</summary>
   * - closing ::: → </details>
   */
  private _transformDirectivesToHtml(content: string): string {
    return content
      .replace(/^:::thinking\n([\s\S]*?)\n:::/gm, '<details><summary>Thinking</summary>\n\n$1\n\n</details>')
      .replace(/^:::tool-call\n([\s\S]*?)\n:::/gm, '<details><summary>Tool Call</summary>\n\n$1\n\n</details>')
      .replace(/^:::tool-result\n([\s\S]*?)\n:::/gm, '<details><summary>Tool Result</summary>\n\n$1\n\n</details>')
      .replace(/^:::stdio\n([\s\S]*?)\n:::/gm, '<details><summary>Stdio</summary>\n\n$1\n\n</details>');
  }

  /**
   * Extract run_id from URI path.
   *
   * URI format: ail-log://{run_id}
   * If run_id is empty or missing, return undefined to fetch the latest run.
   */
  private _extractRunId(uri: vscode.Uri): string | undefined {
    const path = uri.path;
    // Remove leading slash if present
    const runId = path.startsWith('/') ? path.slice(1) : path;
    return runId || undefined;
  }

  /**
   * Format an error message as an ail-log/1 document.
   * Allows the error to be displayed in the log viewer.
   */
  private _formatError(message: string): string {
    return `ail-log/1

> [!ERROR]
> **Failed to load log:** ${message}
`;
  }

  /**
   * Notify subscribers that a document has changed.
   * Used by live tail mode (D2) to trigger updates when new content arrives.
   */
  notifyChange(uri: vscode.Uri): void {
    this._onDidChange.fire(uri);
  }

  dispose(): void {
    // Dispose all active streams
    for (const stream of this._streams.values()) {
      stream.dispose();
    }
    this._streams.clear();
    this._contentCache.clear();
    this._onDidChange.dispose();
  }
}
