/**
 * ail VSCode extension — entry point.
 *
 * Activates on workspaces containing .ail.yaml files.
 * Resolves the ail binary, registers commands, and sets up lifecycle management.
 */

import * as vscode from "vscode";
import { resolveBinary, clearBinaryCache } from "./binary";
import { registerValidateCommand, registerSaveValidation } from "./commands/validate";
import { registerRunCommands } from "./commands/run";
import { registerPipelineExplorer } from "./views/PipelineTreeProvider";
import { registerCompletions } from "./language/completions";

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  // Status bar item — shows running state.
  const statusBarItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    100
  );
  context.subscriptions.push(statusBarItem);

  // Clear binary cache when ail.binaryPath changes.
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("ail.binaryPath")) {
        clearBinaryCache();
      }
    })
  );

  // Resolve binary. Errors are surfaced by resolveBinary() itself.
  let binary;
  try {
    binary = await resolveBinary(context);
  } catch {
    // resolveBinary() already showed an error message. Extension is degraded but not crashed.
    // Register commands in a degraded state so the user can fix the binary and retry.
    return;
  }

  // Register all commands and views.
  registerValidateCommand(context, binary);
  registerSaveValidation(context, binary);
  registerRunCommands(context, binary, statusBarItem);
  registerPipelineExplorer(context, binary);
  registerCompletions(context);

  // Log activation.
  console.log(`ail extension activated — binary: ${binary.path} (${binary.version})`);
}

export function deactivate(): void {
  // VS Code disposes all context.subscriptions automatically.
}
