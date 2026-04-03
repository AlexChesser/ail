/**
 * CreatePipelineCommand — QuickPick template selector + file creation.
 *
 * Registered before binary resolution so it works even without the ail binary.
 */

import * as vscode from 'vscode';
import * as path from 'path';
import { PIPELINE_TEMPLATES } from '../templates';
import { setActivePipeline } from '../state';

export class CreatePipelineCommand {
  constructor(private readonly _context: vscode.ExtensionContext) {}

  async execute(): Promise<void> {
    // Show template picker
    const picked = await vscode.window.showQuickPick(PIPELINE_TEMPLATES, {
      title: 'Create Pipeline from Template',
      placeHolder: 'Select a pipeline template',
      matchOnDescription: true,
      matchOnDetail: true,
    });
    if (!picked) return;

    // Determine target path
    const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    let targetUri: vscode.Uri | undefined;

    const defaultPath = workspaceRoot ? path.join(workspaceRoot, '.ail.yaml') : undefined;
    const defaultExists = defaultPath
      ? await vscode.workspace.fs.stat(vscode.Uri.file(defaultPath)).then(() => true, () => false)
      : false;

    if (!defaultExists && defaultPath) {
      targetUri = vscode.Uri.file(defaultPath);
    } else {
      // .ail.yaml already exists — prompt for a save location
      targetUri = await vscode.window.showSaveDialog({
        defaultUri: workspaceRoot ? vscode.Uri.file(path.join(workspaceRoot, 'pipeline.ail.yaml')) : undefined,
        filters: { 'AIL Pipeline': ['yaml'] },
        title: '.ail.yaml already exists — choose a different name',
        saveLabel: 'Create Pipeline',
      });
    }

    if (!targetUri) return;

    // Write template content
    await vscode.workspace.fs.writeFile(targetUri, Buffer.from(picked.content, 'utf8'));

    // Open in editor
    await vscode.window.showTextDocument(targetUri);

    // Set as active pipeline and persist that the user has created one
    setActivePipeline(targetUri.fsPath);
    await this._context.globalState.update('ail.hasCreatedPipeline', true);

    void vscode.window.showInformationMessage(
      `Pipeline created at ${path.basename(targetUri.fsPath)}. Edit the steps, then use the Invocation panel to run it.`
    );
  }
}
