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

export function deactivate(): void {
  processManager?.cancel();
  panel?.dispose();
}

function getWebviewHtml(webview: vscode.Webview, context: vscode.ExtensionContext): string {
  const scriptUri = webview.asWebviewUri(
    vscode.Uri.file(path.join(context.extensionPath, 'dist', 'webview.js'))
  );

  const styleUri = webview.asWebviewUri(
    vscode.Uri.file(path.join(context.extensionPath, 'dist', 'webview.css'))
  );

  const codiconUri = webview.asWebviewUri(
    vscode.Uri.file(path.join(context.extensionPath, 'dist', 'codicon.css'))
  );

  // Content Security Policy: only allow scripts from our extension origin.
  const nonce = generateNonce();

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy" content="
    default-src 'none';
    script-src 'nonce-${nonce}';
    style-src ${webview.cspSource} 'unsafe-inline';
    font-src ${webview.cspSource};
  ">
  <title>ail Chat</title>
  <link rel="stylesheet" href="${codiconUri.toString()}">
  <link rel="stylesheet" href="${styleUri.toString()}">
</head>
<body>
  <div id="root"></div>
  <script nonce="${nonce}" src="${scriptUri.toString()}"></script>
</body>
</html>`;
}

function generateNonce(): string {
  let text = '';
  const possible = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
  for (let i = 0; i < 32; i++) {
    text += possible.charAt(Math.floor(Math.random() * possible.length));
  }
  return text;
  // Process manager cleanup is handled by the view's onDidDispose.
}
