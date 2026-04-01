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
import { registerStepsView } from "./views/StepsTreeProvider";
import { ChatViewProvider } from "./views/ChatViewProvider";
import { registerCompletions } from "./language/completions";
import { initState } from "./state";

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  // Persistent active pipeline state
  initState(context);

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
    return;
  }

  // Register views
  const chatProvider = new ChatViewProvider();
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(ChatViewProvider.viewId, chatProvider)
  );

  registerPipelineExplorer(context, binary);
  const stepsProvider = registerStepsView(context);

  // Register commands
  registerValidateCommand(context, binary);
  registerSaveValidation(context, binary);
  registerRunCommands(context, binary, statusBarItem, chatProvider, stepsProvider);
  registerCompletions(context);

  console.log(`ail extension activated — binary: ${binary.path} (${binary.version})`);
}

export function deactivate(): void {
  // VS Code disposes all context.subscriptions automatically.
}
