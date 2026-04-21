/**
 * DiffContentProvider — supplies synthetic "before" and "after" file content
 * to VS Code's built-in diff editor for per-step file diffs (issue #23).
 *
 * Registered under the `ail-diff` URI scheme. Content is stored in memory;
 * callers use setContent(key, content) before opening a diff URI.
 *
 * Singleton: use DiffContentProvider.instance to obtain the shared instance.
 */

import * as vscode from 'vscode';

export const DIFF_SCHEME = 'ail-diff';

export class DiffContentProvider implements vscode.TextDocumentContentProvider {
  private static _instance: DiffContentProvider | undefined;

  private readonly _contents = new Map<string, string>();

  private constructor() {}

  /** Returns the shared singleton instance. */
  static get instance(): DiffContentProvider {
    if (!DiffContentProvider._instance) {
      DiffContentProvider._instance = new DiffContentProvider();
    }
    return DiffContentProvider._instance;
  }

  /**
   * Store content under `key`. The key must match `uri.path` for the URI
   * that will be passed to `vscode.commands.executeCommand('vscode.diff', ...)`.
   */
  setContent(key: string, content: string): void {
    this._contents.set(key, content);
  }

  /** Called by VS Code when it needs to render an `ail-diff:` URI. */
  provideTextDocumentContent(uri: vscode.Uri): string {
    return this._contents.get(uri.path) ?? '';
  }
}
