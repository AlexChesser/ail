/**
 * ail Chat VS Code extension — entry point.
 *
 * Activates on workspaces with .ail.yaml files, or when the sidebar view is opened.
 * Registers the ChatViewProvider as a dockable sidebar WebviewView.
 */

import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import { clearBinaryCache, resolveBinary } from './binary';
import { SessionManager } from './session-manager';
import { ChatViewProvider } from './chat-view-provider';
import { PipelineGraphPanel } from './pipeline-graph/PipelineGraphPanel';
import { AilOutputChannel } from './output-channel';
import { checkAndOfferInstall } from './install-wizard';
import { RunHistoryProvider, registerRunLogCommand } from './history-tree-provider';
import { PipelineStepsProvider } from './steps-tree-provider';

let chatProvider: ChatViewProvider | undefined;

export function activate(context: vscode.ExtensionContext): void {
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration('ail-chat.binaryPath')) {
        clearBinaryCache();
      }
    })
  );

  const sessionManager = new SessionManager(context);
  const rawChannel = vscode.window.createOutputChannel('AIL');
  context.subscriptions.push(rawChannel);
  const outputChannel = new AilOutputChannel(rawChannel);

  const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;

  const historyProvider = new RunHistoryProvider('', cwd);
  const stepsProvider = new PipelineStepsProvider();

  // Resolve binary path lazily for tree providers (falls back gracefully if binary not found)
  void resolveBinary(context).then((b) => {
    historyProvider.setBinaryPath(b.path);
    historyProvider.refresh();
  }).catch(() => { /* binary not found — history tree stays empty */ });

  context.subscriptions.push(
    vscode.window.registerTreeDataProvider('ail-chat.historyView', historyProvider),
    vscode.window.registerTreeDataProvider('ail-chat.stepsView', stepsProvider)
  );

  registerRunLogCommand(context, historyProvider);

  context.subscriptions.push(
    vscode.commands.registerCommand('ail-chat.openStep', (item) => {
      stepsProvider.openStep(item);
    })
  );

  let panelVisible = false;
  context.subscriptions.push(
    vscode.commands.registerCommand('ail-chat.toggleInfoPanel', () => {
      panelVisible = !panelVisible;
      void vscode.commands.executeCommand('setContext', 'ail-chat.panelVisible', panelVisible);
    })
  );

  chatProvider = new ChatViewProvider(context, sessionManager, outputChannel, historyProvider, stepsProvider);

  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(ChatViewProvider.viewId, chatProvider, {
      webviewOptions: { retainContextWhenHidden: true },
    }),

    vscode.commands.registerCommand('ail-chat.open', () => {
      void vscode.commands.executeCommand('workbench.view.extension.ail-chat-sidebar');
    }),

    vscode.commands.registerCommand('ail-chat.newSession', () => {
      chatProvider?.reveal();
    }),

    vscode.commands.registerCommand('ail-chat.openPipelineGraph', async () => {
      // Pipeline resolution order:
      // 1. Active editor (if it's a .ail.yaml file)
      // 2. File picker dialog
      const activeEditor = vscode.window.activeTextEditor;
      const activeFile = activeEditor?.document.uri.fsPath;
      const isYaml = activeFile && /\.ail\.ya?ml$/i.test(activeFile);

      let pipelinePath: string | undefined;
      if (isYaml) {
        pipelinePath = activeFile;
      } else {
        // Check workspace root for .ail.yaml
        const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
        if (cwd) {
          const candidate = path.join(cwd, '.ail.yaml');
          if (fs.existsSync(candidate)) {
            pipelinePath = candidate;
          }
        }
      }

      if (!pipelinePath) {
        const uris = await vscode.window.showOpenDialog({
          canSelectMany: false,
          canSelectFolders: false,
          filters: { 'ail Pipeline': ['yaml', 'yml'] },
          title: 'Select ail pipeline file to visualize',
          openLabel: 'Open Pipeline Graph',
        });
        if (uris && uris.length > 0) {
          pipelinePath = uris[0].fsPath;
        }
      }

      if (pipelinePath) {
        PipelineGraphPanel.show(context.extensionPath, pipelinePath);
      }
    }),

    vscode.workspace.onDidChangeWorkspaceFolders(() => {
      if (chatProvider) void checkAndOfferInstall(context, chatProvider);
    })
  );

  void checkAndOfferInstall(context, chatProvider);
}

// Process manager cleanup is handled by the view's onDidDispose.
export function deactivate(): void {}
