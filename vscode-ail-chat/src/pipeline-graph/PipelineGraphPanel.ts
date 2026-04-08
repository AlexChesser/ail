/**
 * PipelineGraphPanel — singleton WebviewPanel that hosts the React Flow
 * pipeline graph visualizer.
 *
 * Opened via the `ail-chat.openPipelineGraph` command.
 * Communicates with the React webview via postMessage.
 */

import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import { transformPipeline, TransformResult } from './graphTransform';

/** Messages from the graph webview to the extension host. */
export type GraphWebviewToHostMessage =
  | { type: 'ready' }
  | { type: 'openStepInEditor'; sourceFile: string; sourceLine: number };

/** Messages from the extension host to the graph webview. */
export type GraphHostToWebviewMessage =
  | { type: 'init'; data: TransformResult; pipelinePath: string; pipelineName: string }
  | { type: 'update'; data: TransformResult; pipelinePath: string; pipelineName: string }
  | { type: 'error'; message: string };

export class PipelineGraphPanel {
  public static readonly viewType = 'ail-chat.pipelineGraph';

  private static _instance: PipelineGraphPanel | undefined;
  private readonly _panel: vscode.WebviewPanel;
  private readonly _extensionPath: string;
  private _pipelinePath: string;
  private _disposables: vscode.Disposable[] = [];
  private _fileWatchers: vscode.FileSystemWatcher[] = [];

  private constructor(
    panel: vscode.WebviewPanel,
    extensionPath: string,
    pipelinePath: string
  ) {
    this._panel = panel;
    this._extensionPath = extensionPath;
    this._pipelinePath = pipelinePath;

    this._panel.webview.html = this._getHtml();

    this._panel.webview.onDidReceiveMessage(
      (msg: GraphWebviewToHostMessage) => this._handleMessage(msg),
      null,
      this._disposables
    );

    this._panel.onDidDispose(() => this._dispose(), null, this._disposables);

    this._setupFileWatchers();
  }

  /**
   * Show the pipeline graph panel. Creates it if it doesn't exist,
   * otherwise reveals and updates the pipeline.
   */
  static show(extensionPath: string, pipelinePath: string): void {
    if (PipelineGraphPanel._instance) {
      const inst = PipelineGraphPanel._instance;
      const pathChanged = inst._pipelinePath !== pipelinePath;
      inst._pipelinePath = pipelinePath;
      inst._panel.reveal(vscode.ViewColumn.One);
      if (pathChanged) {
        // Reset the webview HTML to destroy any crashed React tree and start fresh.
        // The webview will send 'ready' again, triggering _sendInit() with the new pipeline.
        inst._panel.webview.html = inst._getHtml();
      } else {
        inst._sendUpdate();
      }
      inst._setupFileWatchers();
      return;
    }

    const panel = vscode.window.createWebviewPanel(
      PipelineGraphPanel.viewType,
      `Pipeline: ${path.basename(pipelinePath)}`,
      vscode.ViewColumn.One,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [
          vscode.Uri.file(path.join(extensionPath, 'dist')),
        ],
      }
    );

    PipelineGraphPanel._instance = new PipelineGraphPanel(panel, extensionPath, pipelinePath);
  }

  private _handleMessage(msg: GraphWebviewToHostMessage): void {
    switch (msg.type) {
      case 'ready':
        this._sendInit();
        break;

      case 'openStepInEditor': {
        const uri = vscode.Uri.file(msg.sourceFile);
        const line = Math.max(0, msg.sourceLine);
        const range = new vscode.Range(line, 0, line, 0);
        void vscode.window.showTextDocument(uri, {
          selection: range,
          preview: true,
        });
        break;
      }
    }
  }

  private _sendInit(): void {
    const result = transformPipeline(this._pipelinePath);
    const name = this._extractPipelineName();
    const msg: GraphHostToWebviewMessage = {
      type: 'init',
      data: result,
      pipelinePath: this._pipelinePath,
      pipelineName: name,
    };
    void this._panel.webview.postMessage(msg);

    if (result.errors.length > 0) {
      void this._panel.webview.postMessage({
        type: 'error',
        message: result.errors.join('\n'),
      } satisfies GraphHostToWebviewMessage);
    }
  }

  private _sendUpdate(): void {
    const result = transformPipeline(this._pipelinePath);
    const name = this._extractPipelineName();
    this._panel.title = `Pipeline: ${path.basename(this._pipelinePath)}`;
    const msg: GraphHostToWebviewMessage = {
      type: 'update',
      data: result,
      pipelinePath: this._pipelinePath,
      pipelineName: name,
    };
    void this._panel.webview.postMessage(msg);
  }

  private _extractPipelineName(): string {
    try {
      const content = fs.readFileSync(this._pipelinePath, 'utf-8');
      const match = content.match(/^\s*name:\s*["']?(.+?)["']?\s*$/m);
      return match?.[1] ?? path.basename(this._pipelinePath);
    } catch {
      return path.basename(this._pipelinePath);
    }
  }

  private _setupFileWatchers(): void {
    // Dispose old watchers.
    for (const w of this._fileWatchers) w.dispose();
    this._fileWatchers = [];

    // Watch the pipeline file and its directory for YAML changes.
    const dir = path.dirname(this._pipelinePath);
    const watcher = vscode.workspace.createFileSystemWatcher(
      new vscode.RelativePattern(dir, '**/*.{yaml,yml}')
    );
    watcher.onDidChange(() => this._sendUpdate());
    watcher.onDidCreate(() => this._sendUpdate());
    watcher.onDidDelete(() => this._sendUpdate());
    this._fileWatchers.push(watcher);
  }

  private _dispose(): void {
    PipelineGraphPanel._instance = undefined;
    for (const d of this._disposables) d.dispose();
    for (const w of this._fileWatchers) w.dispose();
    this._disposables = [];
    this._fileWatchers = [];
  }

  private _getHtml(): string {
    const webview = this._panel.webview;
    const scriptUri = webview.asWebviewUri(
      vscode.Uri.file(path.join(this._extensionPath, 'dist', 'graphWebview.js'))
    );
    const cssUri = webview.asWebviewUri(
      vscode.Uri.file(path.join(this._extensionPath, 'dist', 'graphWebview.css'))
    );
    const nonce = generateNonce();

    // React Flow requires its own CSS. We bundle it into the JS via esbuild's
    // CSS loader, but also need to allow inline styles for the node positioning.
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
  <title>Pipeline Graph</title>
  <link rel="stylesheet" href="${cssUri.toString()}">
  <style>
    html, body, #root {
      margin: 0;
      padding: 0;
      width: 100%;
      height: 100%;
      overflow: hidden;
      background: var(--vscode-editor-background);
      color: var(--vscode-editor-foreground);
      font-family: var(--vscode-font-family);
    }
    /* React Flow Controls — match VS Code theme */
    .react-flow__controls-button {
      background: var(--vscode-button-secondaryBackground) !important;
      border-color: var(--vscode-panel-border) !important;
      fill: var(--vscode-button-secondaryForeground) !important;
    }
    .react-flow__controls-button:hover {
      background: var(--vscode-button-secondaryHoverBackground) !important;
    }
    /* React Flow MiniMap — theme-aware mask */
    .react-flow__minimap-mask {
      fill: var(--vscode-sideBar-background) !important;
      fill-opacity: 0.8 !important;
    }
    /* React Flow edge labels — ensure theme foreground */
    .react-flow__edge-textbg {
      fill: var(--vscode-editor-background) !important;
    }
    .react-flow__edge-text {
      fill: var(--vscode-editor-foreground) !important;
    }
  </style>
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
