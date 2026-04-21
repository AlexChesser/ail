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
import { parseStepsFromYaml } from '../utils/parseYaml';

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

    // ── Plan & Approval (ail.confirmBeforeRun) ────────────────────────────────
    const confirmBeforeRun = vscode.workspace
      .getConfiguration('ail')
      .get<boolean>('confirmBeforeRun', false);

    if (confirmBeforeRun) {
      const steps = parseStepsFromYaml(pipelinePath);
      const stepList =
        steps.length > 0
          ? ['invocation', ...steps.map((s) => s.id)].join(' → ')
          : 'invocation';
      const totalLabel = steps.length > 0
        ? `${steps.length + 1} step${steps.length + 1 !== 1 ? 's' : ''}`
        : '1 step (invocation only)';

      const answer = await vscode.window.showInformationMessage(
        `About to run: ${stepList} (${totalLabel}). Continue?`,
        { modal: false },
        'Run',
        'Cancel',
      );

      if (answer !== 'Run') {
        return;
      }
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
