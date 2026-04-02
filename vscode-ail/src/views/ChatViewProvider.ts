/**
 * Chat input view — sidebar WebviewView.
 *
 * Renders a textarea + Run/Stop buttons inside the AIL sidebar panel.
 * Submitting the form dispatches ail.runWithPrompt; Stop dispatches ail.stopPipeline.
 */

import * as vscode from "vscode";

export class ChatViewProvider implements vscode.WebviewViewProvider {
  public static readonly viewId = "ail.chatView";

  private _view?: vscode.WebviewView;

  resolveWebviewView(webviewView: vscode.WebviewView): void {
    this._view = webviewView;

    webviewView.webview.options = { enableScripts: true };
    webviewView.webview.html = this._html();

    webviewView.webview.onDidReceiveMessage((msg: { type: string; prompt?: string }) => {
      if (msg.type === "send" && msg.prompt) {
        void vscode.commands.executeCommand("ail.runWithPrompt", msg.prompt);
      } else if (msg.type === "stop") {
        void vscode.commands.executeCommand("ail.stopPipeline");
      }
    });
  }

  setRunning(running: boolean): void {
    this._view?.webview.postMessage({ type: "setRunning", running });
  }

  /** Pre-populate the textarea with a prompt (used by Fork from history). */
  populate(prompt: string): void {
    this._view?.webview.postMessage({ type: "populate", prompt });
  }

  private _html(): string {
    return /* html */ `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline';">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    padding: 8px;
    font-family: var(--vscode-font-family);
    font-size: var(--vscode-font-size);
    color: var(--vscode-foreground);
    background: transparent;
  }
  textarea {
    width: 100%;
    min-height: 72px;
    resize: vertical;
    background: var(--vscode-input-background);
    color: var(--vscode-input-foreground);
    border: 1px solid var(--vscode-input-border, transparent);
    padding: 6px 8px;
    font-family: var(--vscode-font-family);
    font-size: var(--vscode-font-size);
    border-radius: 2px;
    line-height: 1.4;
  }
  textarea:focus {
    outline: 1px solid var(--vscode-focusBorder);
    border-color: transparent;
  }
  textarea::placeholder { color: var(--vscode-input-placeholderForeground); }
  .row {
    margin-top: 6px;
    display: flex;
    gap: 6px;
  }
  button {
    flex: 1;
    padding: 4px 10px;
    font-family: var(--vscode-font-family);
    font-size: var(--vscode-font-size);
    border: none;
    border-radius: 2px;
    cursor: pointer;
  }
  #runBtn {
    background: var(--vscode-button-background);
    color: var(--vscode-button-foreground);
  }
  #runBtn:hover:not(:disabled) { background: var(--vscode-button-hoverBackground); }
  #runBtn:disabled { opacity: 0.5; cursor: default; }
  #stopBtn {
    background: var(--vscode-button-secondaryBackground);
    color: var(--vscode-button-secondaryForeground);
    display: none;
  }
  #stopBtn:hover { background: var(--vscode-button-secondaryHoverBackground); }
  #stopBtn.visible { display: block; flex: 0 0 auto; }
  .hint {
    margin-top: 5px;
    font-size: 11px;
    color: var(--vscode-descriptionForeground);
  }
</style>
</head>
<body>
<textarea id="prompt" placeholder="Enter prompt for ail pipeline…"></textarea>
<div class="row">
  <button id="runBtn">Run</button>
  <button id="stopBtn">Stop</button>
</div>
<p class="hint">Ctrl+Enter to send</p>

<script>
  const vscode = acquireVsCodeApi();
  const textarea = document.getElementById('prompt');
  const runBtn   = document.getElementById('runBtn');
  const stopBtn  = document.getElementById('stopBtn');

  function send() {
    const prompt = textarea.value.trim();
    if (!prompt || runBtn.disabled) return;
    textarea.value = '';
    vscode.postMessage({ type: 'send', prompt });
  }

  runBtn.addEventListener('click', send);

  stopBtn.addEventListener('click', () => {
    vscode.postMessage({ type: 'stop' });
  });

  textarea.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      send();
    }
  });

  window.addEventListener('message', (e) => {
    const msg = e.data;
    if (msg.type === 'setRunning') {
      runBtn.disabled = msg.running;
      stopBtn.classList.toggle('visible', msg.running);
    } else if (msg.type === 'populate') {
      textarea.value = msg.prompt || '';
      textarea.focus();
    }
  });
</script>
</body>
</html>`;
  }
}
