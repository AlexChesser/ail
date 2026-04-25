/**
 * ail Chat VS Code extension — entry point.
 *
 * Activates on workspaces with .ail.yaml files, or when the sidebar view is opened.
 * Registers the ChatViewProvider as a dockable sidebar WebviewView.
 */

import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import { clearBinaryCache, resolveBinary, ResolvedBinary } from './binary';
import { checkForLatestRelease, compareVersions } from './update-checker';
import {
  getMinVersionState,
  isAcceptable,
  MinVersionStatusItem,
  setUseAnyway,
  MinVersionState,
} from './min-version-gate';
import { SessionManager } from './session-manager';
import { ChatViewProvider } from './chat-view-provider';
import { PipelineGraphPanel } from './pipeline-graph/PipelineGraphPanel';
import { AilOutputChannel } from './output-channel';
import { runInstallWizard } from './install-wizard';
import { clearPathCache } from './path-detection';
import { RunHistoryProvider, registerRunLogCommand } from './history-tree-provider';
import { PipelineStepsProvider } from './steps-tree-provider';
import { createBinaryInstaller } from './platforms';

let chatProvider: ChatViewProvider | undefined;
let minVersionState: MinVersionState | undefined;
let minVersionStatusItem: MinVersionStatusItem | undefined;

export function activate(context: vscode.ExtensionContext): void {
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration('ail-chat.binaryPath')) {
        clearBinaryCache();
      }
    })
  );

  const sessionManager = new SessionManager(context);
  const rawChannel = vscode.window.createOutputChannel('AIL');
  context.subscriptions.push(rawChannel);
  const outputChannel = new AilOutputChannel(rawChannel);

  const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;

  const historyProvider = new RunHistoryProvider('', cwd);
  const stepsProvider = new PipelineStepsProvider();

  minVersionStatusItem = new MinVersionStatusItem();
  context.subscriptions.push(minVersionStatusItem);

  // Resolve binary path lazily for tree providers (falls back gracefully if binary not found)
  void resolveBinary(context).then((b) => {
    historyProvider.setBinaryPath(b.path);
    historyProvider.refresh();
    void onBinaryResolved(context, b);
  }).catch(() => { /* binary not found — history tree stays empty */ });

  context.subscriptions.push(
    vscode.window.registerTreeDataProvider('ail-chat.historyView', historyProvider),
    vscode.window.registerTreeDataProvider('ail-chat.stepsView', stepsProvider)
  );

  registerRunLogCommand(context, historyProvider);

  context.subscriptions.push(
    vscode.commands.registerCommand('ail-chat.openStep', (item) => {
      stepsProvider.openStep(item);
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ail-chat.checkForUpdates', async () => {
      const binary = await resolveBinary(context).catch(() => undefined);
      const release = await checkForLatestRelease(context, { force: true });
      if (!release) {
        void vscode.window.showInformationMessage(
          'No ail release information available right now. Check your network connection or try again later.'
        );
        return;
      }
      if (binary && compareVersions(release.version, binary.version) > 0) {
        void promptUpdateAvailable(release.version, binary.version);
      } else {
        void vscode.window.showInformationMessage(
          `ail is up to date (running ${binary?.version ?? 'unknown'}; latest release ${release.version}).`
        );
      }
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ail-chat.installBinary', async () => {
      try {
        const binary = await resolveBinary(context);
        const installer = createBinaryInstaller();
        const result = await installer.install(binary.path);
        clearPathCache();
        // Re-resolve so subsequent gate checks see the freshly-installed binary.
        clearBinaryCache();
        await refreshMinVersionGate(context);
        void vscode.window.showInformationMessage(result.message);
        if (chatProvider) void chatProvider.sendSetupStatus();
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : String(err);
        void vscode.window.showErrorMessage(`Failed to install ail binary: ${errorMsg}`);
      }
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ail-chat.useAnyway', async () => {
      const currentlyActive = minVersionState?.useAnywayActive ?? false;
      if (currentlyActive) {
        const choice = await vscode.window.showWarningMessage(
          'Disable the "Use Anyway" override? The extension will refuse to start sessions with an outdated ail binary until you install an update.',
          { modal: true },
          'Disable'
        );
        if (choice === 'Disable') {
          await setUseAnyway(context, false);
          await refreshMinVersionGate(context);
          void vscode.window.showInformationMessage('"Use Anyway" override disabled.');
        }
      } else {
        const choice = await vscode.window.showWarningMessage(
          'Bypass the minimum-version check? The extension may not work correctly with an outdated ail binary, and features may fail unpredictably.',
          { modal: true, detail: `Required minimum: ${minVersionState?.minVersion ?? 'unknown'}\nResolved binary: ${minVersionState?.resolvedVersion ?? 'unknown'}` },
          'Use Anyway'
        );
        if (choice === 'Use Anyway') {
          await setUseAnyway(context, true);
          await refreshMinVersionGate(context);
          void vscode.window.showInformationMessage(
            '"Use Anyway" override enabled. Run the same command to disable it later.'
          );
        }
      }
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ail-chat.runInstallWizard', async () => {
      if (!(await ensureMinVersionGate(context))) return;
      if (chatProvider) {
        await runInstallWizard(context, chatProvider, { bypassDismiss: true });
        void chatProvider.sendSetupStatus();
      }
    })
  );

  let panelVisible = false;
  context.subscriptions.push(
    vscode.commands.registerCommand('ail-chat.toggleInfoPanel', () => {
      panelVisible = !panelVisible;
      void vscode.commands.executeCommand('setContext', 'ail-chat.panelVisible', panelVisible);
    })
  );

  chatProvider = new ChatViewProvider(context, sessionManager, outputChannel, historyProvider, stepsProvider);

  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(ChatViewProvider.viewId, chatProvider, {
      webviewOptions: { retainContextWhenHidden: true },
    }),

    vscode.commands.registerCommand('ail-chat.open', () => {
      void vscode.commands.executeCommand('workbench.view.extension.ail-chat-sidebar');
    }),

    vscode.commands.registerCommand('ail-chat.newSession', async () => {
      if (!(await ensureMinVersionGate(context))) return;
      chatProvider?.reveal();
    }),

    vscode.commands.registerCommand('ail-chat.openPipelineGraph', async () => {
      if (!(await ensureMinVersionGate(context))) return;
      // Pipeline resolution order:
      // 1. Active editor (if it's a .ail.yaml file)
      // 2. File picker dialog
      const activeEditor = vscode.window.activeTextEditor;
      const activeFile = activeEditor?.document.uri.fsPath;
      const isYaml = activeFile && /\.ail\.ya?ml$/i.test(activeFile);

      let pipelinePath: string | undefined;
      if (isYaml) {
        pipelinePath = activeFile;
      } else {
        // Check workspace root for .ail.yaml
        const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
        if (cwd) {
          const candidate = path.join(cwd, '.ail.yaml');
          if (fs.existsSync(candidate)) {
            pipelinePath = candidate;
          }
        }
      }

      if (!pipelinePath) {
        const uris = await vscode.window.showOpenDialog({
          canSelectMany: false,
          canSelectFolders: false,
          filters: { 'ail Pipeline': ['yaml', 'yml'] },
          title: 'Select ail pipeline file to visualize',
          openLabel: 'Open Pipeline Graph',
        });
        if (uris && uris.length > 0) {
          pipelinePath = uris[0].fsPath;
        }
      }

      if (pipelinePath) {
        PipelineGraphPanel.show(context.extensionPath, pipelinePath);
      }
    }),

    vscode.workspace.onDidChangeWorkspaceFolders(() => {
      if (chatProvider) void chatProvider.sendSetupStatus();
    })
  );
}

