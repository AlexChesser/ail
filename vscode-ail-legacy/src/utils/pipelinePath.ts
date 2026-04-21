/**
 * Shared pipeline path resolution utility.
 *
 * Resolution priority (highest first):
 *   1. Active pipeline set via the sidebar selector
 *   2. Active text editor (if it's a .ail.yaml/.ail.yml file)
 *   3. ail.defaultPipeline workspace setting
 *   4. .ail.yaml at the workspace root
 */

import * as vscode from 'vscode';
import * as path from 'path';
import { getActivePipeline } from '../state';

export function resolvePipelinePath(): string | undefined {
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
