/**
 * AilLogProvider — virtual document provider for ail-log format.
 *
 * Implements vscode.TextDocumentContentProvider to serve virtual documents
 * with URI scheme 'ail-log://'. When a user opens an ail-log document,
 * VS Code calls provideTextDocumentContent() with the URI, which extracts
 * the run_id and spawns `ail log [run_id] --format markdown` to fetch
 * the formatted log output.
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

export class AilLogProvider implements vscode.TextDocumentContentProvider {
  private readonly _ailProcess: AilProcess;
  private readonly _onDidChange = new vscode.EventEmitter<vscode.Uri>();

  readonly onDidChange = this._onDidChange.event;

  constructor(ailProcess: AilProcess) {
    this._ailProcess = ailProcess;
  }

  async provideTextDocumentContent(uri: vscode.Uri): Promise<string> {
    const runId = this._extractRunId(uri);

    try {
      const content = await this._ailProcess.log(runId);
      return content;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return this._formatError(message);
    }
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
    this._onDidChange.dispose();
  }
}