/**
 * Run after a successful resolveBinary() during activation.
 * - Compute and surface the minimum-version gate state (status bar + toast if below min).
 * - If using bundled fallback, surface an informational toast inviting installation to PATH.
 * - Run a throttled update check; if a newer ail-v* release is available, prompt to update.
 */
async function onBinaryResolved(
  context: vscode.ExtensionContext,
  binary: ResolvedBinary
): Promise<void> {
  minVersionState = getMinVersionState(context, binary.version);
  minVersionStatusItem?.update(minVersionState);

  if (!isAcceptable(minVersionState)) {
    void promptBelowMinVersion(minVersionState);
  }

  if (binary.source === 'bundled') {
    void promptInstallBundledFallback();
  }

  const release = await checkForLatestRelease(context);
  if (release && compareVersions(release.version, binary.version) > 0) {
    void promptUpdateAvailable(release.version, binary.version);
  }
}

/**
 * Recompute the gate state (call after binary install or override toggle).
 * Idempotent — safe to call as often as needed.
 */
async function refreshMinVersionGate(context: vscode.ExtensionContext): Promise<void> {
  const binary = await resolveBinary(context).catch(() => undefined);
  if (!binary) return;
  minVersionState = getMinVersionState(context, binary.version);
  minVersionStatusItem?.update(minVersionState);
}

