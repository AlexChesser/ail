/**
 * RunCommand — thin command handler that delegates to RunnerService.
 *
 * No spawn() calls here. All process management lives in AilProcess.
 */

import * as vscode from 'vscode';
import { RunnerService } from '../application/RunnerService';
import { resolvePipelinePath } from '../utils/pipelinePath';

export class RunCommand {
  constructor(private readonly _service: RunnerService) {}

  async execute(promptOverride?: string): Promise<void> {
    if (this._service.isRunning) {
      void vscode.window.showWarningMessage(
        "An ail pipeline is already running. Use 'Ail: Stop Pipeline' to cancel it first."
      );
      return;
    }

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

    await this._service.startRun(prompt, pipelinePath);
  }
}
