import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import { ChatViewProvider } from './chat-view-provider';

const DISMISSED_KEY = 'ail-chat.installPromptDismissed';

const PIPELINE_PATTERNS = ['.ail.yaml', '.ail.yml'];
const AIL_DIR_PATTERNS = ['.ail'];

const TEMPLATES = [
  {
    label: '① Starter',
    description: 'Invocation-only pipeline (recommended)',
    detail: 'Single explicit invocation step. Heavily commented so you see exactly what AIL does.',
    dir: 'starter',
  },
  {
    label: '② Oh My AIL',
    description: 'Intent-routed multi-agent orchestration',
    detail: 'Sisyphus classifier (TRIVIAL/EXPLICIT/EXPLORATORY/AMBIGUOUS) + full agent/workflow tree.',
    dir: 'oh-my-ail',
  },
  {
    label: '③ Superpowers',
    description: 'Curated high-leverage workflows',
    detail: 'TDD, code-review, planning, brainstorming, parallel debug, plan execution, and more.',
    dir: 'superpowers',
  },
  {
    label: '$(x) Dismiss',
    description: "Don't ask again for this workspace",
    detail: '',
    dir: '',
  },
] as const;

function pipelineExistsInWorkspace(cwd: string): boolean {
  for (const p of PIPELINE_PATTERNS) {
    if (fs.existsSync(path.join(cwd, p))) return true;
  }
  for (const dir of AIL_DIR_PATTERNS) {
    const ailDir = path.join(cwd, dir);
    if (fs.existsSync(ailDir) && fs.statSync(ailDir).isDirectory()) {
      try {
        const entries = fs.readdirSync(ailDir);
        if (entries.some((e) => e.endsWith('.yaml') || e.endsWith('.yml'))) return true;
      } catch {
        // ignore unreadable dirs
      }
    }
  }
  return false;
}

function copyDir(src: string, dest: string): void {
  fs.mkdirSync(dest, { recursive: true });
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const sp = path.join(src, entry.name);
    const dp = path.join(dest, entry.name);
    if (entry.isDirectory()) copyDir(sp, dp);
    else fs.copyFileSync(sp, dp);
  }
}

export async function checkAndOfferInstall(
  context: vscode.ExtensionContext,
  chatProvider: ChatViewProvider
): Promise<void> {
  if (context.workspaceState.get<boolean>(DISMISSED_KEY)) return;

  const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  if (!cwd) return;

  if (pipelineExistsInWorkspace(cwd)) return;

  const picked = await vscode.window.showQuickPick(
    TEMPLATES.map((t) => ({ label: t.label, description: t.description, detail: t.detail, dir: t.dir })),
    {
      title: 'Set up an AIL pipeline for this workspace',
      placeHolder: 'Choose a template to get started, or dismiss',
      matchOnDescription: true,
    }
  );

  if (!picked) return; // Escape — do not set flag, re-prompt next time

  if (picked.dir === '') {
    // Dismiss item
    await context.workspaceState.update(DISMISSED_KEY, true);
    return;
  }

  const templateSrc = path.join(context.extensionPath, 'dist', 'templates', picked.dir);
  if (!fs.existsSync(templateSrc)) {
    void vscode.window.showErrorMessage(`AIL: template "${picked.dir}" not found in extension bundle.`);
    return;
  }

  const dest = path.join(cwd, '.ail');
  try {
    copyDir(templateSrc, dest);
  } catch (err) {
    void vscode.window.showErrorMessage(`AIL: failed to copy template — ${String(err)}`);
    return;
  }

  chatProvider.reloadPipeline();

  const readmePath = path.join(dest, 'README.md');
  if (fs.existsSync(readmePath)) {
    void vscode.commands.executeCommand('markdown.showPreview', vscode.Uri.file(readmePath));
  }
}