/**
 * Gate for session-starting commands. Returns true if execution should
 * proceed; surfaces an actionable error toast and returns false otherwise.
 */
async function ensureMinVersionGate(context: vscode.ExtensionContext): Promise<boolean> {
  // If we haven't resolved a binary yet, refresh once.
  if (!minVersionState) {
    await refreshMinVersionGate(context);
  }
  if (!minVersionState || isAcceptable(minVersionState)) return true;

  const choice = await vscode.window.showErrorMessage(
    `ail v${minVersionState.resolvedVersion} is below the minimum supported version (${minVersionState.minVersion}). Install an update to continue.`,
    'Install Update',
    'Use Anyway',
    'Cancel'
  );
  if (choice === 'Install Update') {
    void vscode.commands.executeCommand('ail-chat.installBinary');
  } else if (choice === 'Use Anyway') {
    void vscode.commands.executeCommand('ail-chat.useAnyway');
  }
  return false;
}

async function promptBelowMinVersion(state: MinVersionState): Promise<void> {
  const choice = await vscode.window.showErrorMessage(
    `ail v${state.resolvedVersion} is below the minimum supported version (${state.minVersion}). Sessions are blocked until you install an update.`,
    'Install Update',
    'Use Anyway',
    'Dismiss'
  );
  if (choice === 'Install Update') {
    void vscode.commands.executeCommand('ail-chat.installBinary');
  } else if (choice === 'Use Anyway') {
    void vscode.commands.executeCommand('ail-chat.useAnyway');
  }
}

async function promptInstallBundledFallback(): Promise<void> {
  const choice = await vscode.window.showInformationMessage(
    'ail is not on your PATH — using the bundled fallback. Install to PATH for command-line use?',
    'Install',
    'Not Now'
  );
  if (choice === 'Install') {
    void vscode.commands.executeCommand('ail-chat.installBinary');
  }
}

async function promptUpdateAvailable(latest: string, current: string): Promise<void> {
  const choice = await vscode.window.showInformationMessage(
    `ail v${latest} is available (you have ${current}). Install update?`,
    'Install Update',
    'Release Notes',
    'Not Now'
  );
  if (choice === 'Install Update') {
    void vscode.commands.executeCommand('ail-chat.installBinary');
  } else if (choice === 'Release Notes') {
    void vscode.env.openExternal(
      vscode.Uri.parse(`https://github.com/AlexChesser/ail/releases/tag/ail-v${latest}`)
    );
  }
}

// Process manager cleanup is handled by the view's onDidDispose.
export function deactivate(): void {}
