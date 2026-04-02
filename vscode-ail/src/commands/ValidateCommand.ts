/**
 * ValidateCommand — thin command handler that delegates to IAilClient.validate().
 *
 * Publishes VS Code Diagnostics for any validation errors.
 */

import * as vscode from 'vscode';
import { ServiceContext } from '../application/ServiceContext';
import { resolvePipelinePath } from '../utils/pipelinePath';

const diagnosticCollection = vscode.languages.createDiagnosticCollection('ail');

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
