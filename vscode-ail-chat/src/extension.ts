/**
 * ail Chat VS Code extension — entry point.
 *
 * Activates on workspaces with .ail.yaml files, or when the sidebar view is opened.
 * Registers the ChatViewProvider as a dockable sidebar WebviewView.
 */

import * as vscode from 'vscode';
import { clearBinaryCache } from './binary';
import { SessionManager } from './session-manager';
import { ChatViewProvider } from './chat-view-provider';

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
  chatProvider = new ChatViewProvider(context, sessionManager);

  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(ChatViewProvider.viewId, chatProvider, {
      webviewOptions: { retainContextWhenHidden: true },
    }),

    vscode.commands.registerCommand('ail-chat.open', () => {
      void vscode.commands.executeCommand('workbench.view.extension.ail-chat-sidebar');
    }),

    vscode.commands.registerCommand('ail-chat.newSession', () => {
      chatProvider?.reveal();
    })
  );
}

// Process manager cleanup is handled by the view's onDidDispose.
export function deactivate(): void {}
