import * as vscode from 'vscode';
import { spawn } from 'child_process';

// ── Types ─────────────────────────────────────────────────────────────────────

interface SessionSummary {
  run_id: string;
  pipeline_source?: string;
  started_at?: number;
  steps?: Array<{ step_id: string; prompt?: string }>;
}

export type RunLogFn = (binaryPath: string, args: string[], cwd?: string) => Promise<string>;

// ── Helpers ───────────────────────────────────────────────────────────────────

function relativeTime(epochMs: number): string {
  const diffSec = Math.floor((Date.now() - epochMs) / 1000);
  if (diffSec < 60) return 'just now';
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m ago`;
  if (diffSec < 86400) return `${Math.floor(diffSec / 3600)}h ago`;
  if (diffSec < 86400 * 7) return `${Math.floor(diffSec / 86400)}d ago`;
  return new Date(epochMs).toLocaleDateString();
}

function invocationPrompt(session: SessionSummary): string {
  const step = session.steps?.find((s) => s.step_id === 'invocation');
  return step?.prompt ?? session.run_id;
}

function truncate(s: string, max: number): string {
  return s.length <= max ? s : s.slice(0, max - 1) + '…';
}

function defaultRunLogFn(binaryPath: string, args: string[], cwd?: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const proc = spawn(binaryPath, args, { cwd, env: { ...process.env, CLAUDECODE: undefined } });
    const chunks: Buffer[] = [];
    proc.stdout.on('data', (d: Buffer) => chunks.push(d));
    proc.on('close', (code) => {
      if (code === 0) resolve(Buffer.concat(chunks).toString());
      else reject(new Error(`ail exited with code ${code}`));
    });
    proc.on('error', reject);
  });
}

// ── Tree items ────────────────────────────────────────────────────────────────

export class RunItem extends vscode.TreeItem {
  constructor(
    public readonly runId: string,
    prompt: string,
    timestamp: number,
    public readonly binaryPath: string,
    public readonly cwd?: string,
    public readonly runLogFn?: RunLogFn
  ) {
    const label = truncate(prompt, 60);
    super(label, vscode.TreeItemCollapsibleState.None);
    this.description = timestamp > 0 ? relativeTime(timestamp * 1000) : undefined;
    this.tooltip = prompt;
    this.contextValue = 'ailRun';
    this.iconPath = new vscode.ThemeIcon('history');
    this.command = {
      command: 'ail-chat.openRunLog',
      title: 'Open Run Log',
      arguments: [this],
    };
  }
}

class EmptyItem extends vscode.TreeItem {
  constructor(message: string) {
    super(message, vscode.TreeItemCollapsibleState.None);
    this.iconPath = new vscode.ThemeIcon('info');
    this.contextValue = 'ailRunEmpty';
  }
}

// ── Provider ──────────────────────────────────────────────────────────────────

export class RunHistoryProvider implements vscode.TreeDataProvider<RunItem | EmptyItem> {
  private _onDidChangeTreeData = new vscode.EventEmitter<undefined>();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

  private _items: (RunItem | EmptyItem)[] = [new EmptyItem('No runs yet')];

  constructor(
    private _binaryPath: string,
    private readonly _cwd?: string,
    private readonly _runLogFn: RunLogFn = defaultRunLogFn
  ) {}

  setBinaryPath(binaryPath: string): void {
    this._binaryPath = binaryPath;
  }

  refresh(): void {
    if (!this._binaryPath) return;
    void this._load();
  }

  getTreeItem(element: RunItem | EmptyItem): vscode.TreeItem {
    return element;
  }

  getChildren(): (RunItem | EmptyItem)[] {
    return this._items;
  }

  async openRunLog(item: RunItem): Promise<void> {
    let content: string;
    try {
      content = await item.runLogFn!(item.binaryPath, ['log', item.runId], item.cwd);
    } catch (err) {
      void vscode.window.showErrorMessage(`Failed to load run log: ${String(err)}`);
      return;
    }
    const doc = await vscode.workspace.openTextDocument({ content, language: 'markdown' });
    await vscode.window.showTextDocument(doc);
  }

  private async _load(): Promise<void> {
    let raw: string;
    try {
      raw = await this._runLogFn(this._binaryPath, ['logs', '--format', 'json', '--limit', '100'], this._cwd);
    } catch {
      this._items = [new EmptyItem('Failed to load run history')];
      this._onDidChangeTreeData.fire(undefined);
      return;
    }

    const sessions: SessionSummary[] = raw
      .split('\n')
      .filter(Boolean)
      .map((line) => {
        try { return JSON.parse(line) as SessionSummary; } catch { return null; }
      })
      .filter((s): s is SessionSummary => s !== null);

    if (sessions.length === 0) {
      this._items = [new EmptyItem('No runs yet — run a pipeline to see history')];
    } else {
      this._items = sessions.map(
        (s) =>
          new RunItem(
            s.run_id,
            invocationPrompt(s),
            s.started_at ?? 0,
            this._binaryPath,
            this._cwd,
            this._runLogFn
          )
      );
    }
    this._onDidChangeTreeData.fire(undefined);
  }
}

export function registerRunLogCommand(
  context: vscode.ExtensionContext,
  provider: RunHistoryProvider
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand('ail-chat.openRunLog', (item: RunItem) => {
      void provider.openRunLog(item);
    })
  );
}

