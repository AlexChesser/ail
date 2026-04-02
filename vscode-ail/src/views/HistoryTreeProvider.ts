/**
 * HistoryTreeProvider — sidebar view showing past pipeline runs.
 *
 * Each item shows: [glyph] <relative-time> — <pipeline> ($<cost>)
 * Clicking fires `ail.openHistoryRun` with the runId.
 *
 * Glyphs:
 *   ✓  completed
 *   ✗  failed
 *   ●  unknown / active
 */

import * as vscode from 'vscode';
import { HistoryService, RunRecord, RunOutcome } from '../application/HistoryService';

// ── Tree item ─────────────────────────────────────────────────────────────────

export class HistoryItem extends vscode.TreeItem {
  constructor(public readonly record: RunRecord) {
    const glyph = outcomeGlyph(record.outcome);
    const relTime = relativeTime(record.timestamp);
    const pipeline = pipelineLabel(record.pipelineSource);
    const cost =
      record.totalCostUsd > 0 ? ` ($${record.totalCostUsd.toFixed(4)})` : '';

    super(`${glyph} ${relTime} — ${pipeline}${cost}`, vscode.TreeItemCollapsibleState.None);

    this.tooltip = record.invocationPrompt || record.runId;
    this.description = record.invocationPrompt
      ? truncate(record.invocationPrompt, 60)
      : undefined;
    this.contextValue = 'ailHistoryRun';
    this.iconPath = outcomeIcon(record.outcome);
    this.command = {
      command: 'ail.openHistoryRun',
      title: 'Open Run',
      arguments: [record.runId],
    };
  }
}

class EmptyItem extends vscode.TreeItem {
  constructor(message: string) {
    super(message, vscode.TreeItemCollapsibleState.None);
    this.iconPath = new vscode.ThemeIcon('info');
    this.contextValue = 'ailHistoryEmpty';
  }
}

// ── Provider ──────────────────────────────────────────────────────────────────

export class HistoryTreeProvider implements vscode.TreeDataProvider<HistoryItem | EmptyItem> {
  private _onDidChangeTreeData = new vscode.EventEmitter<
    HistoryItem | EmptyItem | undefined | null
  >();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

  constructor(private readonly _historyService: HistoryService) {}

  /** Force a reload of the history index. */
  refresh(): void {
    this._onDidChangeTreeData.fire(undefined);
  }

  getTreeItem(element: HistoryItem | EmptyItem): vscode.TreeItem {
    return element;
  }

  async getChildren(): Promise<(HistoryItem | EmptyItem)[]> {
    let records: RunRecord[];
    try {
      records = await this._historyService.getHistory();
    } catch {
      return [new EmptyItem('Failed to load run history')];
    }

    if (records.length === 0) {
      return [new EmptyItem('No runs yet — run a pipeline to see history')];
    }

    return records.map((r) => new HistoryItem(r));
  }
}

// ── Registration ──────────────────────────────────────────────────────────────

export function registerHistoryView(
  context: vscode.ExtensionContext,
  historyService: HistoryService
): HistoryTreeProvider {
  const provider = new HistoryTreeProvider(historyService);

  const treeView = vscode.window.createTreeView('ail.historyView', {
    treeDataProvider: provider,
    showCollapseAll: false,
  });
  context.subscriptions.push(treeView);

  context.subscriptions.push(
    vscode.commands.registerCommand('ail.refreshHistory', () => provider.refresh())
  );

  return provider;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function outcomeGlyph(outcome: RunOutcome): string {
  switch (outcome) {
    case 'completed': return '✓';
    case 'failed':    return '✗';
    default:          return '●';
  }
}

function outcomeIcon(outcome: RunOutcome): vscode.ThemeIcon {
  switch (outcome) {
    case 'completed':
      return new vscode.ThemeIcon('pass', new vscode.ThemeColor('charts.green'));
    case 'failed':
      return new vscode.ThemeIcon('error', new vscode.ThemeColor('charts.red'));
    default:
      return new vscode.ThemeIcon('circle-filled');
  }
}

function pipelineLabel(pipelineSource: string): string {
  if (!pipelineSource || pipelineSource === 'unknown') {
    return 'unknown';
  }
  // Extract just the filename portion
  const parts = pipelineSource.replace(/\\/g, '/').split('/');
  return parts[parts.length - 1] ?? pipelineSource;
}

function relativeTime(timestampMs: number): string {
  const now = Date.now();
  const diffMs = now - timestampMs;
  const diffSec = Math.floor(diffMs / 1000);

  if (diffSec < 60) return 'just now';
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m ago`;
  if (diffSec < 86400) return `${Math.floor(diffSec / 3600)}h ago`;
  if (diffSec < 86400 * 7) return `${Math.floor(diffSec / 86400)}d ago`;
  return new Date(timestampMs).toLocaleDateString();
}

function truncate(s: string, maxLen: number): string {
  if (s.length <= maxLen) return s;
  return s.slice(0, maxLen - 1) + '…';
}
