/**
 * ail.validatePipeline command.
 *
 * Runs `ail validate --pipeline <path>` on the active editor's file.
 * On success: shows an info notification.
 * On failure: publishes structured diagnostics to the Problems panel.
 */

import * as vscode from "vscode";
import { execFile } from "child_process";
import * as path from "path";
import { ResolvedBinary } from "../binary";

const diagnosticCollection = vscode.languages.createDiagnosticCollection("ail");

/** Parse the ail validate error output into a VS Code Diagnostic. */
function parseErrorOutput(stderr: string, fileUri: vscode.Uri): vscode.Diagnostic[] {
  const diagnostics: vscode.Diagnostic[] = [];

  // ail errors come in the format: [error_type] title: detail
  // We don't have line numbers yet, so pin to line 0.
  const lines = stderr.trim().split("\n").filter(Boolean);
  for (const line of lines) {
    const range = new vscode.Range(0, 0, 0, 0);
    const diagnostic = new vscode.Diagnostic(
      range,
      line,
      vscode.DiagnosticSeverity.Error
    );
    diagnostic.source = "ail";
    diagnostics.push(diagnostic);
  }

  return diagnostics;
}

/** Resolve the pipeline path to validate. */
function resolvePipelinePath(): string | undefined {
  const editor = vscode.window.activeTextEditor;
  if (editor) {
    const filePath = editor.document.uri.fsPath;
    if (filePath.endsWith(".ail.yaml") || filePath.endsWith(".ail.yml")) {
      return filePath;
    }
  }

  const config = vscode.workspace.getConfiguration("ail");
  const defaultPipeline = config.get<string>("defaultPipeline", "");
  if (defaultPipeline) {
    return defaultPipeline;
  }

  // Try .ail.yaml in workspace root
  const workspaceFolders = vscode.workspace.workspaceFolders;
  if (workspaceFolders?.[0]) {
    return path.join(workspaceFolders[0].uri.fsPath, ".ail.yaml");
  }

  return undefined;
}

export function registerValidateCommand(
  context: vscode.ExtensionContext,
  binary: ResolvedBinary
): void {
  context.subscriptions.push(diagnosticCollection);

  const disposable = vscode.commands.registerCommand(
    "ail.validatePipeline",
    async () => {
      const pipelinePath = resolvePipelinePath();
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
          title: "Validating pipeline...",
          cancellable: false,
        },
        async () => {
          return new Promise<void>((resolve) => {
            execFile(
              binary.path,
              ["validate", "--pipeline", pipelinePath],
              { timeout: 15000 },
              (err, stdout, stderr) => {
                resolve();

                if (!err) {
                  void vscode.window.showInformationMessage(
                    `ail: ${stdout.trim() || "Pipeline valid."}`
                  );
                  diagnosticCollection.delete(fileUri);
                } else {
                  const diags = parseErrorOutput(stderr || stdout, fileUri);
                  diagnosticCollection.set(fileUri, diags);
                  void vscode.window.showErrorMessage(
                    `Pipeline validation failed — see Problems panel.`
                  );
                }
              }
            );
          });
        }
      );
    }
  );

  context.subscriptions.push(disposable);
}

/** Register save-time validation for .ail.yaml files. */
export function registerSaveValidation(
  context: vscode.ExtensionContext,
  binary: ResolvedBinary
): void {
  const disposable = vscode.workspace.onDidSaveTextDocument((document) => {
    const filePath = document.uri.fsPath;
    if (!filePath.endsWith(".ail.yaml") && !filePath.endsWith(".ail.yml")) {
      return;
    }

    const fileUri = document.uri;
    execFile(
      binary.path,
      ["validate", "--pipeline", filePath],
      { timeout: 15000 },
      (err, stdout, stderr) => {
        if (!err) {
          diagnosticCollection.delete(fileUri);
        } else {
          const diags = parseErrorOutput(stderr || stdout, fileUri);
          diagnosticCollection.set(fileUri, diags);
        }
      }
    );
  });

  context.subscriptions.push(disposable);
}
