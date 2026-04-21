/**
 * Steps view — shows pipeline steps for the active pipeline.
 *
 * Step status uses glyphs from the TUI visual language spec:
 *   ○  not yet reached
 *   ●  active / running
 *   ✓  completed (success)
 *   ✗  failed
 *   ⊘  skipped
 *   ⊖  disabled (user)
 *   ◉  paused (HITL gate)
 *
 * Static steps show ○. Status is updated via setStepStatus() during execution.
 * Hot-reloads when the active pipeline file changes on disk.
 */

import * as vscode from "vscode";
import { getActivePipeline, onDidChangeActivePipeline } from "../state";
import { parseStepsFromYaml, stepTypeIcon } from "../pipeline";

// ── Step status ───────────────────────────────────────────────────────────────

export type StepStatus =
  | "pending"     // ○ not yet reached
  | "running"     // ●
  | "completed"   // ✓
  | "failed"      // ✗
  | "skipped"     // ⊘
  | "disabled"    // ⊖
  | "hitl";       // ◉

function statusGlyph(s: StepStatus): string {
  switch (s) {
    case "pending":   return "○";
    case "running":   return "●";
    case "completed": return "✓";
    case "failed":    return "✗";
    case "skipped":   return "⊘";
    case "disabled":  return "⊖";
    case "hitl":      return "◉";
  }
}

function statusIcon(s: StepStatus): vscode.ThemeIcon {
  switch (s) {
    case "pending":   return new vscode.ThemeIcon("circle-large-outline");
    case "running":   return new vscode.ThemeIcon("loading~spin", new vscode.ThemeColor("charts.blue"));
    case "completed": return new vscode.ThemeIcon("pass", new vscode.ThemeColor("charts.green"));
    case "failed":    return new vscode.ThemeIcon("error", new vscode.ThemeColor("charts.red"));
    case "skipped":   return new vscode.ThemeIcon("circle-slash", new vscode.ThemeColor("charts.yellow"));
    case "disabled":  return new vscode.ThemeIcon("circle-slash", new vscode.ThemeColor("disabledForeground"));
    case "hitl":      return new vscode.ThemeIcon("debug-breakpoint", new vscode.ThemeColor("charts.yellow"));
  }
}

// ── Tree item ─────────────────────────────────────────────────────────────────

export class StepItem extends vscode.TreeItem {
  constructor(
    public readonly stepId: string,
    public readonly stepType: string,
    public readonly pipelinePath: string,
    public readonly stepLine: number,
    public readonly status: StepStatus
  ) {
    super(stepId, vscode.TreeItemCollapsibleState.None);
    this.description = `${statusGlyph(status)}  ${stepType}`;
    this.tooltip = `${stepType}: ${stepId} [${status}]`;
    this.contextValue = "ailStep";
    this.iconPath = status === "pending" ? stepTypeIcon(stepType) : statusIcon(status);
    this.command = {
      command: "vscode.open",
      title: "Go to Step",
      arguments: [
        vscode.Uri.file(pipelinePath),
        { selection: new vscode.Range(stepLine, 0, stepLine, 0) },
      ],
    };
  }
}

class EmptyItem extends vscode.TreeItem {
  constructor(message: string) {
    super(message, vscode.TreeItemCollapsibleState.None);
    this.iconPath = new vscode.ThemeIcon("info");
    this.contextValue = "ailStepsEmpty";
  }
}

// ── Provider ──────────────────────────────────────────────────────────────────

export class StepsTreeProvider implements vscode.TreeDataProvider<StepItem | EmptyItem> {
  private _onDidChangeTreeData = new vscode.EventEmitter<
    StepItem | EmptyItem | undefined | null
  >();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

  private _stepStatuses = new Map<string, StepStatus>();
  private _fileWatcher?: vscode.FileSystemWatcher;

  constructor(subscriptions: vscode.Disposable[]) {
    subscriptions.push(
      onDidChangeActivePipeline(() => {
        this._stepStatuses.clear();
        this._rewatchFile();
        this._onDidChangeTreeData.fire(undefined);
      })
    );
  }

  /** Called by run commands to update step state during execution. */
  setStepStatus(stepId: string, status: StepStatus): void {
    this._stepStatuses.set(stepId, status);
    this._onDidChangeTreeData.fire(undefined);
  }

  /** Reset all step statuses (called when a new run starts). */
  resetStatuses(): void {
    this._stepStatuses.clear();
    this._onDidChangeTreeData.fire(undefined);
  }

  getTreeItem(element: StepItem | EmptyItem): vscode.TreeItem {
    return element;
  }

  getChildren(): (StepItem | EmptyItem)[] {
    const active = getActivePipeline();
    if (!active) {
      return [new EmptyItem("No active pipeline — select one above")];
    }

    const steps = parseStepsFromYaml(active);
    if (steps.length === 0) {
      return [new EmptyItem("No steps found in pipeline")];
    }

    return steps.map(
      (s) =>
        new StepItem(
          s.id,
          s.type,
          active,
          s.line,
          this._stepStatuses.get(s.id) ?? "pending"
        )
    );
  }

  private _rewatchFile(): void {
    this._fileWatcher?.dispose();
    const active = getActivePipeline();
    if (!active) return;

    this._fileWatcher = vscode.workspace.createFileSystemWatcher(
      new vscode.RelativePattern(vscode.Uri.file(active), "*"),
      true,
      false,
      true
    );
    this._fileWatcher.onDidChange(() => this._onDidChangeTreeData.fire(undefined));
  }

  dispose(): void {
    this._fileWatcher?.dispose();
  }
}

// ── Registration ──────────────────────────────────────────────────────────────

export function registerStepsView(
  context: vscode.ExtensionContext
): StepsTreeProvider {
  const provider = new StepsTreeProvider(context.subscriptions);

  const treeView = vscode.window.createTreeView("ail.stepsView", {
    treeDataProvider: provider,
    showCollapseAll: false,
  });

  context.subscriptions.push(treeView);
  context.subscriptions.push({ dispose: () => provider.dispose() });

  return provider;
}
