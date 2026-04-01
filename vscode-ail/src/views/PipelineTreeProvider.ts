/**
 * Pipeline Explorer — sidebar tree view.
 *
 * Shows discovered .ail.yaml files in the workspace and their steps.
 * Context menu actions: Run, Validate, Open File.
 */

import * as vscode from "vscode";
import * as fs from "fs";
import * as path from "path";
import { execFile } from "child_process";
import { ResolvedBinary } from "../binary";

// ── Tree Item types ──────────────────────────────────────────────────────────

export class PipelineItem extends vscode.TreeItem {
  constructor(
    public readonly filePath: string,
    public readonly label: string
  ) {
    super(label, vscode.TreeItemCollapsibleState.Collapsed);
    this.resourceUri = vscode.Uri.file(filePath);
    this.tooltip = filePath;
    this.contextValue = "ailPipeline";
    this.iconPath = new vscode.ThemeIcon("symbol-file");
    this.command = {
      command: "vscode.open",
      title: "Open Pipeline",
      arguments: [vscode.Uri.file(filePath)],
    };
  }
}

export class StepItem extends vscode.TreeItem {
  constructor(
    public readonly stepId: string,
    public readonly stepType: string,
    public readonly pipelinePath: string,
    public readonly stepLine: number
  ) {
    super(stepId, vscode.TreeItemCollapsibleState.None);
    this.description = stepType;
    this.tooltip = `${stepType}: ${stepId}`;
    this.contextValue = "ailStep";
    this.iconPath = stepTypeIcon(stepType);
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

function stepTypeIcon(stepType: string): vscode.ThemeIcon {
  switch (stepType) {
    case "prompt":
      return new vscode.ThemeIcon("comment");
    case "context":
      return new vscode.ThemeIcon("terminal");
    case "action":
      return new vscode.ThemeIcon("debug-pause");
    case "pipeline":
      return new vscode.ThemeIcon("references");
    default:
      return new vscode.ThemeIcon("symbol-misc");
  }
}

// ── Step parsing ─────────────────────────────────────────────────────────────

interface ParsedStep {
  id: string;
  type: string;
  line: number;
}

function parseStepsFromYaml(filePath: string): ParsedStep[] {
  let content: string;
  try {
    content = fs.readFileSync(filePath, "utf-8");
  } catch {
    return [];
  }

  const steps: ParsedStep[] = [];
  const lines = content.split("\n");

  // Simple line-based parser — finds `- id:` blocks under `pipeline:`.
  // Good enough for the tree view without pulling in a YAML parser.
  let inPipeline = false;
  let currentId: string | undefined;
  let currentLine = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    if (trimmed === "pipeline:") {
      inPipeline = true;
      continue;
    }

    if (inPipeline) {
      // Top-level keys after pipeline: (no indentation) end the pipeline block
      if (/^\S/.test(line) && trimmed !== "") {
        inPipeline = false;
        continue;
      }

      const idMatch = trimmed.match(/^-?\s*id:\s*(.+)/);
      if (idMatch) {
        currentId = idMatch[1].trim().replace(/['"]/g, "");
        currentLine = i;
      }

      if (currentId) {
        const promptMatch = trimmed.match(/^prompt:/);
        const contextMatch = trimmed.match(/^context:/);
        const actionMatch = trimmed.match(/^action:/);
        const pipelineMatch = trimmed.match(/^pipeline:/);

        let type: string | undefined;
        if (promptMatch) type = "prompt";
        else if (contextMatch) type = "context";
        else if (actionMatch) type = "action";
        else if (pipelineMatch) type = "pipeline";

        if (type) {
          steps.push({ id: currentId, type, line: currentLine });
          currentId = undefined;
        }
      }
    }
  }

  return steps;
}

// ── Tree data provider ───────────────────────────────────────────────────────

export class PipelineTreeProvider
  implements vscode.TreeDataProvider<PipelineItem | StepItem>
{
  private _onDidChangeTreeData = new vscode.EventEmitter<
    PipelineItem | StepItem | undefined | null
  >();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

  private _pipelines: PipelineItem[] = [];

  constructor(private readonly binary: ResolvedBinary) {}

  refresh(): void {
    this._pipelines = this._discoverPipelines();
    this._onDidChangeTreeData.fire(undefined);
  }

  getTreeItem(element: PipelineItem | StepItem): vscode.TreeItem {
    return element;
  }

  getChildren(
    element?: PipelineItem | StepItem
  ): (PipelineItem | StepItem)[] {
    if (!element) {
      // Root: return pipelines
      if (this._pipelines.length === 0) {
        this._pipelines = this._discoverPipelines();
      }
      return this._pipelines;
    }

    if (element instanceof PipelineItem) {
      return this._stepsForPipeline(element.filePath);
    }

    return [];
  }

  private _discoverPipelines(): PipelineItem[] {
    const pipelines: PipelineItem[] = [];
    const folders = vscode.workspace.workspaceFolders;
    if (!folders) return pipelines;

    for (const folder of folders) {
      const candidates = [
        path.join(folder.uri.fsPath, ".ail.yaml"),
        path.join(folder.uri.fsPath, ".ail.yml"),
      ];

      // Also search one level deep
      try {
        const entries = fs.readdirSync(folder.uri.fsPath, { withFileTypes: true });
        for (const entry of entries) {
          if (entry.isDirectory() && entry.name === ".ail") {
            const subdir = path.join(folder.uri.fsPath, ".ail");
            try {
              const subEntries = fs.readdirSync(subdir, { withFileTypes: true });
              for (const sub of subEntries) {
                if (sub.isFile() && (sub.name.endsWith(".yaml") || sub.name.endsWith(".yml"))) {
                  candidates.push(path.join(subdir, sub.name));
                }
              }
            } catch { /* ignore */ }
          }
          if (
            entry.isFile() &&
            (entry.name.endsWith(".ail.yaml") || entry.name.endsWith(".ail.yml"))
          ) {
            candidates.push(path.join(folder.uri.fsPath, entry.name));
          }
        }
      } catch { /* ignore */ }

      // Deduplicate and check existence
      const seen = new Set<string>();
      for (const filePath of candidates) {
        if (!seen.has(filePath) && fs.existsSync(filePath)) {
          seen.add(filePath);
          const label = path.relative(folder.uri.fsPath, filePath);
          pipelines.push(new PipelineItem(filePath, label));
        }
      }
    }

    return pipelines;
  }

  private _stepsForPipeline(filePath: string): StepItem[] {
    const parsed = parseStepsFromYaml(filePath);
    return parsed.map((s) => new StepItem(s.id, s.type, filePath, s.line));
  }
}

// ── Register tree view ───────────────────────────────────────────────────────

export function registerPipelineExplorer(
  context: vscode.ExtensionContext,
  binary: ResolvedBinary
): PipelineTreeProvider {
  const provider = new PipelineTreeProvider(binary);

  const treeView = vscode.window.createTreeView("ail.pipelineExplorer", {
    treeDataProvider: provider,
    showCollapseAll: true,
  });

  context.subscriptions.push(treeView);

  // Refresh on workspace file changes
  const watcher = vscode.workspace.createFileSystemWatcher(
    "**/.ail.yaml",
    false,
    false,
    false
  );
  watcher.onDidCreate(() => provider.refresh());
  watcher.onDidDelete(() => provider.refresh());
  watcher.onDidChange(() => provider.refresh());
  context.subscriptions.push(watcher);

  // Register refresh command
  context.subscriptions.push(
    vscode.commands.registerCommand("ail.refreshExplorer", () => provider.refresh())
  );

  // Run from tree context menu
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "ail.runFromTree",
      async (item: PipelineItem) => {
        // Open the file first, then trigger run
        await vscode.window.showTextDocument(vscode.Uri.file(item.filePath));
        await vscode.commands.executeCommand("ail.runPipeline");
      }
    )
  );

  // Validate from tree context menu
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "ail.validateFromTree",
      async (item: PipelineItem) => {
        await vscode.window.showTextDocument(vscode.Uri.file(item.filePath));
        await vscode.commands.executeCommand("ail.validatePipeline");
      }
    )
  );

  return provider;
}
