import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import { resolveBinary } from './binary';
import { AilProcessManager } from './ail-process-manager';
import { SessionManager } from './session-manager';
import { WebviewToHostMessage } from './types';

const LAST_PIPELINE_KEY = 'ail-chat.lastPipeline';

export class ChatViewProvider implements vscode.WebviewViewProvider {
  public static readonly viewId = 'ail-chat.chatView';

  private _view?: vscode.WebviewView;
  private _processManager?: AilProcessManager;
  /** Currently active pipeline path, or null for passthrough mode. */
  private _currentPipeline: string | null = null;

  constructor(
    private readonly _context: vscode.ExtensionContext,
    private readonly _sessionManager: SessionManager
  ) {
    // Restore last pipeline from workspace state.
    const saved = this._context.workspaceState.get<string>(LAST_PIPELINE_KEY);
    if (saved && fs.existsSync(saved)) {
      this._currentPipeline = saved;
    }
  }

  resolveWebviewView(
    webviewView: vscode.WebviewView,
    _resolveContext: vscode.WebviewViewResolveContext,
    _token: vscode.CancellationToken
  ): void {
    this._view = webviewView;

    webviewView.webview.options = {
      enableScripts: true,
      localResourceRoots: [vscode.Uri.file(path.join(this._context.extensionPath, 'dist'))],
    };

    webviewView.webview.html = this._getWebviewHtml(webviewView.webview);

    webviewView.webview.onDidReceiveMessage((raw: WebviewToHostMessage) => {
      void this._handleWebviewMessage(raw);
    });

    webviewView.onDidDispose(() => {
      this._processManager?.cancel();
      this._view = undefined;
      this._processManager = undefined;
    });
  }

  reveal(): void {
    this._view?.show?.(true);
  }

  private _sendPipelineChanged(): void {
    const p = this._currentPipeline;
    void this._view?.webview.postMessage({
      type: 'pipelineChanged',
      path: p,
      displayName: p ? path.basename(p) : null,
    });
  }

  private async _handleWebviewMessage(msg: WebviewToHostMessage): Promise<void> {
    switch (msg.type) {
      case 'ready': {
        const list = await this._sessionManager.getSessions();
        void this._view?.webview.postMessage({ type: 'sessionsUpdated', sessions: list });
        this._sendPipelineChanged();
        break;
      }

      case 'submitPrompt': {
        if (!this._processManager) {
          let binary;
          try {
            binary = await resolveBinary(this._context);
          } catch {
            return; // resolveBinary already showed the error
          }
          const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
          this._processManager = new AilProcessManager(binary.path, cwd);
          this._processManager.onMessage((m) => {
            void this._view?.webview.postMessage(m);
          });
        }

        // Pipeline resolution order:
        // 1. Explicitly loaded pipeline (_currentPipeline)
        // 2. ail-chat.defaultPipeline setting
        // 3. .ail.yaml at workspace root
        // 4. Passthrough mode (no --pipeline flag)
        const config = vscode.workspace.getConfiguration('ail-chat');
        const headless = config.get<boolean>('headless', false);
        const pipeline = this._resolvedPipeline();

        void this._processManager.start(msg.text, pipeline ?? undefined, { headless }).catch((err: Error) => {
          void this._view?.webview.postMessage({ type: 'processError', message: err.message });
        });

        void this._sessionManager.recordPrompt(msg.text).then(() =>
          this._sessionManager.getSessions().then((list) => {
            void this._view?.webview.postMessage({ type: 'sessionsUpdated', sessions: list });
          })
        );
        break;
      }

      case 'loadPipeline': {
        const uris = await vscode.window.showOpenDialog({
          canSelectMany: false,
          canSelectFolders: false,
          filters: { 'ail Pipeline': ['yaml', 'yml'] },
          title: 'Select ail pipeline file',
          openLabel: 'Load Pipeline',
        });
        if (uris && uris.length > 0) {
          this._currentPipeline = uris[0].fsPath;
          void this._context.workspaceState.update(LAST_PIPELINE_KEY, this._currentPipeline);
          this._sendPipelineChanged();
        }
        break;
      }

      case 'hitlResponse':
        this._processManager?.writeStdin({
          type: 'hitl_response',
          step_id: msg.stepId,
          text: msg.text,
        });
        break;

      case 'permissionResponse':
        this._processManager?.writeStdin({
          type: 'permission_response',
          allowed: msg.allowed,
          reason: msg.reason,
          allow_for_session: msg.allowForSession,
        });
        break;

      case 'killProcess':
        this._processManager?.cancel();
        break;

      case 'switchSession':
        void this._sessionManager.switchSession(msg.sessionId).then((sessionData) => {
          if (sessionData) {
            void this._view?.webview.postMessage({ type: 'sessionsUpdated', sessions: [] });
          }
        });
        break;

      case 'newSession':
        // Nothing to do on the host side; the webview resets its own state.
        break;
    }
  }

  /** Returns the effective pipeline path to use, or null for passthrough mode. */
  private _resolvedPipeline(): string | null {
    if (this._currentPipeline) return this._currentPipeline;
    const config = vscode.workspace.getConfiguration('ail-chat');
    const defaultPipeline = config.get<string>('defaultPipeline', '');
    if (defaultPipeline) return defaultPipeline;
    const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (cwd) {
      const candidate = path.join(cwd, '.ail.yaml');
      if (fs.existsSync(candidate)) return candidate;
    }
    return null;
  }

  private _getWebviewHtml(webview: vscode.Webview): string {
    const scriptUri = webview.asWebviewUri(
      vscode.Uri.file(path.join(this._context.extensionPath, 'dist', 'webview.js'))
    );
    const styleUri = webview.asWebviewUri(
      vscode.Uri.file(path.join(this._context.extensionPath, 'dist', 'webview.css'))
    );
    const codiconUri = webview.asWebviewUri(
      vscode.Uri.file(path.join(this._context.extensionPath, 'dist', 'codicon.css'))
    );
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
}

function generateNonce(): string {
  let text = '';
  const possible = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
  for (let i = 0; i < 32; i++) {
    text += possible.charAt(Math.floor(Math.random() * possible.length));
  }
  return text;
}
