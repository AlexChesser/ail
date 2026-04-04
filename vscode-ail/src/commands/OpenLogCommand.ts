/**
 * OpenLogCommand — handler for the ail.openLog command.
 *
 * Opens a log viewer for a given run ID (or the latest run if omitted).
 * Creates a virtual document with URI scheme 'ail-log://{run_id}',
 * opens it in the editor, and shows the Markdown preview.
 */

import * as vscode from 'vscode';
import { AilProcess } from '../infrastructure/AilProcess';
import { AilLogProvider } from '../infrastructure/AilLogProvider';

export class OpenLogCommand {
  constructor(
    private readonly _ailProcess: AilProcess,
    private readonly _logProvider: AilLogProvider
  ) {}

  async execute(runId?: string): Promise<void> {
    try {
      // If no runId provided, fetch the latest run to get its ID.
      // The binary will resolve it automatically, but we need to extract
      // the run_id from the response to construct the URI.
      let effectiveRunId = runId;

      if (!effectiveRunId) {
        // Fetch the latest run's formatted output, then extract the run_id.
        // For now, we'll use an empty path which tells the provider to fetch the latest.
        // The URI will be 'ail-log://' (empty path), and the provider will call
        // AilProcess.log(undefined) which the binary handles as "get latest".
      }

      // Construct the virtual document URI
      const uri = vscode.Uri.parse(`ail-log://${effectiveRunId || ''}`);

      // Open the virtual document in the native ail-log viewer.
      // The ail-log language ID is set by the provider so grammar/folding activate automatically.
      const doc = await vscode.workspace.openTextDocument(uri);
      await vscode.window.showTextDocument(doc, { preview: false });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      void vscode.window.showErrorMessage(`Failed to open log: ${message}`);
    }
  }
}
