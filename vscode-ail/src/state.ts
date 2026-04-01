/**
 * Active pipeline state — shared across providers and commands.
 *
 * Persisted in workspaceState so the selection survives VS Code restarts.
 */

import * as vscode from "vscode";

const _onDidChangeActivePipeline = new vscode.EventEmitter<string | undefined>();
export const onDidChangeActivePipeline = _onDidChangeActivePipeline.event;

let _activePipeline: string | undefined;
let _ctx: vscode.ExtensionContext | undefined;

export function initState(context: vscode.ExtensionContext): void {
  _ctx = context;
  _activePipeline = context.workspaceState.get<string>("ail.activePipeline");
}

export function getActivePipeline(): string | undefined {
  return _activePipeline;
}

export function setActivePipeline(filePath: string | undefined): void {
  _activePipeline = filePath;
  void _ctx?.workspaceState.update("ail.activePipeline", filePath);
  _onDidChangeActivePipeline.fire(filePath);
}
