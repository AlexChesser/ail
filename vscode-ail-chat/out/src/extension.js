"use strict";
/**
 * ail Chat VS Code extension — entry point.
 *
 * Activates on workspaces with .ail.yaml files.
 * Registers the `ail-chat.open` command which creates or reveals
 * the single chat WebviewPanel.
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = __importStar(require("vscode"));
const path = __importStar(require("path"));
const fs = __importStar(require("fs"));
const binary_1 = require("./binary");
const ail_process_manager_1 = require("./ail-process-manager");
const session_manager_1 = require("./session-manager");
let panel;
let processManager;
async function activate(context) {
    // Clear binary cache when binaryPath config changes.
    context.subscriptions.push(vscode.workspace.onDidChangeConfiguration((e) => {
        if (e.affectsConfiguration('ail-chat.binaryPath')) {
            (0, binary_1.clearBinaryCache)();
        }
    }));
    const sessionManager = new session_manager_1.SessionManager(context);
    context.subscriptions.push(vscode.commands.registerCommand('ail-chat.open', async () => {
        if (panel) {
            panel.reveal();
            return;
        }
        // Resolve binary (may show error message and throw on failure).
        let binary;
        try {
            binary = await (0, binary_1.resolveBinary)(context);
        }
        catch {
            return; // resolveBinary already showed the error
        }
        const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
        processManager = new ail_process_manager_1.AilProcessManager(binary.path, cwd);
        panel = vscode.window.createWebviewPanel('ailChat', 'ail Chat', vscode.ViewColumn.Beside, {
            enableScripts: true,
            retainContextWhenHidden: true,
            localResourceRoots: [vscode.Uri.file(path.join(context.extensionPath, 'dist'))],
        });
        panel.webview.html = getWebviewHtml(panel.webview, context);
        // Extension Host → Webview: wire process manager events
        processManager.onMessage((msg) => {
            void panel?.webview.postMessage(msg);
        });
        // Webview → Extension Host: handle incoming messages
        panel.webview.onDidReceiveMessage((raw) => {
            handleWebviewMessage(raw, processManager, sessionManager, panel, cwd);
        });
        panel.onDidDispose(() => {
            processManager?.cancel();
            panel = undefined;
            processManager = undefined;
        });
        context.subscriptions.push(panel);
    }), vscode.commands.registerCommand('ail-chat.newSession', () => {
        void vscode.commands.executeCommand('ail-chat.open');
    }));
}
function handleWebviewMessage(msg, mgr, sessions, wvPanel, cwd) {
    switch (msg.type) {
        case 'ready': {
            // Send existing sessions on panel ready
            void sessions.getSessions().then((list) => {
                void wvPanel.webview.postMessage({ type: 'sessionsUpdated', sessions: list });
            });
            break;
        }
        case 'submitPrompt': {
            const config = vscode.workspace.getConfiguration('ail-chat');
            const defaultPipeline = config.get('defaultPipeline', '');
            const headless = config.get('headless', false);
            // Discover pipeline: explicit setting > .ail.yaml in cwd
            let pipeline = defaultPipeline;
            if (!pipeline && cwd) {
                const candidate = path.join(cwd, '.ail.yaml');
                if (fs.existsSync(candidate)) {
                    pipeline = candidate;
                }
            }
            if (!pipeline) {
                void wvPanel.webview.postMessage({
                    type: 'processError',
                    message: 'No pipeline file found. Set ail-chat.defaultPipeline or add .ail.yaml to your workspace.',
                });
                return;
            }
            void mgr.start(msg.text, pipeline, { headless }).catch((err) => {
                void wvPanel.webview.postMessage({ type: 'processError', message: err.message });
            });
            // Persist prompt for session title
            void sessions.recordPrompt(msg.text).then(() => sessions.getSessions().then((list) => {
                void wvPanel.webview.postMessage({ type: 'sessionsUpdated', sessions: list });
            }));
            break;
        }
        case 'hitlResponse':
            mgr.writeStdin({ type: 'hitl_response', step_id: msg.stepId, text: msg.text });
            break;
        case 'permissionResponse':
            mgr.writeStdin({ type: 'permission_response', allowed: msg.allowed, reason: msg.reason });
            break;
        case 'killProcess':
            mgr.cancel();
            break;
        case 'switchSession':
            void sessions.switchSession(msg.sessionId).then((sessionData) => {
                if (sessionData) {
                    void wvPanel.webview.postMessage({ type: 'sessionsUpdated', sessions: [] });
                }
            });
            break;
        case 'newSession':
            // Nothing to do on the host side; the webview resets its own state.
            break;
    }
}
function deactivate() {
    processManager?.cancel();
    panel?.dispose();
}
function getWebviewHtml(webview, context) {
    const scriptUri = webview.asWebviewUri(vscode.Uri.file(path.join(context.extensionPath, 'dist', 'webview.js')));
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
    style-src 'unsafe-inline';
    font-src ${webview.cspSource};
  ">
  <title>ail Chat</title>
  <style>
    body, html { margin: 0; padding: 0; height: 100vh; overflow: hidden; }
    #root { display: flex; height: 100%; }
  </style>
</head>
<body>
  <div id="root"></div>
  <script nonce="${nonce}" src="${scriptUri.toString()}"></script>
</body>
</html>`;
}
function generateNonce() {
    let text = '';
    const possible = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    for (let i = 0; i < 32; i++) {
        text += possible.charAt(Math.floor(Math.random() * possible.length));
    }
    return text;
}
//# sourceMappingURL=extension.js.map