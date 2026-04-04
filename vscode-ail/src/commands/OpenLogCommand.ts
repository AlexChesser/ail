/**
 * OpenLogCommand — handler for the ail.openLog command.
 *
 * Opens a log viewer for a given run ID (or the latest run if omitted).
 * Creates a virtual document with URI scheme 'ail-log://{run_id}',
 * opens it in the editor, and shows the Markdown preview by default.
 */

import * as vscode from 'vscode';
import { AilProcess } from '../infrastructure/AilProcess';
import { AilLogProvider } from '../infrastructure/AilLogProvider';

export class OpenLogCommand {
  private _lastUri?: vscode.Uri;

  constructor(
    private readonly _ailProcess: AilProcess,
    private readonly _logProvider: AilLogProvider
  ) {}

  async execute(runId?: string): Promise<void> {
    try {
      // If no runId provided, fetch the latest run to get its ID.
      // The binary will resolve it automatically, but we need to extract
      // the run_id from the response to construct the URI.
      const effectiveRunId = runId;

      if (!effectiveRunId) {
        // Fetch the latest run's formatted output, then extract the run_id.
        // For now, we'll use an empty path which tells the provider to fetch the latest.
        // The URI will be 'ail-log://' (empty path), and the provider will call
        // AilProcess.log(undefined) which the binary handles as "get latest".
      }

      // Construct the virtual document URI
      const uri = vscode.Uri.parse(`ail-log://${effectiveRunId || ''}`);
      this._lastUri = uri;

      // Open the virtual document so AilLogProvider can serve it.
      await vscode.workspace.openTextDocument(uri);

      // Default: Markdown preview (no split, formats automatically).
      await vscode.commands.executeCommand('markdown.showPreview', uri);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      void vscode.window.showErrorMessage(`Failed to open log: ${message}`);
    }
  }

  /** Returns the URI of the most recently opened log, for use by toggleView. */
  getLastUri(): vscode.Uri | undefined {
    return this._lastUri;
  }

  /** Open the raw text editor for a URI (used by toggleView to go back to raw). */
  async openRaw(uri: vscode.Uri): Promise<void> {
    const doc = await vscode.workspace.openTextDocument(uri);
    await vscode.window.showTextDocument(doc, { preview: false });
  }
}
