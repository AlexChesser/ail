/**
 * Pipeline Explorer — flat list of discovered .ail.yaml files.
 *
 * One item per pipeline. The active pipeline is marked with a filled circle.
 * A "Browse…" item at the bottom opens a file picker for pipelines not in the workspace.
 * Clicking a pipeline sets it as active. Context menu: Open, Validate, Run.
 */

import * as vscode from "vscode";
import * as path from "path";
import { ResolvedBinary } from "../binary";
import { discoverPipelines, pipelineLabel } from "../pipeline";
import { getActivePipeline, setActivePipeline, onDidChangeActivePipeline } from "../state";

// ── Tree item types ───────────────────────────────────────────────────────────

export class PipelineItem extends vscode.TreeItem {
  constructor(
    public readonly filePath: string,
    active: boolean
  ) {
    const label = pipelineLabel(filePath);
    super(label, vscode.TreeItemCollapsibleState.None);
    this.resourceUri = vscode.Uri.file(filePath);
    this.tooltip = filePath;
    this.contextValue = active ? "ailActivePipeline" : "ailPipeline";

    if (active) {
      this.iconPath = new vscode.ThemeIcon(
        "pass-filled",
        new vscode.ThemeColor("charts.green")
      );
      this.description = "active";
    } else {
      this.iconPath = new vscode.ThemeIcon("circle-large-outline");
    }

    this.command = {
      command: "ail.setActivePipeline",
      title: "Set as Active Pipeline",
      arguments: [filePath],
    };
  }
}

class BrowseItem extends vscode.TreeItem {
  constructor() {
    super("Browse for pipeline…", vscode.TreeItemCollapsibleState.None);
    this.iconPath = new vscode.ThemeIcon("folder-opened");
    this.tooltip = "Pick a .ail.yaml file from disk";
    this.contextValue = "ailBrowse";
    this.command = {
      command: "ail.browsePipeline",
      title: "Browse for Pipeline",
    };
  }
}

class CreatePipelineItem extends vscode.TreeItem {
  constructor() {
    super("Create a pipeline…", vscode.TreeItemCollapsibleState.None);
    this.iconPath = new vscode.ThemeIcon("add");
    this.tooltip = "Create a new .ail.yaml pipeline from a template";
    this.contextValue = "ailCreatePipeline";
    this.command = {
      command: "ail.createPipeline",
      title: "Create Pipeline from Template",
    };
  }
}

// ── Provider ─────────────────────────────────────────────────────────────────

export class PipelineTreeProvider
  implements vscode.TreeDataProvider<PipelineItem | BrowseItem | CreatePipelineItem>
{
  private _onDidChangeTreeData = new vscode.EventEmitter<
    PipelineItem | BrowseItem | CreatePipelineItem | undefined | null
  >();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

  private _pipelines: string[] = [];
  private _extraPipelines: string[] = []; // added via Browse

  constructor(
    private readonly binary: ResolvedBinary,
    subscriptions: vscode.Disposable[]
  ) {
    // Refresh when the active pipeline changes (to update the active marker)
    subscriptions.push(
      onDidChangeActivePipeline(() => this._onDidChangeTreeData.fire(undefined))
    );
  }

  refresh(): void {
    this._pipelines = discoverPipelines();
    this._onDidChangeTreeData.fire(undefined);
  }

  addExtraPipeline(filePath: string): void {
    if (!this._extraPipelines.includes(filePath)) {
      this._extraPipelines.push(filePath);
    }
    this._onDidChangeTreeData.fire(undefined);
  }

  getTreeItem(element: PipelineItem | BrowseItem | CreatePipelineItem): vscode.TreeItem {
    return element;
  }

  getChildren(): (PipelineItem | BrowseItem | CreatePipelineItem)[] {
    if (this._pipelines.length === 0) {
      this._pipelines = discoverPipelines();
    }

    const active = getActivePipeline();
    const all = [...this._pipelines];

    // Merge in any extra pipelines added via Browse that aren't already discovered
    for (const extra of this._extraPipelines) {
      if (!all.includes(extra)) all.push(extra);
    }

    const items: (PipelineItem | BrowseItem | CreatePipelineItem)[] = all.map(
      (p) => new PipelineItem(p, p === active)
    );
    if (all.length === 0) {
      items.push(new CreatePipelineItem());
    }
    items.push(new BrowseItem());
    return items;
  }
}

// ── Registration ──────────────────────────────────────────────────────────────

export function registerPipelineExplorer(
  context: vscode.ExtensionContext,
  binary: ResolvedBinary
): PipelineTreeProvider {
  const provider = new PipelineTreeProvider(binary, context.subscriptions);

  const treeView = vscode.window.createTreeView("ail.pipelineExplorer", {
    treeDataProvider: provider,
    showCollapseAll: false,
  });
  context.subscriptions.push(treeView);

  // Refresh on .ail.yaml create/delete
  const watcher = vscode.workspace.createFileSystemWatcher("**/.ail.yaml", false, false, false);
  watcher.onDidCreate(() => provider.refresh());
  watcher.onDidDelete(() => provider.refresh());
  context.subscriptions.push(watcher);

  context.subscriptions.push(
    vscode.commands.registerCommand("ail.refreshExplorer", () => provider.refresh())
  );

  // Set active pipeline
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "ail.setActivePipeline",
      (filePath: string) => {
        const current = getActivePipeline();
        if (current === filePath) {
          // Already active — open the file
          void vscode.window.showTextDocument(vscode.Uri.file(filePath));
        } else {
          setActivePipeline(filePath);
        }
      }
    )
  );

  // Browse for pipeline
  context.subscriptions.push(
    vscode.commands.registerCommand("ail.browsePipeline", async () => {
      const uris = await vscode.window.showOpenDialog({
        canSelectFiles: true,
        canSelectFolders: false,
        canSelectMany: false,
        filters: { "AIL Pipelines": ["yaml", "yml"] },
        title: "Select an AIL pipeline file",
        openLabel: "Set as Active",
      });
      if (uris?.[0]) {
        const filePath = uris[0].fsPath;
        provider.addExtraPipeline(filePath);
        setActivePipeline(filePath);
      }
    })
  );

  // Open from context menu
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "ail.openPipelineFile",
      async (item: PipelineItem) => {
        await vscode.window.showTextDocument(vscode.Uri.file(item.filePath));
      }
    )
  );

  // Run from context menu
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "ail.runFromTree",
      async (item: PipelineItem) => {
        setActivePipeline(item.filePath);
        await vscode.commands.executeCommand("ail.runPipeline");
      }
    )
  );

  // Validate from context menu
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
