/**
 * ValidateCommand — thin command handler that delegates to IAilClient.validate().
 *
 * Publishes VS Code Diagnostics for any validation errors.
 */

import * as vscode from 'vscode';
import * as path from 'path';
import { ServiceContext } from '../application/ServiceContext';

const diagnosticCollection = vscode.languages.createDiagnosticCollection('ail');

/** Resolve the pipeline path to validate. */
function resolvePipelinePath(): string | undefined {
  const editor = vscode.window.activeTextEditor;
  if (editor) {
    const filePath = editor.document.uri.fsPath;
    if (filePath.endsWith('.ail.yaml') || filePath.endsWith('.ail.yml')) {
      return filePath;
    }
  }

  const config = vscode.workspace.getConfiguration('ail');
  const defaultPipeline = config.get<string>('defaultPipeline', '');
  if (defaultPipeline) {
    return defaultPipeline;
  }

  const workspaceFolders = vscode.workspace.workspaceFolders;
  if (workspaceFolders?.[0]) {
    return path.join(workspaceFolders[0].uri.fsPath, '.ail.yaml');
  }

  return undefined;
}

export class ValidateCommand {
  constructor(private readonly _ctx: ServiceContext) {}

  async execute(pipelinePathOverride?: string): Promise<void> {
    const pipelinePath = pipelinePathOverride ?? resolvePipelinePath();
    if (!pipelinePath) {
      void vscode.window.showWarningMessage(
        "No .ail.yaml file found. Open a pipeline file or set ail.defaultPipeline."
      );
      return;
    }

    const fileUri = vscode.Uri.file(pipelinePath);
    diagnosticCollection.delete(fileUri);

    void vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: 'Validating pipeline...',
        cancellable: false,
      },
      async () => {
        const result = await this._ctx.client.validate(pipelinePath);

        if (result.valid) {
          void vscode.window.showInformationMessage('ail: Pipeline valid.');
          diagnosticCollection.delete(fileUri);
        } else {
          const diagnostics: vscode.Diagnostic[] = result.errors.map((msg) => {
            const range = new vscode.Range(0, 0, 0, 0);
            const d = new vscode.Diagnostic(range, msg, vscode.DiagnosticSeverity.Error);
            d.source = 'ail';
            return d;
          });
          diagnosticCollection.set(fileUri, diagnostics);
          void vscode.window.showErrorMessage(
            `Pipeline validation failed — see Problems panel.`
          );
        }
      }
    );
  }

  getDiagnosticCollection(): vscode.DiagnosticCollection {
    return diagnosticCollection;
  }
}
