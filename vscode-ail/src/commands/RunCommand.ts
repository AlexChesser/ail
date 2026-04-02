/**
 * RunCommand — thin command handler that delegates to RunnerService.
 *
 * No spawn() calls here. All process management lives in AilProcess.
 *
 * If the active text editor has a non-empty selection, that text is passed
 * to the ail process as the AIL_SELECTION environment variable, making it
 * available in pipeline templates as {{ env.AIL_SELECTION }}.
 */

import * as vscode from 'vscode';
import { RunnerService } from '../application/RunnerService';
import { resolvePipelinePath } from '../utils/pipelinePath';

export class RunCommand {
  constructor(private readonly _service: RunnerService) {}

  async execute(promptOverride?: string): Promise<void> {
    const pipelinePath = resolvePipelinePath();
    if (!pipelinePath) {
      void vscode.window.showWarningMessage(
        "No .ail.yaml file found. Open a pipeline file or set ail.defaultPipeline."
      );
      return;
    }

    const prompt = promptOverride ?? await vscode.window.showInputBox({
      prompt: 'Enter your prompt for ail',
      placeHolder: 'e.g. refactor the auth module for DRY compliance',
      ignoreFocusOut: true,
    });

    if (!prompt) {
      return;
    }

    // Capture any active editor selection to pass as AIL_SELECTION
    const env: Record<string, string> = {};
    const activeEditor = vscode.window.activeTextEditor;
    if (activeEditor && !activeEditor.selection.isEmpty) {
      const selectedText = activeEditor.document.getText(activeEditor.selection);
      if (selectedText.trim().length > 0) {
        env['AIL_SELECTION'] = selectedText;
      }
    }

    await this._service.startRun(prompt, pipelinePath, Object.keys(env).length > 0 ? env : undefined);
  }
}
