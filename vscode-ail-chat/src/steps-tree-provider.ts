import * as vscode from 'vscode';
import * as fs from 'fs';
import { parse as parseYaml } from 'yaml';

// ── Types ─────────────────────────────────────────────────────────────────────

interface ParsedStep {
  id: string;
  type: string;
  line: number;
}

// ── YAML parsing ──────────────────────────────────────────────────────────────

function stepType(raw: Record<string, unknown>): string {
  if ('prompt' in raw) return 'prompt';
  if ('context' in raw) return 'context';
  if ('skill' in raw) return 'skill';
  if ('pipeline' in raw) return 'sub-pipeline';
  if ('action' in raw) return 'action';
  if ('do_while' in raw) return 'do_while';
  if ('for_each' in raw) return 'for_each';
  return 'step';
}

function stepTypeIcon(type: string): vscode.ThemeIcon {
  switch (type) {
    case 'prompt': return new vscode.ThemeIcon('comment');
    case 'context': return new vscode.ThemeIcon('terminal');
    case 'skill': return new vscode.ThemeIcon('zap');
    case 'sub-pipeline': return new vscode.ThemeIcon('references');
    case 'action': return new vscode.ThemeIcon('play');
    case 'do_while': return new vscode.ThemeIcon('refresh');
    case 'for_each': return new vscode.ThemeIcon('list-ordered');
    default: return new vscode.ThemeIcon('circle-outline');
  }
}

function parseStepsFromYaml(pipelinePath: string): ParsedStep[] {
  let content: string;
  try {
    content = fs.readFileSync(pipelinePath, 'utf8');
  } catch {
    return [];
  }

  let doc: unknown;
  try {
    doc = parseYaml(content);
  } catch {
    return [];
  }

  if (!doc || typeof doc !== 'object') return [];
  const pipeline = (doc as Record<string, unknown>).pipeline;
  if (!Array.isArray(pipeline)) return [];

  const lines = content.split('\n');
  const steps: ParsedStep[] = [];

  for (const raw of pipeline) {
    if (!raw || typeof raw !== 'object') continue;
    const step = raw as Record<string, unknown>;
    const id = typeof step.id === 'string' ? step.id : '(unnamed)';
    const type = stepType(step);

    // For sub-pipeline references, show the filename if available
    const label =
      type === 'sub-pipeline' && typeof step.pipeline === 'string'
        ? `${id} → ${step.pipeline.split('/').pop()}`
        : id;

    // Find the line where this step's id appears
    const lineIdx = lines.findIndex((l) => l.includes(`id: ${id}`) || l.includes(`id: "${id}"`));

    steps.push({ id: label, type, line: lineIdx >= 0 ? lineIdx : 0 });
  }
  return steps;
}

// ── Tree items ────────────────────────────────────────────────────────────────

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
    this.contextValue = 'ailStep';
    this.iconPath = stepTypeIcon(stepType);
    this.command = {
      command: 'ail-chat.openStep',
      title: 'Go to Step',
      arguments: [this],
    };
  }
}

class ErrorItem extends vscode.TreeItem {
  constructor(message: string) {
    super(message, vscode.TreeItemCollapsibleState.None);
    this.iconPath = new vscode.ThemeIcon('info');
    this.contextValue = 'ailStepsError';
  }
}

// ── Provider ──────────────────────────────────────────────────────────────────

export class PipelineStepsProvider implements vscode.TreeDataProvider<StepItem | ErrorItem> {
  private _onDidChangeTreeData = new vscode.EventEmitter<undefined>();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

  private _pipelinePath: string | null = null;

  refresh(pipelinePath: string | null): void {
    this._pipelinePath = pipelinePath;
    this._onDidChangeTreeData.fire(undefined);
  }

  getTreeItem(element: StepItem | ErrorItem): vscode.TreeItem {
    return element;
  }

  getChildren(): (StepItem | ErrorItem)[] {
    if (!this._pipelinePath) {
      return [new ErrorItem('No pipeline loaded')];
    }

    let steps: ParsedStep[];
    try {
      steps = parseStepsFromYaml(this._pipelinePath);
    } catch {
      return [new ErrorItem('Failed to parse pipeline YAML')];
    }

    if (steps.length === 0) {
      return [new ErrorItem('No steps found in pipeline')];
    }

    return steps.map((s) => new StepItem(s.id, s.type, this._pipelinePath!, s.line));
  }

  openStep(item: StepItem): void {
    const uri = vscode.Uri.file(item.pipelinePath);
    void vscode.window.showTextDocument(uri).then((editor) => {
      const pos = new vscode.Position(item.stepLine, 0);
      editor.revealRange(new vscode.Range(pos, pos), vscode.TextEditorRevealType.InCenter);
    });
  }
}

export { parseStepsFromYaml };
