/**
 * RunCommand — thin command handler that delegates to RunnerService.
 *
 * No spawn() calls here. All process management lives in AilProcess.
 */

import * as vscode from 'vscode';
import * as path from 'path';
import { RunnerService } from '../application/RunnerService';
import { getActivePipeline } from '../state';

/** Resolve the pipeline path to run. Priority: active selection > open editor > config > workspace root. */
function resolvePipelinePath(): string | undefined {
  // 1. Active pipeline set via sidebar selector
  const active = getActivePipeline();
  if (active) return active;

  // 2. Active text editor
  const editor = vscode.window.activeTextEditor;
  if (editor) {
    const filePath = editor.document.uri.fsPath;
    if (filePath.endsWith('.ail.yaml') || filePath.endsWith('.ail.yml')) {
      return filePath;
    }
  }

  // 3. ail.defaultPipeline setting
  const config = vscode.workspace.getConfiguration('ail');
  const defaultPipeline = config.get<string>('defaultPipeline', '');
  if (defaultPipeline) {
    return defaultPipeline;
  }

  // 4. Workspace root .ail.yaml
  const workspaceFolders = vscode.workspace.workspaceFolders;
  if (workspaceFolders?.[0]) {
    return path.join(workspaceFolders[0].uri.fsPath, '.ail.yaml');
  }

  return undefined;
}

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
