import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import { execFile } from 'child_process';
import { promisify } from 'util';
import { resolveBinary } from './binary';
import { ChatViewProvider } from './chat-view-provider';

const execFileAsync = promisify(execFile);

const DISMISSED_KEY = 'ail-chat.installPromptDismissed';

export const PIPELINE_PATTERNS = ['.ail.yaml', '.ail.yml'];
export const AIL_DIR_PATTERNS = ['.ail'];

const TEMPLATES = [
  {
    label: '① Starter',
    description: 'Invocation-only pipeline (recommended)',
    detail: 'Single explicit invocation step. Heavily commented so you see exactly what AIL does.',
    templateName: 'starter',
  },
  {
    label: '② Oh My AIL',
    description: 'Intent-routed multi-agent orchestration',
    detail: 'Sisyphus classifier (TRIVIAL/EXPLICIT/EXPLORATORY/AMBIGUOUS) + full agent/workflow tree.',
    templateName: 'oh-my-ail',
  },
  {
    label: '③ Superpowers',
    description: 'Curated high-leverage workflows',
    detail: 'TDD, code-review, planning, brainstorming, parallel debug, plan execution, and more.',
    templateName: 'superpowers',
  },
  {
    label: '$(x) Dismiss',
    description: "Don't ask again for this workspace",
    detail: '',
    templateName: '',
  },
] as const;

export function pipelineExistsInWorkspace(cwd: string): boolean {
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

export async function runInstallWizard(
  context: vscode.ExtensionContext,
  chatProvider: ChatViewProvider,
  options?: { bypassDismiss?: boolean }
): Promise<void> {
  if (!options?.bypassDismiss && context.workspaceState.get<boolean>(DISMISSED_KEY)) return;

  const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  if (!cwd) return;

  if (pipelineExistsInWorkspace(cwd)) return;

  const picked = await vscode.window.showQuickPick(
    TEMPLATES.map((t) => ({ label: t.label, description: t.description, detail: t.detail, templateName: t.templateName })),
    {
      title: 'Set up an AIL pipeline for this workspace',
      placeHolder: 'Choose a template to get started, or dismiss',
      matchOnDescription: true,
    }
  );

  if (!picked) return; // Escape — do not set flag, re-prompt next time

  if (picked.templateName === '') {
    // Dismiss item
    await context.workspaceState.update(DISMISSED_KEY, true);
    return;
  }

  let binaryPath: string;
  try {
    const binary = await resolveBinary(context);
    binaryPath = binary.path;
  } catch {
    return; // resolveBinary already showed the error
  }

  try {
    await execFileAsync(binaryPath, ['init', picked.templateName], { cwd });
  } catch (err) {
    const stderr = (err as { stderr?: string }).stderr ?? String(err);
    void vscode.window.showErrorMessage(`AIL: ail init failed — ${stderr.trim()}`);
    return;
  }

  chatProvider.reloadPipeline();

  const readmePath = path.join(cwd, '.ail', 'README.md');
  if (fs.existsSync(readmePath)) {
    void vscode.commands.executeCommand('markdown.showPreview', vscode.Uri.file(readmePath));
  }
}

export async function checkAndOfferInstall(
  context: vscode.ExtensionContext,
  chatProvider: ChatViewProvider
): Promise<void> {
  return runInstallWizard(context, chatProvider);
}
